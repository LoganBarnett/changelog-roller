use std::collections::HashSet;

use orgize::{elements::Title, Headline, Org};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RollError {
  #[error("No '{heading}' heading found in changelog")]
  UpcomingNotFound { heading: String },

  #[error("Failed to modify changelog structure: {0}")]
  StructureModification(String),

  #[error("Failed to write rolled changelog output: {0}")]
  OutputWrite(#[from] std::io::Error),

  #[error("Rolled changelog output is not valid UTF-8: {0}")]
  OutputEncoding(#[from] std::string::FromUtf8Error),
}

/// Returns `true` if the heading content (everything after `"** "`) has a
/// `COMMENT` keyword, meaning the entire subtree is commented out in org-mode.
fn is_comment_heading(heading_rest: &str) -> bool {
  let s = heading_rest.trim();
  s == "COMMENT" || s.starts_with("COMMENT ")
}

/// Returns `true` if the heading line carries a `:noexport:` tag, meaning
/// the subtree is excluded from org export.
fn has_noexport_tag(heading_rest: &str) -> bool {
  heading_rest.contains(":noexport:")
}

/// Extracts the visible content lines from the upcoming section.
///
/// "Visible content" means lines that a reader would see as changelog entries:
/// list items, plain text paragraphs.  Skipped are:
/// - headings (used only for structure)
/// - blank lines
/// - property drawers (`:PROPERTIES:` … `:END:`)
/// - org planning lines (`SCHEDULED:`, `DEADLINE:`, `CLOSED:`)
/// - `#+KEYWORD:` metadata and `# comment` lines
/// - entire subtrees under a `COMMENT` heading or a `:noexport:`-tagged heading
///
/// Returns an empty `Vec` when the upcoming heading is absent rather than an
/// error, so callers can still do a diff even if one side of the comparison
/// has no upcoming section at all.
fn upcoming_content_lines(
  org_text: &str,
  upcoming_heading: &str,
) -> Vec<String> {
  let mut in_upcoming = false;
  let mut upcoming_stars: usize = 0;
  let mut in_drawer = false;
  // Star level of the outermost active COMMENT/noexport exclusion, if any.
  let mut excluded_at: Option<usize> = None;
  let mut lines = Vec::new();

  for line in org_text.lines() {
    let trimmed = line.trim();

    let star_count = line.chars().take_while(|&c| c == '*').count();
    let is_heading = star_count > 0 && line[star_count..].starts_with(' ');

    if is_heading {
      // Headings always close any open property drawer.
      in_drawer = false;

      // A heading at the same or higher level exits any active exclusion.
      if let Some(excl) = excluded_at {
        if star_count <= excl {
          excluded_at = None;
        }
      }

      if !in_upcoming {
        let title = line[star_count + 1..].trim();
        if title == upcoming_heading {
          in_upcoming = true;
          upcoming_stars = star_count;
        }
      } else if star_count <= upcoming_stars {
        // Leaving the upcoming section entirely.
        break;
      } else if excluded_at.is_none() {
        // Inside upcoming — check if this sub-heading starts an exclusion.
        let rest = &line[star_count + 1..];
        if is_comment_heading(rest) || has_noexport_tag(rest) {
          excluded_at = Some(star_count);
        }
      }
      continue;
    }

    if !in_upcoming || excluded_at.is_some() {
      continue;
    }

    // Property drawer boundaries.
    if trimmed.eq_ignore_ascii_case(":PROPERTIES:")
      || (trimmed.starts_with(':')
        && trimmed.ends_with(':')
        && trimmed.len() > 1
        && !trimmed[1..trimmed.len() - 1].contains(' '))
    {
      in_drawer = true;
      continue;
    }
    if trimmed.eq_ignore_ascii_case(":END:") {
      in_drawer = false;
      continue;
    }
    if in_drawer {
      continue;
    }

    // Org planning lines, keyword metadata, and comment lines.
    if trimmed.starts_with("SCHEDULED:")
      || trimmed.starts_with("DEADLINE:")
      || trimmed.starts_with("CLOSED:")
      || trimmed.starts_with("#+")
      || trimmed == "#"
      || trimmed.starts_with("# ")
    {
      continue;
    }

    if !trimmed.is_empty() {
      lines.push(line.to_string());
    }
  }

  lines
}

/// Returns `true` if `head_text` contains content lines under the upcoming
/// section that are not present in `base_text`.
///
/// This is a PR-guard: it answers "did this branch add entries to Upcoming?"
/// without caring whether Upcoming already had content in the base.
pub fn has_upcoming_additions(
  base_text: &str,
  head_text: &str,
  upcoming_heading: &str,
) -> bool {
  let base_lines: HashSet<String> =
    upcoming_content_lines(base_text, upcoming_heading)
      .into_iter()
      .collect();
  let head_lines = upcoming_content_lines(head_text, upcoming_heading);
  head_lines.iter().any(|l| !base_lines.contains(l))
}

/// Returns `true` if the upcoming section has at least one subsection with
/// content, indicating the changelog is ready to be stamped with a version.
/// Returns `false` if every subsection is empty.
///
/// Intended for use as a CI gate: a non-ready result should produce a
/// non-zero exit code so pull requests without documented changes are
/// blocked from releasing.
pub fn is_ready_to_roll(
  org_text: &str,
  upcoming_heading: &str,
) -> Result<bool, RollError> {
  let org = Org::parse(org_text);

  let upcoming = org
    .headlines()
    .find(|h| h.title(&org).raw.as_ref() == upcoming_heading)
    .ok_or_else(|| RollError::UpcomingNotFound {
      heading: upcoming_heading.to_string(),
    })?;

  let ready = upcoming.children(&org).any(|c| c.section_node().is_some());
  Ok(ready)
}

/// Rolls a changelog forward by stamping the upcoming section as a new
/// version and inserting a fresh empty upcoming section above it.
///
/// Empty subsections (those with no content) are pruned from the versioned
/// entry so that only changes that actually happened appear under the new
/// version.  The fresh upcoming section always carries the full set of
/// subsection headings, ready to be populated.
pub fn roll(
  org_text: String,
  new_version: &str,
  upcoming_heading: &str,
) -> Result<String, RollError> {
  let mut org = Org::parse_string(org_text);

  let upcoming = org
    .headlines()
    .find(|h| h.title(&org).raw.as_ref() == upcoming_heading)
    .ok_or_else(|| RollError::UpcomingNotFound {
      heading: upcoming_heading.to_string(),
    })?;

  let upcoming_level = upcoming.level();

  // Collect children before any mutation so borrows of `org` don't overlap
  // with the mutable borrows required for tree surgery below.
  let children: Vec<Headline> = upcoming.children(&org).collect();
  let child_titles: Vec<String> = children
    .iter()
    .map(|c| c.title(&org).raw.to_string())
    .collect();
  let child_has_content: Vec<bool> = children
    .iter()
    .map(|c| c.section_node().is_some())
    .collect();

  // Detach every child so they can be rehomed.
  for &child in &children {
    child.detach(&mut org);
  }

  // Build the versioned heading; non-empty children carry their content into
  // it.  Empty children are intentionally discarded — no empty sections in
  // released versions.
  let new_version_hl = Headline::new(
    Title {
      level: upcoming_level,
      raw: String::from(new_version).into(),
      ..Title::default()
    },
    &mut org,
  );
  for (i, &child) in children.iter().enumerate() {
    if child_has_content[i] {
      new_version_hl
        .append(child, &mut org)
        .map_err(|e| RollError::StructureModification(format!("{:?}", e)))?;
    }
  }

  // Build the fresh upcoming heading with all subsections recreated empty.
  let new_upcoming_hl = Headline::new(
    Title {
      level: upcoming_level,
      raw: String::from(upcoming_heading).into(),
      ..Title::default()
    },
    &mut org,
  );
  for title in &child_titles {
    let empty_child = Headline::new(
      Title {
        level: upcoming_level + 1,
        raw: title.clone().into(),
        ..Title::default()
      },
      &mut org,
    );
    new_upcoming_hl
      .append(empty_child, &mut org)
      .map_err(|e| RollError::StructureModification(format!("{:?}", e)))?;
  }

  // Splice both new headings into the document at the old upcoming's position,
  // then remove the now-empty old upcoming.
  upcoming
    .insert_before(new_upcoming_hl, &mut org)
    .map_err(|e| RollError::StructureModification(format!("{:?}", e)))?;
  new_upcoming_hl
    .insert_after(new_version_hl, &mut org)
    .map_err(|e| RollError::StructureModification(format!("{:?}", e)))?;
  upcoming.detach(&mut org);

  let mut output: Vec<u8> = Vec::new();
  org.write_org(&mut output)?;
  Ok(String::from_utf8(output)?)
}
