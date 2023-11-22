mod open_api;
mod terraform;

use anyhow::anyhow;
use clap::Parser;
use merge_yaml_hash::MergeYamlHash;
use oapi::{OApi, OApiDocument};
use open_api::validate_open_api;
use paris::{error, info};
use sppparse::SparseRoot;
use std::io::{self, Write};
use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
};
// extern crate pretty_env_logger;
// #[macro_use]
// extern crate log;
use terraform::validate_terraform;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// The path to the openapi files
    #[arg(short, long)]
    api_path: PathBuf,
    #[arg(short, long)]
    terraform: PathBuf,
    // /// Number of times to greet
    // #[arg(short, long, default_value_t = 1)]
    // count: u8,
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
    let api_path = args.api_path;
    let _ = validating_path(&api_path)?;
    let _ = validating_path(&args.terraform)?;
    let merged_yaml_content = validate_open_api(api_path)?;
    let keys = validate_terraform(args.terraform)?;
    for key in keys {
        let len = merged_yaml_content.matches(&key).count();
        if len > 1 {
            error!(
                "The Lambda ARN placeholder '{}' is used {} times in in the OpenAPI docs",
                key, len
            );
        }
    }
    info!("\nMake sure to check the JSON policy in either api_gateway.tf or the resources for the attached policy.");
    println!("Done");
    Ok(())
}
