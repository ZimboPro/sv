use openapiv3::Operation;
use simplelog::{debug, error, warn};

use crate::{
  open_api::{APIType, OpenAPIData},
  terraform::{APIPath, Lambda},
  util::HttpMethod,
};

pub fn cross_validation(
  lambda_data: Vec<Lambda>,
  open_api_data: Vec<OpenAPIData>,
) -> anyhow::Result<()> {
  let mut valid = true;
  for lambda_item in &lambda_data {
    if let Some(arn_key) = &lambda_item.arn_template_key {
      lambda_item.apis.iter().for_each(|api| {
        if !validate_lambda_against_open_api(&open_api_data, arn_key, &lambda_item.key, api) {
          valid = false;
        }
      });
    }
  }
  let lambda_apis: Vec<APIPath> = lambda_data.iter().flat_map(|x| x.apis.clone()).collect();
  open_api_data
    .iter()
    .for_each(|open_api_item| match open_api_item.execution_type {
      APIType::Lambda => {
        let temp = lambda_apis.clone();
        let mut filtered_lambdas = Vec::new();
        for api in temp {
          if api.route == open_api_item.path {
            filtered_lambdas.push(api.method);
          }
        }
        debug!("Filtered lambdas: {:?}", filtered_lambdas);
        if filtered_lambdas.is_empty() {
          valid = false;
          error!(
            "The path {} is not defined in Terraform",
            open_api_item.path
          );
        } else if !filtered_lambdas.contains(&open_api_item.method)
          && !filtered_lambdas.contains(&HttpMethod::Any)
        {
          valid = false;
          error!(
            "The {} method is not defined for the path {} in Terraform",
            open_api_item.method, open_api_item.path
          );
        }
      }
      APIType::SQS => warn!("SQS Functions are currently not handled"), // TODO: Handle SQS
      APIType::StepFunction => warn!("Step Functions are currently not handled"), // TODO: Handle Step Functions
    });
  if !valid {
    return Err(anyhow::anyhow!("Invalid Terraform and OpenAPI documents"));
  }
  Ok(())
}

fn validate_lambda_against_open_api(
  open_api_data: &[OpenAPIData],
  arn_key: &str,
  lambda_key: &str,
  api: &APIPath,
) -> bool {
  debug!("API details: {:?}", api);
  let mut valid = true;
  let filtered = open_api_data.iter().filter(|x| x.path == api.route);
  if filtered.clone().count() == 0 {
    valid = false;
    error!(
      "The path {} is not defined in OpenAPI for the lambda {}",
      api.route, lambda_key
    );
  } else {
    debug!("Routes: {:#?}", filtered.clone().collect::<Vec<_>>());
    let filtered = filtered.filter(|x| api.method == HttpMethod::Any || x.method == api.method);
    debug!(
      "Filtered routes and methods: {:#?}",
      filtered.clone().collect::<Vec<_>>()
    );
    if filtered.clone().count() == 0 {
      valid = false;
      error!(
        "The {} method is not defined for the path {} for the lambda {}",
        api.method, api.route, lambda_key
      );
    } else {
      filtered.for_each(|x| {
        if x.execution_type == APIType::Lambda && !x.uri.contains(arn_key) {
          valid = false;
          error!(
            "The 'uri' doesn't contain the ARN placeholder '{}' in the 'x-amazon-apigateway-integration' extension for {} {} for the lambda {}",
            arn_key, api.method, api.route, lambda_key
          );
        }
      });
    }
  }
  valid
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

// pub fn validate_aws_api_gateway_method(
//   config: &Operation,
//   lambda_key: &str,
//   api: &APIPath,
// ) -> bool {
//   let mut valid = true;
//   match config.request_body {
//     Some(request_body) => {
//       if request_body.required.unwrap_or(false) {
//         valid = false;
//         error!(
//           "The 'requestBody' is required for {} {} for the lambda {}",
//           api.method, api.route, lambda_key
//         );
//       }
//     }
//     None => {
//       valid = false;
//       error!(
//         "The 'requestBody' doesn't exist for {} {} for the lambda {}",
//         api.method, api.route, lambda_key
//       );
//     }
//   }
//   match config.responses.get("200") {
//     Some(response) => match response.content.get("application/json") {
//       Some(content) => {
//         if content.schema.is_none() {
//           valid = false;
//           error!(
//             "The 'schema' doesn't exist for {} {} for the lambda {}",
//             api.method, api.route, lambda_key
//           );
//         }
//       }
//       None => {
//         valid = false;
//         error!(
//           "The 'application/json' content doesn't exist for {} {} for the lambda {}",
//           api.method, api.route, lambda_key
//         );
//       }
//     },
//     None => {
//       valid = false;
//       error!(
//         "The '200' response doesn't exist for {} {} for the lambda {}",
//         api.method, api.route, lambda_key
//       );
//     }
//   }
//   valid
// }

#[cfg(test)]
mod tests {
  use crate::util::HttpMethod;

  use super::*;

  // validate_lambda_against_open_api tests
  #[test]
  fn test_validate_lambda_against_open_api_arn() {
    let open_api_data = vec![OpenAPIData {
      path: "/test".to_string(),
      method: HttpMethod::Get,
      execution_type: APIType::Lambda,
      uri: "arn".to_string(),
    }];
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Get,
      }
    ));
    assert!(!validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Post,
      }
    ));
    assert!(!validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test2".to_string(),
        method: HttpMethod::Get,
      }
    ));
  }

  #[test]
  fn test_validate_lambda_against_open_api_step_function() {
    let open_api_data = vec![OpenAPIData {
      path: "/test".to_string(),
      method: HttpMethod::Get,
      execution_type: APIType::StepFunction,
      uri: "state:action".to_string(),
    }];
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Get,
      }
    ));
    assert!(!validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Post,
      }
    ));
    assert!(!validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test2".to_string(),
        method: HttpMethod::Get,
      }
    ));
  }

  #[test]
  fn test_validate_lambda_against_open_api_multiple_paths() {
    let open_api_data = vec![
      OpenAPIData {
        path: "/test".to_string(),
        method: HttpMethod::Get,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
      OpenAPIData {
        path: "/test2".to_string(),
        method: HttpMethod::Get,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
    ];
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Get,
      }
    ));
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test2".to_string(),
        method: HttpMethod::Get,
      }
    ));
    assert!(!validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test3".to_string(),
        method: HttpMethod::Get,
      }
    ));
  }

  #[test]
  fn test_validate_lambda_against_open_api_multiple_methods() {
    let open_api_data = vec![
      OpenAPIData {
        path: "/test".to_string(),
        method: HttpMethod::Get,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
      OpenAPIData {
        path: "/test".to_string(),
        method: HttpMethod::Post,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
    ];

    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Get,
      }
    ));
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Post,
      }
    ));
    assert!(!validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Put,
      }
    ));
  }

  #[test]
  fn test_validate_lambda_against_open_api_multiple_paths_multiple_methods() {
    let open_api_data = vec![
      OpenAPIData {
        path: "/test".to_string(),
        method: HttpMethod::Get,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
      OpenAPIData {
        path: "/test".to_string(),
        method: HttpMethod::Post,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
      OpenAPIData {
        path: "/test2".to_string(),
        method: HttpMethod::Get,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
      OpenAPIData {
        path: "/test2".to_string(),
        method: HttpMethod::Post,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
      OpenAPIData {
        path: "/test2".to_string(),
        method: HttpMethod::Put,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
      OpenAPIData {
        path: "/test2".to_string(),
        method: HttpMethod::Patch,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
      OpenAPIData {
        path: "/test2".to_string(),
        method: HttpMethod::Delete,
        execution_type: APIType::Lambda,
        uri: "arn".to_string(),
      },
    ];
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Get,
      }
    ));
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test".to_string(),
        method: HttpMethod::Post,
      }
    ));
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test2".to_string(),
        method: HttpMethod::Get,
      }
    ));
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test2".to_string(),
        method: HttpMethod::Post,
      }
    ));
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test2".to_string(),
        method: HttpMethod::Put,
      }
    ));
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test2".to_string(),
        method: HttpMethod::Patch,
      }
    ));
    assert!(validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test2".to_string(),
        method: HttpMethod::Delete,
      }
    ));
    assert!(!validate_lambda_against_open_api(
      &open_api_data,
      "arn",
      "test",
      &APIPath {
        route: "/test3".to_string(),
        method: HttpMethod::Get,
      }
    ));
  }
}
