use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Ok;

use simplelog::debug;
use simplelog::error;
use simplelog::info;

use crate::util::HttpMethod;

/// The Lambda data that gets extracted
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Lambda {
  /// Terraform lambda key
  pub key: String,
  /// The lambda handler
  pub handler: String,
  /// Is a step function
  pub step_function: bool,
  /// List of APIs and HTTP methods
  pub apis: Vec<APIPath>,
  /// ARN template key
  pub arn_template_key: Option<String>,
  /// Lambda type
  pub lambda_type: LambdaTriggerType,
}

/// The Lambda trigger type
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum LambdaTriggerType {
  /// Step Function
  StepFunction,
  /// API Gateway
  #[default]
  ApiGateway,
  /// Event Bridge
  EventBridge,
  /// Scheduler
  Scheduler,
}

/// API path data
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default, Clone)]
pub struct APIPath {
  /// The HTTP method e.g. GET, POST
  pub method: HttpMethod,
  /// The route path
  pub route: String,
}

/// Validate the Terraform files and extract the data
pub fn validate_terraform(terraform: PathBuf) -> anyhow::Result<Vec<Lambda>> {
  validate_terraform_files(&terraform)?;
  let lambda = terraform.join("lambda.tf");
  let lambda_permissions = terraform.join("lambda_permissions.tf");
  let api_gw = terraform.join("api_gateway.tf");
  let step_fn = terraform.join("step_function.tf");
  let mut lambda_metadata = if lambda.exists() {
    validate_lambda(lambda)?
  } else {
    return Err(anyhow!("File lambda.tf doesn't exist in {:?}", terraform));
  };
  if lambda_permissions.exists() {
    validate_lambda_permissions(lambda_permissions, &mut lambda_metadata)?;
  } else {
    return Err(anyhow!(
      "File lambda_permissions.tf doesn't exist in {:?}",
      terraform
    ));
  }
  let mut lambda_data = if api_gw.exists() {
    extract_api_gw(api_gw, lambda_metadata)?
  } else {
    return Err(anyhow!(
      "File api_gateway.tf doesn't exist in {:?}",
      terraform
    ));
  };
  if step_fn.exists() {
    lambda_data = extract_step_function(lambda_data, step_fn)?;
    let mut valid = true;
    for lambda_item in &lambda_data {
      if lambda_item.arn_template_key.is_none() && !lambda_item.apis.is_empty() {
        valid = false;
        error!(
          "The lambda {} is not used in API gateway but is used in lambda_permissions.tf",
          lambda_item.key
        )
      }
      if lambda_item.arn_template_key.is_some() && lambda_item.apis.is_empty() {
        error!(
          "The lambda arn {} exits in API gateway but not in lambda_permissions.tf",
          lambda_item.key
        )
      }
      if !lambda_item.step_function
        && lambda_item.arn_template_key.is_none()
        && lambda_item.apis.is_empty()
      {
        error!(
          "The lambda arn {} exits in lambda.tf but used anywhere else",
          lambda_item.key
        )
      }
    }
    if !valid {
      return Err(anyhow!("Invalid Terraform configuration"));
    }
    Ok(lambda_data)
  } else {
    let mut valid = true;
    for lambda_item in &lambda_data {
      if lambda_item.arn_template_key.is_none() && !lambda_item.apis.is_empty() {
        valid = false;
        error!("The lambda {} is not use in API gateway", lambda_item.key)
      }
    }
    if !valid {
      return Err(anyhow!("Invalid Terraform configuration"));
    }
    Ok(lambda_data)
  }
}

/// Finds all the files with the extension in the directory recursively for Terraform files
fn find_files(path: &std::path::Path, extension: &OsStr) -> Vec<PathBuf> {
  let mut files = Vec::new();
  for entry in path.read_dir().expect("Failed to read directory").flatten() {
    if entry.path().is_dir() && !entry.path().ends_with(".terraform") {
      files.append(&mut find_files(&entry.path(), extension));
    } else if entry.path().extension() == Some(extension) {
      files.push(entry.path());
    }
  }
  files
}

/// Check if all the Terraform files are valid
fn validate_terraform_files(path: &Path) -> anyhow::Result<()> {
  info!("Validating Terraform files");
  let files = find_files(path, OsStr::new("tf"));
  for file in files {
    let lambda_contents = std::fs::read_to_string(file)?;
    let _ = hcl::parse(&lambda_contents)?;
  }
  Ok(())
}

/// Validate and extract from the lambda.tf file
fn validate_lambda(lambda: PathBuf) -> anyhow::Result<Vec<Lambda>> {
  info!("Validating lambda.tf config");
  let mut lambda_metadata: Vec<Lambda> = Vec::new();
  let mut valid = true;
  debug!("Read Lambda file: {:?}", lambda);
  let lambda_contents = std::fs::read_to_string(lambda)?;
  debug!("Parsing Lambda file");
  let body = hcl::parse(&lambda_contents)?;
  let locals = body
    .blocks()
    .find(|x| x.identifier.to_string() == *"locals")
    .expect("Expected locals to be set");
  let lambdas = locals
    .body
    .attributes()
    .find(|x| x.key.to_string() == *"lambdas")
    .expect("Expected 'lambdas' variable to be set");
  match &lambdas.expr {
    hcl::Expression::Object(s) => {
      for key in s.keys() {
        let l = s.get_key_value(key).expect("Failed to get key");

        let lambda_key = match l.0 {
          hcl::ObjectKey::Identifier(s) => s.to_string(),
          hcl::ObjectKey::Expression(_) => todo!("Unsupported lambda key"),
          _ => todo!("Should not get here"),
        };
        match &l.1 {
          hcl::Expression::Object(data) => {
            let handler = data
              .iter()
              .find_map(|data_item| match data_item.0 {
                hcl::ObjectKey::Identifier(data_key) => {
                  if data_key.to_string().to_lowercase() == "handler".to_lowercase() {
                    return Some(data_item.1.to_string().replace('\"', ""));
                  }
                  None
                }
                hcl::ObjectKey::Expression(_) => None,
                _ => None,
              })
              .expect("Failed to get handler");
            lambda_metadata.push(Lambda {
              key: lambda_key,
              handler,
              ..Default::default()
            })
          }
          _ => todo!("Unsupported lambda expression, expecting object"),
        }
      }
    }
    _ => {
      panic!("Expected Object");
    }
  }
  if !lambda_metadata.is_empty() {
    let mut index = 0;
    let start = lambda_contents
      .find("lambdas")
      .expect("Could not find 'lambdas' in file");
    let (_, end_str) = lambda_contents.split_at(start);
    let end = lambda_contents
      .find("\n}")
      .expect("Could not find closing '}', expecting it to be '\\n}'");
    let (locals, _) = end_str.split_at(end);
    while index < lambda_metadata.len() - 1 {
      let mut j = index + 1;
      let meta = lambda_metadata
        .get(index)
        .expect("Failed to get lambda details");
      if locals.matches(&meta.key).count() > 1 {
        valid = false;
        error!("Key is duplicated: {}", meta.key);
      }
      while j < lambda_metadata.len() {
        let t = lambda_metadata
          .get(j)
          .expect("Failed to get lambda details");
        if meta.handler == t.handler {
          valid = false;
          error!(
            "Both lambda keys '{}' and '{}' are using the same handler {}",
            meta.key, t.key, t.handler
          );
        }
        j += 1;
      }
      index += 1;
    }
  }
  if !valid {
    return Err(anyhow!("Invalid lambda.tf file"));
  }
  Ok(lambda_metadata)
}

/// Validate and extract data from lambda_permissions.tf
fn validate_lambda_permissions(
  lambda_permissions: PathBuf,
  lambda_metadata: &mut [Lambda],
) -> anyhow::Result<()> {
  info!("Validating lambda_permissions.tf config");
  let mut valid = true;
  let lambda_contents = std::fs::read_to_string(lambda_permissions)?;
  let body = hcl::parse(&lambda_contents)?;
  let locals = body
    .blocks()
    .find(|x| x.identifier.to_string() == *"locals")
    .expect("Variable locals not defined in lambda_permissions.tf");
  let lambdas = locals
    .body
    .attributes()
    .find(|x| x.key.to_string() == *"lambdas_permissions")
    .expect("Variable lambdas_permissions doesn't exist in locals");
  match &lambdas.expr {
    hcl::Expression::Object(permissions) => {
      let mut lambda_permission_keys = Vec::new();
      for permission_group in permissions.keys() {
        let lambda_key = match permission_group {
          hcl::ObjectKey::Identifier(s) => s.to_string(),
          hcl::ObjectKey::Expression(_) => todo!("Unsupported lambda key"),
          _ => todo!("Should not get here"),
        };
        lambda_permission_keys.push(lambda_key);
      }
      for permission_group in permissions {
        match permission_group.1 {
          hcl::Expression::Array(arr) => {
            for arr_item in arr {
              match arr_item {
                hcl::Expression::Object(route_obj) => {
                  let s = lambda_metadata
                    .iter_mut()
                    .find(|x| x.key == permission_group.0.to_string())
                    .unwrap_or_else(|| {
                      panic!(
                        "Failed to match permission to key in lambda: {}",
                        permission_group.0
                      )
                    });

                  let principal = route_obj
                    .iter()
                    .find(|r| r.0.to_string() == *"principal")
                    .unwrap();

                  let service = principal.1.to_string().replace('\"', "");
                  match service.as_str() {
                    "apigateway.amazonaws.com" => {
                      s.lambda_type = LambdaTriggerType::ApiGateway;
                    }
                    "events.amazonaws.com" => {
                      s.lambda_type = LambdaTriggerType::EventBridge;
                    }
                    "scheduler.amazonaws.com" => {
                      s.lambda_type = LambdaTriggerType::EventBridge;
                    }
                    _ => todo!("Need to cater for {} service", service),
                  }

                  if s.lambda_type == LambdaTriggerType::ApiGateway {
                    let source_arn = route_obj
                      .iter()
                      .find(|r| r.0.to_string() == *"source_arn")
                      .unwrap();

                    let data = handle_api_gateway_lambda(source_arn.1.to_string())?;
                    debug!("API Gateway Lambda Data: {:?}", data);
                    s.apis.push(APIPath {
                      method: data[0].trim().into(),
                      route: data[1].trim().into(),
                    });
                  }
                }
                _ => todo!("Terraform expression not supported currently, expecting object"),
              }
            }
          }
          _ => todo!("Terraform expression not supported currently, expecting array"),
        }
      }
      for key in lambda_permission_keys {
        if !lambda_metadata.iter().any(|x| x.key == key) {
          valid = false;
          error!("'lambda_permissions' has extra key '{}'", key);
        }
        let len = lambda_contents.matches(&key).count();
        if lambda_contents.matches(&key).count() > 1
          && lambda_metadata
            .iter_mut()
            .find(|x| x.key == key)
            .expect("Failed to match lambda key")
            .apis
            .len()
            != len
        {
          valid = false;
          error!("Key is duplicated: {}", key);
        }
      }
    }
    _ => todo!("Terraform expression not supported currently for lambdas_permissions variable"),
  }
  if !valid {
    return Err(anyhow!("Invalid lambda_permissions.tf file"));
  }
  Ok(())
}

/// Validate and extract data from api_gateway.tf
fn extract_api_gw(api_gw: PathBuf, mut lambda: Vec<Lambda>) -> anyhow::Result<Vec<Lambda>> {
  info!("Validating api_gateway.tf config");
  let contents = std::fs::read_to_string(api_gw)?;
  {
    let _ = hcl::parse(&contents)?;
  }
  let lines = contents.lines();
  let mut valid = true;
  for line in lines {
    for name in &mut lambda {
      if line.contains(&name.key) && !line.trim().starts_with('#') && !line.trim().starts_with("//")
      {
        let parts: Vec<&str> = line.split(':').collect();
        if name.arn_template_key.is_some() {
          valid = false;
          error!("The lambda key '{}' is used more than once", name.key);
        }
        name.arn_template_key = Some(parts[0].trim().to_string());
        break;
      }
    }
  }
  if !valid {
    return Err(anyhow!("Invalid api_gateway.tf"));
  }
  Ok(lambda)
}

/// Validate and extract data from step_function.tf
fn extract_step_function(
  mut lambda_data: Vec<Lambda>,
  step_fn: PathBuf,
) -> anyhow::Result<Vec<Lambda>> {
  info!("Validating step_function.tf config");
  let contents = std::fs::read_to_string(step_fn)?;
  {
    let _ = hcl::parse(&contents)?;
  }
  let lines = contents.lines();
  for line in lines {
    for lambda in &mut lambda_data {
      if line.contains(&format!("module.lambda[\"{}", lambda.key)) {
        lambda.step_function = true;
      }
    }
  }
  Ok(lambda_data)
}

/// Extract the API endpoint and HTTP method
fn extract_api_and_method(line: &str, method: HttpMethod) -> Option<(String, String)> {
  if line.contains(method.to_string().to_uppercase().as_str()) {
    Some((
      method.to_string(),
      line.replace(
        format!("/{}", method.to_string().to_uppercase()).as_str(),
        "",
      ),
    ))
  } else {
    None
  }
}

/// Extract API endpoint and HTTP method from the ARN
fn handle_api_gateway_lambda(source_arn: String) -> anyhow::Result<Vec<String>> {
  let section = source_arn.replace('\"', "");
  debug!("Lambda route: {}", section);
  let parts: Vec<String> = section.split('}').map(|x| x.to_string()).collect();
  if section.contains("/*/*/*") {
    Err(anyhow!(
      "Unsupported route: {}. It should rather be explicit. eg. /*/GET/the/endpoint",
      section
    ))
  } else if section.contains('*') && section.matches('*').count() == 1 {
    let parts: Vec<String> = section.split('*').map(|x| x.to_string()).collect();
    debug!("Parts: {:?}", parts);
    let section = parts[1].replacen('/', " ", 2);
    debug!("Section: {}", section);
    let mut data: Vec<String> = section.trim().split(' ').map(|x| x.to_string()).collect();
    debug!("Data: {:?}", data);
    data[1] = format!("/{}", data[1].trim());
    Ok(data)
  } else if section.contains('*') && section.matches('*').count() == 2 && section.contains("/*/*") {
    let parts: Vec<String> = section.split("/*/*").map(|x| x.to_string()).collect();
    debug!("Parts: {:?}", parts);
    let section = parts[1].replacen('/', "", 1);
    debug!("Section: {}", section);
    let mut data: Vec<String> = section.trim().split(' ').map(|x| x.to_string()).collect();
    data.insert(0, HttpMethod::Any.to_string());
    debug!("Data: {:?}", data);
    data[1] = format!("/{}", data[1].trim());
    Ok(data)
  } else if let Some(data) = extract_api_and_method(parts[1].trim(), HttpMethod::Get) {
    Ok([data.0, data.1].to_vec())
  } else if let Some(data) = extract_api_and_method(parts[1].trim(), HttpMethod::Post) {
    Ok([data.0, data.1].into())
  } else if let Some(data) = extract_api_and_method(parts[1].trim(), HttpMethod::Put) {
    Ok([data.0, data.1].into())
  } else if let Some(data) = extract_api_and_method(parts[1].trim(), HttpMethod::Delete) {
    Ok([data.0, data.1].into())
  } else if let Some(data) = extract_api_and_method(parts[1].trim(), HttpMethod::Patch) {
    Ok([data.0, data.1].into())
  } else {
    todo!("Need to cater for {}", parts[1].trim());
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  // use std::path::PathBuf;

  // #[test]
  // fn test_validate_terraform() {
  //   let terraform = PathBuf::from("tests/terraform");
  //   let lambda_data = validate_terraform(terraform).unwrap();
  //   assert_eq!(lambda_data.len(), 2);
  //   assert_eq!(lambda_data[0].key, "lambda1");
  //   assert_eq!(lambda_data[0].handler, "lambda1.handler");
  //   assert_eq!(lambda_data[0].arn_template_key, Some("lambda1_arn".into()));
  //   assert_eq!(lambda_data[0].apis.len(), 1);
  //   assert_eq!(lambda_data[0].apis[0].method, HttpMethod::Get);
  //   assert_eq!(lambda_data[0].apis[0].route, "/lambda1");
  //   assert_eq!(lambda_data[0].step_function, false);
  //   assert_eq!(lambda_data[0].lambda_type, LambdaType::ApiGateway);
  //   assert_eq!(lambda_data[1].key, "lambda2");
  //   assert_eq!(lambda_data[1].handler, "lambda2.handler");
  //   assert_eq!(lambda_data[1].arn_template_key, Some("lambda2_arn".into()));
  //   assert_eq!(lambda_data[1].apis.len(), 1);
  //   assert_eq!(lambda_data[1].apis[0].method, HttpMethod::Get);
  //   assert_eq!(lambda_data[1].apis[0].route, "/lambda2");
  //   assert_eq!(lambda_data[1].step_function, true);
  //   assert_eq!(lambda_data[1].lambda_type, LambdaType::ApiGateway);
  // }

  // #[test]
  // fn test_validate_terraform_files() {
  //   let terraform = PathBuf::from("tests/terraform");
  //   let result = validate_terraform_files(&terraform);
  //   assert!(result.is_ok());
  // }

  // #[test]
  // fn test_validate_terraform_files_invalid() {
  //   let terraform = PathBuf::from("tests/terraform_invalid");
  //   let result = validate_terraform_files(&terraform);
  //   assert!(result.is_err());
  // }

  // #[test]
  // fn test_validate_lambda() {
  //   let lambda = PathBuf::from("tests/terraform/lambda.tf");
  //   let result = validate_lambda(lambda);
  //   assert!(result.is_ok());
  //   let lambda = result.unwrap();
  //   assert_eq!(lambda.len(), 2);
  //   assert_eq!(lambda[0].key, "lambda1");
  //   assert_eq!(lambda[0].handler, "lambda1.handler");
  //   assert_eq!(lambda[1].key, "lambda2");
  //   assert_eq!(lambda[1].handler, "lambda2.handler");
  // }

  // Tests for handle_api_gateway_lambda
  #[test]
  fn test_handle_api_gateway_lambda() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/api/GET/health\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "GET");
    assert_eq!(data[1], "/api/health");
  }

  #[test]
  fn test_handle_api_gateway_lambda_with_wildcard() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/*/POST/postcode-validation\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "POST");
    assert_eq!(data[1], "/postcode-validation");
  }

  #[test]
  fn test_handle_api_gateway_lambda_with_wildcard_and_path() {
    let source_arn =
      "\"${module.service_api.rest_api_execution_arn}/*/POST/postcode-validation/validate\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "POST");
    assert_eq!(data[1], "/postcode-validation/validate");
  }

  #[test]
  fn test_handle_api_gateway_lambda_with_wildcard_and_path_and_query() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/*/POST/postcode-validation/validate?postcode={postcode}\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "POST");
    assert_eq!(data[1], "/postcode-validation/validate?postcode={postcode}");
  }

  #[test]
  fn test_handle_api_gateway_lambda_with_wildcard_and_path_and_query_and_hash() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/*/POST/postcode-validation/validate?postcode={postcode}#test\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "POST");
    assert_eq!(
      data[1],
      "/postcode-validation/validate?postcode={postcode}#test"
    );
  }

  #[test]
  fn test_handle_api_gateway_lambda_with_wildcard_and_path_and_query_and_hash_and_slash() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/*/POST/postcode-validation/validate?postcode={postcode}#test/\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "POST");
    assert_eq!(
      data[1],
      "/postcode-validation/validate?postcode={postcode}#test/"
    );
  }

  #[test]
  fn test_handle_api_gateway_lambda_post() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/api/POST/health\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "POST");
    assert_eq!(data[1], "/api/health");
  }

  #[test]
  fn test_handle_api_gateway_lambda_put() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/api/PUT/health\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "PUT");
    assert_eq!(data[1], "/api/health");
  }

  #[test]
  fn test_handle_api_gateway_lambda_delete() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/api/DELETE/health\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "DELETE");
    assert_eq!(data[1], "/api/health");
  }

  #[test]
  fn test_handle_api_gateway_lambda_patch() {
    let source_arn = "\"${module.service_api.rest_api_execution_arn}/api/PATCH/health\"";
    let data = handle_api_gateway_lambda(source_arn.to_string()).unwrap();
    assert_eq!(data.len(), 2);
    assert_eq!(data[0], "PATCH");
    assert_eq!(data[1], "/api/health");
  }
}
