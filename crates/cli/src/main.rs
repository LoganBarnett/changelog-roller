mod config;
mod logging;

use changelog_roller_lib::operation::{
  self, CheckAdditionsOutcome, MutationOutcome, OperationError,
  ReadyToRollOutcome,
};
use clap::Parser;
use config::{CliCommand, CliRaw, Config, ConfigError};
use logging::init_logging;
use thiserror::Error;
use tracing::info;

#[derive(Debug, Error)]
enum ApplicationError {
  #[error("Failed to load configuration during startup: {0}")]
  ConfigurationLoad(#[from] ConfigError),

  #[error("{0}")]
  OperationFailed(#[from] OperationError),
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

/// Renders a mutation outcome to stdout / stderr / exit code.  Shared by
/// `roll` and `insert-item` since they have identical output semantics.
fn present_mutation(outcome: MutationOutcome) -> Result<(), ApplicationError> {
  match outcome {
    MutationOutcome::WrittenInPlace => Ok(()),
    MutationOutcome::Content(content) => {
      print!("{}", content);
      Ok(())
    }
    MutationOutcome::HeadingNotFound { heading } => {
      eprintln!("No '{}' heading found", heading);
      std::process::exit(1);
    }
  }
}

fn run(config: Config) -> Result<(), ApplicationError> {
  let upcoming_path = vec![config.upcoming_heading.clone()];
  match config.command {
    CliCommand::ReadyToRoll => {
      match operation::ready_to_roll(&config.input_file, &upcoming_path)? {
        ReadyToRollOutcome::Ready => {
          info!(
            "Upcoming heading '{}' has changes — ready to roll",
            config.upcoming_heading
          );
          Ok(())
        }
        ReadyToRollOutcome::NoChanges => {
          eprintln!(
            "Not ready to roll: no changes in '{}'",
            config.upcoming_heading
          );
          std::process::exit(1);
        }
        ReadyToRollOutcome::HeadingNotFound { heading } => {
          eprintln!("Not ready to roll: no '{}' heading found", heading);
          std::process::exit(1);
        }
      }
    }

    CliCommand::CheckAdditions { base, under } => {
      let mut path = upcoming_path;
      path.extend(under);
      let target_label = path.join(" → ");
      match operation::check_additions(&config.input_file, &base, &path)? {
        CheckAdditionsOutcome::HasAdditions => {
          info!("'{}' has new entries relative to '{}'", target_label, base);
          Ok(())
        }
        CheckAdditionsOutcome::NoAdditions => {
          eprintln!(
            "No new entries added to '{}' relative to '{}'",
            target_label, base
          );
          std::process::exit(1);
        }
      }
    }

    CliCommand::Roll { version } => present_mutation(operation::roll(
      &config.input_file,
      &version,
      &upcoming_path,
      config.in_place,
    )?),

    CliCommand::InsertItem { heading, body } => {
      present_mutation(operation::insert_item(
        &config.input_file,
        &upcoming_path,
        &heading,
        &body,
        config.in_place,
      )?)
    }
  }
}
