use std::fs::read_to_string;

use openapiv3::Operation;
use openapiv3::{OpenAPI, PathItem, ReferenceOr};
use sv::{
  cross_validation::validate_aws_api_gateway_integration, terraform::APIPath, util::HttpMethod,
};

pub fn read_valid_open_api() -> String {
  std::fs::read_to_string("./test_files/open_api/valid.yaml").unwrap()
}

pub fn deserialize_open_api(contents: &str) -> OpenAPI {
  serde_yaml::from_str(&contents).unwrap()
}

pub fn get_operation(path_data: &ReferenceOr<PathItem>, method: HttpMethod) -> Operation {
  path_data
    .as_item()
    .unwrap()
    .iter()
    .find(|x| x.0 == method.to_string().to_lowercase())
    .unwrap()
    .1
    .to_owned()
}

fn get_operation_data(method: HttpMethod) -> Operation {
  let doc = deserialize_open_api(&read_valid_open_api());
  get_operation(doc.paths.paths.get("/v1/valid/path").unwrap(), method)
}

fn get_operation_data_no_extension(method: HttpMethod) -> Operation {
  let doc =
    deserialize_open_api(&read_to_string("test_files/open_api/invalid_no_extension.yaml").unwrap());
  get_operation(doc.paths.paths.get("/v1/valid/path").unwrap(), method)
}

fn get_operation_data_no_uri(method: HttpMethod) -> Operation {
  let doc =
    deserialize_open_api(&read_to_string("test_files/open_api/invalid_no_uri.yaml").unwrap());
  get_operation(doc.paths.paths.get("/v1/valid/path").unwrap(), method)
}

#[test]
fn test_valid_aws_integration_extension() {
  let d = get_operation_data(HttpMethod::Get);
  let api = APIPath {
    method: HttpMethod::Get,
    route: "/v1/valid/path".to_string(),
  };
  assert!(validate_aws_api_gateway_integration(
    &d,
    "random key",
    "lambda_valid_1_arn",
    &api
  ));
}

#[test]
fn test_invalid_aws_integration_extension_arn() {
  let d = get_operation_data(HttpMethod::Get);
  let api = APIPath {
    method: HttpMethod::Get,
    route: "/v1/valid/path".to_string(),
  };
  assert_eq!(
    validate_aws_api_gateway_integration(&d, "random key", "lambda_invalid_1_arn", &api),
    false
  );
}

#[test]
fn test_invalid_aws_integration_extension_arn_no_uri() {
  let d = get_operation_data_no_uri(HttpMethod::Get);
  let api = APIPath {
    method: HttpMethod::Get,
    route: "/v1/valid/path".to_string(),
  };
  assert_eq!(
    validate_aws_api_gateway_integration(&d, "random key", "lambda_invalid_1_arn", &api),
    false
  );
}

#[test]
fn test_invalid_no_aws_integration_extension() {
  let d = get_operation_data_no_extension(HttpMethod::Get);
  let api = APIPath {
    method: HttpMethod::Get,
    route: "/v1/valid/path".to_string(),
  };
  assert_eq!(
    validate_aws_api_gateway_integration(&d, "random key", "lambda_invalid_1_arn", &api),
    false
  );
}
