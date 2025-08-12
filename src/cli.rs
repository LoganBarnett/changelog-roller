use clap::{Subcommand, Parser};

#[Subcommand]
pub enum Subcommand {
  AddVersion(String),
}

#[derive(Debug, Parser)]
#[command(
  name = "changelog-roller",
  about = "Roll CHANGELOG files automatically.",
)]
pub struct Cli {
  #[arg(
    env,
    short,
    long,
    default_value = "",
    help = "",
  )]
  pub input_file: String,
  #[arg(
    env,
    short,
    long,
    default_value = false,
    help = "",
  )]
  pub in_place: Boolean,
  #[command(flatten)]
  pub verbosity: clap_verbosity_flag::Verbosity,
}
