use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;

use anyhow::anyhow;
use anyhow::Ok;

use paris::error;
use paris::info;

use crate::util::HttpMethod;

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
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default, Clone)]
pub struct APIPath {
  pub method: HttpMethod,
  pub route: String,
}

// fn extract_value(bytes: &str) -> IResult<&str, String> {
//     let (remaining, (_, val)) = tuple((tag("\""), take_till(|c| c == '"')))(bytes)?;
//     Ok((remaining, val.to_string()))
// }

// fn till_lambdas(bytes: &str) -> IResult<&str, ()> {
//     let (remaining, _) = take_until("lambdas = {")(bytes)?;
//     Ok((remaining, ()))
// }

// fn extract_key(bytes: &str) -> IResult<&str, String> {
//     let (r, val) = take_till(|c| is_space(c) || c == '=')(bytes)?;
//     Ok((r, val.to_string()))
// }

// fn parse_lambda_content(bytes: &str) -> IResult<&str, Vec<Lambda>> {
//     let (remaining, (_, _, _, key, _, _, _, val)) = tuple((
//         till_lambdas,
//         tag("lambdas = {"),
//         multispace0,
//         extract_key,
//         take_until("handler ="),
//         tag("handler ="),
//         multispace0,
//         extract_value,
//     ));
//     Ok(())
// }

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

fn validate_terraform_files(path: &Path) -> anyhow::Result<()> {
  info!("Validating Terraform files");
  let files = find_files(path, OsStr::new("tf"));
  for file in files {
    let lambda_contents = std::fs::read_to_string(file)?;
    let _ = hcl::parse(&lambda_contents)?;
  }
  Ok(())
}

fn validate_lambda(lambda: PathBuf) -> anyhow::Result<Vec<Lambda>> {
  info!("Validating lambda.tf config");
  let mut lambda_metadata: Vec<Lambda> = Vec::new();
  let mut valid = true;
  let lambda_contents = std::fs::read_to_string(lambda)?;
  let body = hcl::parse(&lambda_contents)?;
  let locals = body
    .blocks()
    .find(|x| x.identifier.to_string() == *"locals")
    .unwrap();
  let lambdas = locals
    .body
    .attributes()
    .find(|x| x.key.to_string() == *"lambdas")
    .unwrap();
  match &lambdas.expr {
    hcl::Expression::Object(s) => {
      for key in s.keys() {
        let l = s.get_key_value(key).unwrap();

        let lambda_key = match l.0 {
          hcl::ObjectKey::Identifier(s) => s.to_string(),
          hcl::ObjectKey::Expression(_) => todo!(),
          _ => todo!(),
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
              .unwrap();
            lambda_metadata.push(Lambda {
              key: lambda_key,
              handler,
              ..Default::default()
            })
          }
          _ => todo!(),
        }
      }
    }
    _ => {
      panic!("Expected Object");
    }
  }
  if !lambda_metadata.is_empty() {
    let mut index = 0;
    let start = lambda_contents.find("lambdas").unwrap();
    let (_, end_str) = lambda_contents.split_at(start);
    let end = lambda_contents.find("\n}").unwrap();
    let (locals, _) = end_str.split_at(end);
    while index < lambda_metadata.len() - 1 {
      let mut j = index + 1;
      let meta = lambda_metadata.get(index).unwrap();
      if locals.matches(&meta.key).count() > 1 {
        valid = false;
        error!("Key is duplicated: {}", meta.key);
      }
      while j < lambda_metadata.len() {
        let t = lambda_metadata.get(j).unwrap();
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

fn validate_lambda_permissions(
  lambda_permissions: PathBuf,
  keys: &mut [Lambda],
) -> anyhow::Result<()> {
  info!("Validating lambda_permissions.tf config");
  let mut valid = true;
  let lambda_contents = std::fs::read_to_string(lambda_permissions)?;
  let body = hcl::parse(&lambda_contents)?;
  let locals = body
    .blocks()
    .find(|x| x.identifier.to_string() == *"locals")
    .unwrap();
  let lambdas = locals
    .body
    .attributes()
    .find(|x| x.key.to_string() == *"lambdas_permissions")
    .unwrap();
  match &lambdas.expr {
    hcl::Expression::Object(s) => {
      let mut p_keys = Vec::new();
      for key in s.keys() {
        let lambda_key = match key {
          hcl::ObjectKey::Identifier(s) => s.to_string(),
          hcl::ObjectKey::Expression(_) => todo!(),
          _ => todo!(),
        };
        p_keys.push(lambda_key);
      }
      for item in s {
        match item.1 {
          hcl::Expression::Array(arr) => {
            for arr_item in arr {
              match arr_item {
                hcl::Expression::Object(route_obj) => {
                  for route in route_obj {
                    if route.0.to_string() == *"source_arn" {
                      let s = keys
                        .iter_mut()
                        .find(|x| x.key == item.0.to_string())
                        .unwrap();
                      let section = route.1.to_string().replace('\"', "");
                      let parts: Vec<&str> = section.split('*').collect();
                      let section = parts[1].replacen('/', " ", 2);
                      let data: Vec<&str> = section.trim().split(' ').collect();

                      s.apis.push(APIPath {
                        method: data[0].trim().into(),
                        route: format!("/{}", data[1].trim()),
                      });
                    }
                  }
                }
                _ => todo!(),
              }
            }
          }
          _ => todo!(),
        }
      }
      for key in p_keys {
        if !keys.iter().any(|x| x.key == key) {
          valid = false;
          error!("'lambda_permissions' has extra key '{}'", key);
        }
        let len = lambda_contents.matches(&key).count();
        if lambda_contents.matches(&key).count() > 1
          && keys.iter_mut().find(|x| x.key == key).unwrap().apis.len() != len
        {
          valid = false;
          error!("Key is duplicated: {}", key);
        }
      }
    }
    _ => todo!(),
  }
  if !valid {
    return Err(anyhow!("Invalid lambda_permissions.tf file"));
  }
  Ok(())
}

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
