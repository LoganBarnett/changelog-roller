use changelog_roller_lib::{LogFormat, LogLevel};
use clap::Parser;
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
  #[arg(long, env = "LOG_LEVEL")]
  pub log_level: Option<String>,

  /// Log format (text, json)
  #[arg(long, env = "LOG_FORMAT")]
  pub log_format: Option<String>,

  /// Path to configuration file
  #[arg(short, long, env = "CONFIG_FILE")]
  pub config: Option<PathBuf>,

  /// Input CHANGELOG file to process
  #[arg(short, long, env = "INPUT_FILE")]
  pub input_file: Option<PathBuf>,

  /// Modify the input file in place rather than writing to stdout
  #[arg(long)]
  pub in_place: bool,

  /// New version string to stamp on the upcoming section (e.g. v0.2.0)
  #[arg(long, env = "ADD_VERSION")]
  pub add_version: Option<String>,

  /// Name of the heading that accumulates unreleased changes
  #[arg(long, env = "UPCOMING_HEADING")]
  pub upcoming_heading: Option<String>,

  /// Exit non-zero if the upcoming section has no changes; useful as a CI gate
  #[arg(long)]
  pub ready_to_roll: bool,

  /// Git ref to diff against (e.g. origin/main); exit non-zero if no new
  /// entries were added to the upcoming section relative to that ref
  #[arg(long, env = "DIFF_RANGE")]
  pub diff_range: Option<String>,
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
  pub input_file: Option<PathBuf>,
  pub in_place: bool,
  pub add_version: Option<String>,
  pub upcoming_heading: String,
  pub ready_to_roll: bool,
  pub diff_range: Option<String>,
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

    let input_file = cli.input_file.or(config_file.input_file);

    let upcoming_heading = cli
      .upcoming_heading
      .or(config_file.upcoming_heading)
      .unwrap_or_else(|| "Upcoming".to_string());

    Ok(Config {
      log_level,
      log_format,
      input_file,
      in_place: cli.in_place,
      add_version: cli.add_version,
      upcoming_heading,
      ready_to_roll: cli.ready_to_roll,
      diff_range: cli.diff_range,
    })
  }
}
