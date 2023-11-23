mod open_api;
mod terraform;
mod util;

use clap::Parser;

use open_api::validate_open_api;
use openapiv3::{self, Operation};
use paris::{error, warn};

use std::path::PathBuf;
// extern crate pretty_env_logger;
// #[macro_use]
// extern crate log;
use terraform::{validate_terraform, APIPath};

use crate::util::HttpMethod;

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
    let keys = validate_terraform(args.terraform)?;
    // TODO rather check for path and attached arn key
    let mut valid = true;
    let doc: openapiv3::OpenAPI = serde_yaml::from_str(&merged_yaml_content)?;
    for key in &keys {
        if let Some(arn_key) = &key.arn_template_key {
            key.apis
                .iter()
                .for_each(|api| {
                    match doc.paths.paths.get(&api.route) {
                    Some(path_item) => match api.method {
                        HttpMethod::Get => {
                            if let Some(config) = &path_item.as_item().unwrap().get {
                                if !validate_aws_api_gateway_integration(config, &key.key, arn_key, api) {
                                    valid = false;
                                }
                            }
                            else {
                                valid = false;
                                error!("The GET method is not defined for the path {} for the lambda {}", api.route, key.key);
                            }
                        }
                        HttpMethod::Post => {
                            if let Some(config) = &path_item.as_item().unwrap().post {
                                if !validate_aws_api_gateway_integration(config, &key.key, arn_key, api) {
                                    valid = false;
                                }
                            } else {
                                valid = false;
                                error!("The POST method is not defined for the path {} for the lambda {}", api.route, key.key);
                            }
                        }
                        HttpMethod::Put => {
                            if let Some(config) = &path_item.as_item().unwrap().put {
                                if !validate_aws_api_gateway_integration(config, &key.key, arn_key, api) {
                                    valid = false;
                                }
                            } else {
                                valid = false;
                                error!("The PUT method is not defined for the path {} for the lambda {}", api.route, key.key);
                            }
                        }
                        HttpMethod::Delete => {
                            if let Some(config) = &path_item.as_item().unwrap().delete {
                                if !validate_aws_api_gateway_integration(config, &key.key, arn_key, api) {
                                    valid = false;
                                }
                            } else {
                                valid = false;
                                error!(
                                    "The DELETE method is not defined for the path {} for the lambda {}",
                                    api.route, key.key
                                );
                            }
                        }
                        HttpMethod::Patch => {
                            if let Some(config) = &path_item.as_item().unwrap().patch {
                                if !validate_aws_api_gateway_integration(config, &key.key, arn_key, api) {
                                    valid = false;
                                }
                            } else {
                                valid = false;
                                error!(
                                    "The PATCH method is not defined for the path {} for the lambda {}",
                                    api.route, key.key
                                );
                            }
                        }
                        HttpMethod::Options => {
                            if let Some(config) = &path_item.as_item().unwrap().options {
                                if !validate_aws_api_gateway_integration(config, &key.key, arn_key, api) {
                                    valid = false;
                                }
                            } else {
                                valid = false;
                                error!(
                                    "The OPTIONS method is not defined for the path {} for the lambda {}",
                                    api.route, key.key
                                );
                            }
                        }
                        HttpMethod::Head => {
                            if let Some(config) = &path_item.as_item().unwrap().head {
                                if !validate_aws_api_gateway_integration(config, &key.key, arn_key, api) {
                                    valid = false;
                                }
                            } else {
                                valid = false;
                                error!("The HEAD method is not defined for the path {} for the lambda {}", api.route, key.key);
                            }
                        }
                        _ => todo!(),
                    },
                    None => {
                        error!("The path {} is not defined in OpenAPi for the lambda {}", api.route, key.key);
                    }
                }
        });
            let len = merged_yaml_content.matches(arn_key).count();
            if len == 0 {
                valid = false;
                error!(
                    "The Lambda ARN placeholder '{}' is not used in the OpenAPI docs",
                    arn_key
                );
            }
        }
    }
    let lambda_apis: Vec<APIPath> = keys.iter().flat_map(|x| x.apis.clone()).collect();
    doc.paths.iter().for_each(|path| {
        let temp = lambda_apis.clone();
        let mut filtered_lambdas = Vec::new();
        for api in temp {
            if &api.route == path.0 {
                filtered_lambdas.push(api.method.to_string().to_lowercase());
            }
        }
        if filtered_lambdas.is_empty() {
            valid = false;
            error!("The path {} is not defined in Terraform", path.0);
        } else {
            let mut methods = Vec::new();
            for method in path.1.as_item().unwrap().iter() {
                methods.push(method.0);
            }
            let mut filtered = Vec::new();
            for method in &methods {
                if !filtered_lambdas.contains(&method.to_string()) {
                    filtered.push(method);
                }
            }
            if !filtered.is_empty() {
                valid = false;
                error!(
                    "The path {} is not defined in Terraform for the following methods: {:?}",
                    path.0, filtered
                );
            }
        }
    });
    if !valid {
        return Err(anyhow::anyhow!("Invalid Terraform and OpenAPI documents"));
    }
    println!();
    warn!("Make sure to check the JSON policy in either api_gateway.tf or the resources for the attached policy.");
    Ok(())
}

fn validate_aws_api_gateway_integration(
    config: &Operation,
    lambda_key: &str,
    arn_key: &str,
    api: &APIPath,
) -> bool {
    let mut valid = true;
    match config.extensions.get("x-amazon-apigateway-integration") {
        Some(aws) => {
            match aws.get("uri") {
                Some(uri) => {
                    let uri_path = uri.as_str().unwrap();
                    if !uri_path.contains(arn_key) {
                        valid = false;
                        error!("The 'uri' doesn't contain the ARN placeholder '{}' in the 'x-amazon-apigateway-integration' extension for {} {} for the lambda {}", arn_key, api.method, api.route, lambda_key);
                    }
                    if uri_path.contains("state:action") {
                        valid = false;
                        error!("The 'uri' for {} {} is set up for step functions instead of the lambda {}", api.method, api.route, lambda_key);
                    }
                }
                None => {
                    valid = false;
                    error!("The 'uri' doesn't exist in the 'x-amazon-apigateway-integration' extension for {} {} for the lambda {}", api.method, api.route, lambda_key);
                }
            }
        }
        None => {
            valid = false;
            error!("The 'x-amazon-apigateway-integration' extension doesn't exist for the {} {} for the lambda {}", api.method, api.route, lambda_key);
        }
    }
    valid
}
