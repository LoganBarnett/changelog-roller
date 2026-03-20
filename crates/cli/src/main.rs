mod config;
mod logging;

use changelog_roller_lib::{
  has_upcoming_additions, is_ready_to_roll, roll, RollError,
};
use clap::Parser;
use config::{CliRaw, Config, ConfigError};
use logging::init_logging;
use std::path::PathBuf;
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
enum ApplicationError {
  #[error("Failed to load configuration during startup: {0}")]
  ConfigurationLoad(#[from] ConfigError),

  #[error("--input-file is required when using --add-version or --diff-range")]
  MissingInputFile,

  #[error("Failed to read changelog at {path:?}: {source}")]
  ChangelogRead {
    path: PathBuf,
    source: std::io::Error,
  },

  #[error("Failed to write changelog at {path:?}: {source}")]
  ChangelogWrite {
    path: PathBuf,
    source: std::io::Error,
  },

  #[error("Failed to roll changelog: {0}")]
  RollFailed(#[from] RollError),

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

fn main() -> Result<(), ApplicationError> {
  let cli = CliRaw::parse();

  let config = Config::from_cli_and_file(cli).map_err(|e| {
    eprintln!("Configuration error: {}", e);
    ApplicationError::ConfigurationLoad(e)
  })?;

  init_logging(config.log_level, config.log_format);

  info!("Starting changelog-roller");

  run(config)?;

  info!("Shutting down changelog-roller");
  Ok(())
}

fn git_show_file(
  git_ref: &str,
  path: &PathBuf,
) -> Result<String, ApplicationError> {
  let path_str = path.to_string_lossy().into_owned();
  let output = std::process::Command::new("git")
    .args(["show", &format!("{}:{}", git_ref, path_str)])
    .output()
    .map_err(ApplicationError::GitRun)?;

  if !output.status.success() {
    return Err(ApplicationError::GitShow {
      git_ref: git_ref.to_string(),
      path: path_str,
      stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    });
  }

  Ok(String::from_utf8(output.stdout)?)
}

fn run(config: Config) -> Result<(), ApplicationError> {
  if config.ready_to_roll {
    let input_path = config
      .input_file
      .ok_or(ApplicationError::MissingInputFile)?;
    let content = std::fs::read_to_string(&input_path).map_err(|source| {
      ApplicationError::ChangelogRead {
        path: input_path.clone(),
        source,
      }
    })?;

    match is_ready_to_roll(&content, &config.upcoming_heading) {
      Ok(true) => {
        info!(
          "Upcoming heading '{}' has changes — ready to roll",
          config.upcoming_heading
        );
      }
      Ok(false) => {
        eprintln!(
          "Not ready to roll: no changes in '{}'",
          config.upcoming_heading
        );
        std::process::exit(1);
      }
      Err(RollError::UpcomingNotFound { ref heading }) => {
        eprintln!("Not ready to roll: no '{}' heading found", heading);
        std::process::exit(1);
      }
      Err(e) => return Err(ApplicationError::RollFailed(e)),
    }

    return Ok(());
  }

  if let Some(ref base_ref) = config.diff_range {
    let input_path = config
      .input_file
      .as_ref()
      .ok_or(ApplicationError::MissingInputFile)?;

    let head_content =
      std::fs::read_to_string(input_path).map_err(|source| {
        ApplicationError::ChangelogRead {
          path: input_path.clone(),
          source,
        }
      })?;

    let base_content = git_show_file(base_ref, input_path)?;

    if has_upcoming_additions(
      &base_content,
      &head_content,
      &config.upcoming_heading,
    ) {
      info!("Upcoming section has new entries relative to '{}'", base_ref);
    } else {
      eprintln!(
        "No new entries added to '{}' relative to '{}'",
        config.upcoming_heading, base_ref
      );
      std::process::exit(1);
    }

    return Ok(());
  }

  let version = match config.add_version {
    Some(v) => v,
    None => {
      info!("No --add-version specified; nothing to do");
      return Ok(());
    }
  };

  let input_path = config
    .input_file
    .ok_or(ApplicationError::MissingInputFile)?;

  let content = std::fs::read_to_string(&input_path).map_err(|source| {
    ApplicationError::ChangelogRead {
      path: input_path.clone(),
      source,
    }
  })?;

  let rolled = roll(content, &version, &config.upcoming_heading)?;

  if config.in_place {
    std::fs::write(&input_path, rolled.as_bytes()).map_err(|source| {
      ApplicationError::ChangelogWrite {
        path: input_path,
        source,
      }
    })?;
  } else {
    print!("{}", rolled);
  }

  Ok(())
}
