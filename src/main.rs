use self_update::cargo_crate_version;
use simplelog::{
  info, warn, Color, ColorChoice, Config, ConfigBuilder, Level, LevelFilter, TermLogger,
  TerminalMode,
};
use sv::{self, cross_validation::cross_validation, open_api, terraform};

use clap::{Args, Parser};

use open_api::validate_open_api;

use std::path::PathBuf;
// extern crate pretty_env_logger;
// #[macro_use]
// extern crate log;
use terraform::validate_terraform;

#[derive(Debug, Parser, PartialEq, Eq)]
enum Commands {
  /// Update the binary to the latest version
  Update,
  /// Verify the OpenAPI and Terraform files
  Verify(Arguments),
  /// Output the markdown help page
  #[command(hide = true)]
  Markdown,
}

#[derive(Args, Debug, PartialEq, Eq)]
#[command(author, version, about, long_about = None)]
struct Arguments {
  /// The path to the OpenAPI files
  #[arg(short, long)]
  api_path: PathBuf,
  /// The path to the Terraform files
  #[arg(short, long)]
  terraform: PathBuf,
  /// Verbose mode
  #[arg(short, long)]
  verbose: bool,
  /// Used to continue even if the CyclicRef error occurs
  #[arg(long)]
  skip_cyclic: bool,
}

fn validating_path(path: &PathBuf) -> anyhow::Result<()> {
  if !path.exists() {
    return Err(anyhow::anyhow!("Path {:?} does not exist", path));
  } else if !path.is_dir() {
    return Err(anyhow::anyhow!("Path {:?} is not a folder", path));
  }
  Ok(())
}

fn check_if_update_is_available() -> anyhow::Result<()> {
  let mut rel_builder = self_update::backends::github::ReleaseList::configure();
  rel_builder.repo_owner("ZimboPro");
  let releases = rel_builder.repo_name("sv").build()?.fetch()?;

  let current = cargo_crate_version!();
  let greater_releases = releases
    .iter()
    .filter(|release| self_update::version::bump_is_greater(current, &release.version).unwrap())
    .collect::<Vec<_>>();
  if !greater_releases.is_empty() {
    let mut latest = greater_releases.first().unwrap().to_owned().clone();
    for release in greater_releases {
      latest = if self_update::version::bump_is_greater(&latest.version, &release.version).unwrap()
      {
        latest
      } else {
        release.clone()
      };
    }

    info!(
      "There is a new version available: {}",
      latest.version.to_string()
    );
    info!("Run `sv update` to update to the latest version.");
  }
  Ok(())
}

fn update_binary(config: Config) -> anyhow::Result<()> {
  TermLogger::init(
    LevelFilter::Info,
    config,
    TerminalMode::Stdout,
    ColorChoice::Auto,
  )
  .unwrap();
  info!("Updating binary to the latest version");
  let mut status_builder = self_update::backends::github::Update::configure();
  let status = status_builder
    .repo_owner("ZimboPro")
    .repo_name("sv")
    .bin_name("sv")
    .show_download_progress(true)
    .current_version(cargo_crate_version!())
    .build()?
    .update()?;

  info!("Updated successfully to {}", status.version());
  Ok(())
}

fn main() -> anyhow::Result<()> {
  // pretty_env_logger::formatted_builder()
  //     .filter_level(log::LevelFilter::Info)
  //     .format_timestamp(None)
  //     .build();
  let config = ConfigBuilder::new()
    .set_level_color(Level::Debug, Some(Color::Cyan))
    .set_level_color(Level::Info, Some(Color::Blue))
    .set_level_color(Level::Warn, Some(Color::Yellow))
    .set_level_color(Level::Error, Some(Color::Magenta))
    .set_level_color(Level::Trace, Some(Color::Green))
    .set_time_level(LevelFilter::Off)
    .build();
  let args = Commands::parse();
  match args {
    Commands::Update => update_binary(config),
    Commands::Verify(args) => {
      if let Err(_) = check_if_update_is_available() {
        warn!("Failed to check for updates");
      }

      let level = if args.verbose {
        LevelFilter::Debug
      } else {
        LevelFilter::Info
      };

      TermLogger::init(level, config, TerminalMode::Stdout, ColorChoice::Auto).unwrap();
      let api_path = args.api_path;
      validating_path(&api_path)?;
      validating_path(&args.terraform)?;
      let open_api_config = validate_open_api(api_path, args.skip_cyclic)?;
      let lambda_data = validate_terraform(args.terraform)?;
      cross_validation(lambda_data, open_api_config)?;
      println!();
      warn!("Make sure to check the JSON policy in either api_gateway.tf or the resources for the attached policy.");
      warn!("NOTE: This tool only checks for common errors. It does not check for all errors.");
      Ok(())
    }
    Commands::Markdown => {
      clap_markdown::print_help_markdown::<Commands>();
      Ok(())
    }
  }
}
