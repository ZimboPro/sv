use sv::{self, cross_validation::cross_validation, open_api, terraform};

use clap::Parser;

use open_api::validate_open_api;

use paris::warn;

use std::path::PathBuf;
// extern crate pretty_env_logger;
// #[macro_use]
// extern crate log;
use terraform::validate_terraform;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
  /// The path to the OpenAPI files
  #[arg(short, long)]
  api_path: PathBuf,
  /// The path to the Terraform files
  #[arg(short, long)]
  terraform: PathBuf,
  /// Used to output the arguments to a Markdown file
  #[arg(long, hide = true)]
  markdown: bool,
}

fn validating_path(path: &PathBuf) -> anyhow::Result<()> {
  if !path.exists() {
    return Err(anyhow::anyhow!("Path {:?} does not exist", path));
  } else if !path.is_dir() {
    return Err(anyhow::anyhow!("Path {:?} is not a folder", path));
  }
  Ok(())
}

fn main() -> anyhow::Result<()> {
  // pretty_env_logger::formatted_builder()
  //     .filter_level(log::LevelFilter::Info)
  //     .format_timestamp(None)
  //     .build();
  let args = Args::parse();
  if args.markdown {
    clap_markdown::print_help_markdown::<Args>();
    return Ok(());
  }
  let api_path = args.api_path;
  validating_path(&api_path)?;
  validating_path(&args.terraform)?;
  let merged_yaml_content = validate_open_api(api_path)?;
  let lambda_data = validate_terraform(args.terraform)?;
  cross_validation(lambda_data, merged_yaml_content)?;
  println!();
  warn!("Make sure to check the JSON policy in either api_gateway.tf or the resources for the attached policy.");
  Ok(())
}
