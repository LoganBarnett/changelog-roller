//! High-level operations the library performs against a changelog file:
//! `ready_to_roll`, `check_additions`, `roll`, `insert_item`.  Each takes
//! the arguments the operation needs and returns a rich outcome that
//! callers can pattern-match on.  These wrap the pure string-in /
//! string-out helpers in [`crate::roller`] with file I/O and (for
//! `check_additions`) a git invocation; they happen to align one-to-one
//! with the CLI's subcommands, but nothing about them is CLI-specific.

use std::{
  fs,
  path::{Path, PathBuf},
  process::Command,
};

use thiserror::Error;

use crate::roller::{self, has_section_additions, is_ready_to_roll, RollError};

/// Errors surfaced by the operations in this module.  Anything an
/// operation can recover from (such as a missing upcoming heading) is
/// expressed as an outcome variant instead — the variants here are real
/// failures: I/O problems, git invocation problems, encoding problems.
#[derive(Debug, Error)]
pub enum OperationError {
  #[error("Failed to read changelog at {path:?}: {source}")]
  ReadChangelog {
    path: PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("Failed to write changelog at {path:?}: {source}")]
  WriteChangelog {
    path: PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("Failed to run git: {0}")]
  GitRun(std::io::Error),

  #[error("git show {git_ref}:{path} failed: {stderr}")]
  GitShow {
    git_ref: String,
    path: String,
    stderr: String,
  },

  #[error("git output is not valid UTF-8: {0}")]
  GitOutputEncoding(#[from] std::string::FromUtf8Error),
}

/// Result of a `ready-to-roll` CI gate check.
#[derive(Debug)]
pub enum ReadyToRollOutcome {
  /// The target section has at least one populated subsection.
  Ready,
  /// The target section exists but all its subsections are empty.
  NoChanges,
  /// No heading matching a segment of the requested path was found.
  HeadingNotFound { heading: String },
}

/// Result of a `check-additions` diff against a git ref.
#[derive(Debug)]
pub enum CheckAdditionsOutcome {
  HasAdditions,
  NoAdditions,
}

/// Result of a changelog-mutating operation (`roll`, `insert-item`).
#[derive(Debug)]
pub enum MutationOutcome {
  /// Wrote the updated changelog back to the input file.
  WrittenInPlace,
  /// Caller did not ask for an in-place write; here is the updated
  /// changelog text for the caller to do whatever it likes with.
  Content(String),
  /// A heading along the requested path was not found in the input file.
  HeadingNotFound { heading: String },
}

fn read_changelog(path: &Path) -> Result<String, OperationError> {
  fs::read_to_string(path).map_err(|source| OperationError::ReadChangelog {
    path: path.to_path_buf(),
    source,
  })
}

fn write_changelog(path: &Path, content: &str) -> Result<(), OperationError> {
  fs::write(path, content).map_err(|source| OperationError::WriteChangelog {
    path: path.to_path_buf(),
    source,
  })
}

fn git_show_file(git_ref: &str, path: &Path) -> Result<String, OperationError> {
  let path_str = path.to_string_lossy().into_owned();
  let output = Command::new("git")
    .args(["show", &format!("{}:{}", git_ref, path_str)])
    .output()
    .map_err(OperationError::GitRun)?;

  if !output.status.success() {
    return Err(OperationError::GitShow {
      git_ref: git_ref.to_string(),
      path: path_str,
      stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    });
  }

  Ok(String::from_utf8(output.stdout)?)
}

/// Decides whether to write the mutated changelog back to `input` or hand
/// the text to the caller via [`MutationOutcome::Content`].
fn deliver(
  input: &Path,
  content: String,
  in_place: bool,
) -> Result<MutationOutcome, OperationError> {
  if in_place {
    write_changelog(input, &content)?;
    Ok(MutationOutcome::WrittenInPlace)
  } else {
    Ok(MutationOutcome::Content(content))
  }
}

/// Reads `input` and reports whether the section at `path` is ready to
/// be stamped with a version.  A missing heading is an outcome, not an
/// error — CI front-ends typically want a clean exit-with-message for
/// either "no changes" or "no heading", not a stack trace.
pub fn ready_to_roll(
  input: &Path,
  path: &[String],
) -> Result<ReadyToRollOutcome, OperationError> {
  let content = read_changelog(input)?;
  match is_ready_to_roll(&content, path) {
    Ok(true) => Ok(ReadyToRollOutcome::Ready),
    Ok(false) => Ok(ReadyToRollOutcome::NoChanges),
    Err(RollError::HeadingNotFound { heading }) => {
      Ok(ReadyToRollOutcome::HeadingNotFound { heading })
    }
  }
}

/// Reads `input` at HEAD and at the given git ref, and reports whether
/// HEAD added any visible-content entries relative to the ref under the
/// section at `path`.  Callers choose the path; the conventional value
/// is `[upcoming_heading]` (optionally followed by drill-down segments),
/// but nothing here enforces that.
pub fn check_additions(
  input: &Path,
  base_ref: &str,
  path: &[String],
) -> Result<CheckAdditionsOutcome, OperationError> {
  let head_content = read_changelog(input)?;
  let base_content = git_show_file(base_ref, input)?;
  if has_section_additions(&base_content, &head_content, path) {
    Ok(CheckAdditionsOutcome::HasAdditions)
  } else {
    Ok(CheckAdditionsOutcome::NoAdditions)
  }
}

/// Rolls the changelog at `input` forward by stamping the section at
/// `path` as `version`.  When `in_place` is true, writes the result back
/// to `input`; otherwise returns the text via [`MutationOutcome::Content`].
pub fn roll(
  input: &Path,
  version: &str,
  path: &[String],
  in_place: bool,
) -> Result<MutationOutcome, OperationError> {
  let content = read_changelog(input)?;
  match roller::roll(content, version, path) {
    Ok(rolled) => deliver(input, rolled, in_place),
    Err(RollError::HeadingNotFound { heading }) => {
      Ok(MutationOutcome::HeadingNotFound { heading })
    }
  }
}

/// Inserts an ordered-list item under `item_heading` (find-or-created)
/// inside the section identified by `parent_path` in the changelog at
/// `input`.  Output handling mirrors [`roll`].
pub fn insert_item(
  input: &Path,
  parent_path: &[String],
  item_heading: &str,
  body: &str,
  in_place: bool,
) -> Result<MutationOutcome, OperationError> {
  let content = read_changelog(input)?;
  match roller::insert_item(content, parent_path, item_heading, body) {
    Ok(updated) => deliver(input, updated, in_place),
    Err(RollError::HeadingNotFound { heading }) => {
      Ok(MutationOutcome::HeadingNotFound { heading })
    }
  }
}
