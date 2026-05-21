use changelog_roller_lib::{LogFormat, LogLevel};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
  #[error(
    "Failed to read configuration file at {path:?} during startup: {source}"
  )]
  FileRead {
    path: PathBuf,
    #[source]
    source: std::io::Error,
  },

  #[error("Failed to parse configuration file at {path:?}: {source}")]
  Parse {
    path: PathBuf,
    #[source]
    source: toml::de::Error,
  },

  #[error("Configuration validation failed: {0}")]
  Validation(String),
}

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct CliRaw {
  /// Log level (trace, debug, info, warn, error)
  #[arg(long, env = "LOG_LEVEL", global = true)]
  pub log_level: Option<String>,

  /// Log format (text, json)
  #[arg(long, env = "LOG_FORMAT", global = true)]
  pub log_format: Option<String>,

  /// Path to configuration file
  #[arg(short, long, env = "CONFIG_FILE", global = true)]
  pub config: Option<PathBuf>,

  /// Input CHANGELOG file to process (defaults to CHANGELOG.org)
  #[arg(short, long, env = "INPUT_FILE", global = true)]
  pub input_file: Option<PathBuf>,

  /// Modify the input file in place rather than writing to stdout
  #[arg(long, global = true)]
  pub in_place: bool,

  /// Name of the heading that accumulates unreleased changes
  #[arg(long, env = "UPCOMING_HEADING", global = true)]
  pub upcoming_heading: Option<String>,

  #[command(subcommand)]
  pub command: CliCommand,
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
  /// Stamp the upcoming section as a new version and start a fresh upcoming
  Roll {
    /// New version string to stamp on the upcoming section (e.g. v0.2.0)
    #[arg(long)]
    version: String,
  },
  /// Exit non-zero if the upcoming section has no changes; useful as a CI gate
  ReadyToRoll,
  /// Exit non-zero if no new entries were added relative to a git ref
  CheckAdditions {
    /// Git ref to diff against (e.g. origin/main)
    #[arg(long)]
    base: String,
    /// Drill into a subheading under upcoming.  Repeat to walk deeper:
    /// `--under Breaking --under Abi` walks Upcoming → Breaking → Abi.
    #[arg(long)]
    under: Vec<String>,
  },
  /// Insert a new item under a subheading of the upcoming section
  InsertItem {
    /// Subheading under upcoming to insert into (e.g. "Addition", "Fix")
    #[arg(long)]
    heading: String,
    /// Body text of the new list item
    #[arg(long)]
    body: String,
  },
}

#[derive(Debug, Deserialize, Default)]
pub struct ConfigFileRaw {
  pub log_level: Option<String>,
  pub log_format: Option<String>,
  pub input_file: Option<PathBuf>,
  pub upcoming_heading: Option<String>,
}

impl ConfigFileRaw {
  pub fn from_file(path: &PathBuf) -> Result<Self, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|source| {
      ConfigError::FileRead {
        path: path.clone(),
        source,
      }
    })?;

    toml::from_str(&contents).map_err(|source| ConfigError::Parse {
      path: path.clone(),
      source,
    })
  }
}

#[derive(Debug)]
pub struct Config {
  pub log_level: LogLevel,
  pub log_format: LogFormat,
  pub input_file: PathBuf,
  pub in_place: bool,
  pub upcoming_heading: String,
  pub command: CliCommand,
}

impl Config {
  pub fn from_cli_and_file(cli: CliRaw) -> Result<Self, ConfigError> {
    let config_file = if let Some(config_path) = &cli.config {
      ConfigFileRaw::from_file(config_path)?
    } else {
      let default_config_path = PathBuf::from("config.toml");
      if default_config_path.exists() {
        ConfigFileRaw::from_file(&default_config_path)?
      } else {
        ConfigFileRaw::default()
      }
    };

    let log_level = cli
      .log_level
      .or(config_file.log_level)
      .unwrap_or_else(|| "info".to_string())
      .parse::<LogLevel>()
      .map_err(|e| ConfigError::Validation(e.to_string()))?;

    let log_format = cli
      .log_format
      .or(config_file.log_format)
      .unwrap_or_else(|| "text".to_string())
      .parse::<LogFormat>()
      .map_err(|e| ConfigError::Validation(e.to_string()))?;

    let input_file = cli
      .input_file
      .or(config_file.input_file)
      .unwrap_or_else(|| PathBuf::from("CHANGELOG.org"));

    let upcoming_heading = cli
      .upcoming_heading
      .or(config_file.upcoming_heading)
      .unwrap_or_else(|| "Upcoming".to_string());

    Ok(Config {
      log_level,
      log_format,
      input_file,
      in_place: cli.in_place,
      upcoming_heading,
      command: cli.command,
    })
  }
}
