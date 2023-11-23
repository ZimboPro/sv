use openapiv3::{OpenAPI, Operation, PathItem, ReferenceOr};
use sv::util::HttpMethod;

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
