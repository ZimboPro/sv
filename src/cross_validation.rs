use openapiv3::Operation;
use paris::error;

use crate::{
  terraform::{APIPath, Lambda},
  util::HttpMethod,
};

pub fn cross_validation(
  lambda_data: Vec<Lambda>,
  merged_yaml_content: String,
) -> anyhow::Result<()> {
  let mut valid = true;
  let doc: openapiv3::OpenAPI = serde_yaml::from_str(&merged_yaml_content)?;
  for lambda_item in &lambda_data {
    if let Some(arn_key) = &lambda_item.arn_template_key {
      lambda_item
        .apis
        .iter()
        .for_each(|api| match doc.paths.paths.get(&api.route) {
          Some(path_item) => match api.method {
            HttpMethod::Get => {
              if let Some(config) = &path_item.as_item().expect("failed to get path data").get {
                if !validate_aws_api_gateway_integration(config, &lambda_item.key, arn_key, api) {
                  valid = false;
                }
              } else {
                valid = false;
                error!(
                  "The GET method is not defined for the path {} for the lambda {}",
                  api.route, lambda_item.key
                );
              }
            }
            HttpMethod::Post => {
              if let Some(config) = &path_item.as_item().expect("failed to get path data").post {
                if !validate_aws_api_gateway_integration(config, &lambda_item.key, arn_key, api) {
                  valid = false;
                }
              } else {
                valid = false;
                error!(
                  "The POST method is not defined for the path {} for the lambda {}",
                  api.route, lambda_item.key
                );
              }
            }
            HttpMethod::Put => {
              if let Some(config) = &path_item.as_item().expect("failed to get path data").put {
                if !validate_aws_api_gateway_integration(config, &lambda_item.key, arn_key, api) {
                  valid = false;
                }
              } else {
                valid = false;
                error!(
                  "The PUT method is not defined for the path {} for the lambda {}",
                  api.route, lambda_item.key
                );
              }
            }
            HttpMethod::Delete => {
              if let Some(config) = &path_item.as_item().expect("failed to get path data").delete {
                if !validate_aws_api_gateway_integration(config, &lambda_item.key, arn_key, api) {
                  valid = false;
                }
              } else {
                valid = false;
                error!(
                  "The DELETE method is not defined for the path {} for the lambda {}",
                  api.route, lambda_item.key
                );
              }
            }
            HttpMethod::Patch => {
              if let Some(config) = &path_item.as_item().expect("failed to get path data").patch {
                if !validate_aws_api_gateway_integration(config, &lambda_item.key, arn_key, api) {
                  valid = false;
                }
              } else {
                valid = false;
                error!(
                  "The PATCH method is not defined for the path {} for the lambda {}",
                  api.route, lambda_item.key
                );
              }
            }
            HttpMethod::Options => {
              if let Some(config) = &path_item
                .as_item()
                .expect("failed to get path data")
                .options
              {
                if !validate_aws_api_gateway_integration(config, &lambda_item.key, arn_key, api) {
                  valid = false;
                }
              } else {
                valid = false;
                error!(
                  "The OPTIONS method is not defined for the path {} for the lambda {}",
                  api.route, lambda_item.key
                );
              }
            }
            HttpMethod::Head => {
              if let Some(config) = &path_item.as_item().expect("failed to get path data").head {
                if !validate_aws_api_gateway_integration(config, &lambda_item.key, arn_key, api) {
                  valid = false;
                }
              } else {
                valid = false;
                error!(
                  "The HEAD method is not defined for the path {} for the lambda {}",
                  api.route, lambda_item.key
                );
              }
            }
            _ => todo!(),
          },
          None => {
            error!(
              "The path {} is not defined in OpenAPi for the lambda {}",
              api.route, lambda_item.key
            );
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
  let lambda_apis: Vec<APIPath> = lambda_data.iter().flat_map(|x| x.apis.clone()).collect();
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
      for method in path.1.as_item().expect("Failed to get method data").iter() {
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
  Ok(())
}

pub fn validate_aws_api_gateway_integration(
  config: &Operation,
  lambda_key: &str,
  arn_key: &str,
  api: &APIPath,
) -> bool {
  let mut valid = true;
  match config.extensions.get("x-amazon-apigateway-integration") {
    Some(aws) => match aws.get("uri") {
      Some(uri) => {
        let uri_path = uri.as_str().expect("Failed to convert URI to string");
        if !uri_path.contains(arn_key) {
          valid = false;
          error!("The 'uri' doesn't contain the ARN placeholder '{}' in the 'x-amazon-apigateway-integration' extension for {} {} for the lambda {}", arn_key, api.method, api.route, lambda_key);
        }
        if uri_path.contains("state:action") {
          valid = false;
          error!(
            "The 'uri' for {} {} is set up for step functions instead of the lambda {}",
            api.method, api.route, lambda_key
          );
        }
      }
      None => {
        valid = false;
        error!("The 'uri' doesn't exist in the 'x-amazon-apigateway-integration' extension for {} {} for the lambda {}", api.method, api.route, lambda_key);
      }
    },
    None => {
      valid = false;
      error!("The 'x-amazon-apigateway-integration' extension doesn't exist for the {} {} for the lambda {}", api.method, api.route, lambda_key);
    }
  }
  valid
}
