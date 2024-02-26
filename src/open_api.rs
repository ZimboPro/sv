use anyhow::anyhow;

use merge_yaml_hash::MergeYamlHash;
use oapi::{OApi, OApiTag};
use simplelog::{debug, error, info, warn};
use sppparse::{SparseError, SparseRoot};

use std::{f32::consts::E, ffi::OsStr, io::Read, path::PathBuf};

use core::fmt::Display;

use crate::util::HttpMethod;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAPIData {
  pub path: String,
  pub method: HttpMethod,
  pub uri: String,
  pub execution_type: APIType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum APIType {
  Lambda,
  StepFunction,
  SQS,
}

impl Display for APIType {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      APIType::Lambda => write!(f, "Lambda"),
      APIType::StepFunction => write!(f, "Step Function"),
      APIType::SQS => write!(f, "SQS"),
    }
  }
}

pub fn validate_open_api(api_path: PathBuf, skip_cyclic: bool) -> anyhow::Result<Vec<OpenAPIData>> {
  info!("Validating OpenAPI documents");

  let mut files = find_files(api_path.as_path(), OsStr::new("yml"));
  files.append(&mut find_files(api_path.as_path(), OsStr::new("yaml")));
  let mut tags = Vec::new();
  let mut valid = true;
  let shared = files.iter().find(|file| {
    let file_name = file
      .file_stem()
      .expect("Failed to get file name")
      .to_str()
      .expect("Failed to convert file name to string");
    file_name == "shared-schemas" || file_name == "shared"
  });
  for file in &files {
    debug!(
      "Validating OpenAPI document {:?}",
      file.file_name().expect("Failed to get file name")
    );
    if let Some(shared) = shared {
      if file == shared {
        continue;
      }
      let shared_contents = open_file(shared.to_path_buf());
      let file_contents = open_file(file.to_path_buf());
      let merged_content = merge(vec![shared_contents, file_contents]);
      let merged_file = temp_file::with_contents(merged_content.as_bytes());
      validate_file(
        merged_file.path().to_path_buf(),
        file.to_path_buf(),
        &mut tags,
        &mut valid,
        skip_cyclic,
      );
    } else {
      // PathBuf::from_iter([
      //   std::env::current_dir().expect("Failed to get current directory"),
      //   file.to_path_buf(),
      // ])
      validate_file(
        PathBuf::from_iter([
          std::env::current_dir().expect("Failed to get current directory"),
          file.to_path_buf(),
        ]),
        file.to_path_buf(),
        &mut tags,
        &mut valid,
        skip_cyclic,
      );
    };
    // match SparseRoot::new_from_file(path) {
    //   Ok(open_api_doc) => {
    //     let doc: OApi = OApi::new(open_api_doc);
    //     if let Err(e) = doc.check() {
    //       valid = false;
    //       error!(
    //         "API document {:?} is not valid: {}",
    //         file.file_name().expect("Failed to get file name"),
    //         e
    //       );
    //     } else {
    //       debug!(
    //         "API document {:?} is valid",
    //         file.file_name().expect("Failed to get file name")
    //       );
    //       let root = doc.root_get().expect("Failed to get OpenAPI root");
    //       if let Some(file_tags) = root.tags() {
    //         tags.append(&mut file_tags.clone());
    //       }
    //     }
    //   }
    //   Err(e) => {
    //     valid = false;
    //     error!(
    //       "API document {:?} was not able to be parsed: {}",
    //       file.file_name().expect("Failed to get file name"),
    //       e
    //     );
    //   }
    // }
  }

  if !valid {
    return Err(anyhow!("Invalid OpenAPI documents"));
  }

  debug!("Validating tags");
  if tags.len() > 1 {
    let mut index = 0;
    while index < tags.len() - 1 {
      let tag = tags.get(index).expect("Failed to get tag");
      let mut j = index + 1;
      while j < tags.len() {
        let t = tags.get(j).expect("Failed to get tag");
        if tag.name() == t.name() && tag.description() == t.description() {
          valid = false;
          error!(
            "Duplicate tags: Name: {}\nDescription: {:?}",
            t.name(),
            t.description()
          );
        }
        j += 1;
      }
      index += 1;
    }

    if !valid {
      return Err(anyhow!("Duplicate tags"));
    }
  }

  if files.len() > 1 {
    info!("Validating combined OpenAPI documents");
    let mut files_content = Vec::new();
    for file in files {
      files_content.push(open_file(file));
    }
    let merged_content = merge(files_content);
    let merged_file = temp_file::with_contents(merged_content.as_bytes());
    match SparseRoot::new_from_file(merged_file.path().to_path_buf()) {
      Ok(s) => {
        let doc: OApi = OApi::new(s);

        doc.check().expect("not to have logic errors");
        Ok(extract_api_data(merged_content)?)
      }
      Err(e) => match e {
        SparseError::CyclicRef => {
          if skip_cyclic {
            warn!("Merged API document was not able to be parsed: {}", e);
            Ok(extract_api_data(merged_content)?)
          } else {
            Err(anyhow!(
              "Merged API document was not able to be parsed: {}",
              e
            ))
          }
        }
        _ => {
          return Err(anyhow!(
            "Failed to validate combined OpenAPI documents: {}",
            e
          ));
        }
      },
    }
  } else {
    Ok(extract_api_data(open_file(
      files.get(0).expect("Failed to get file path").to_path_buf(),
    ))?)
  }
}

fn validate_file(
  path: PathBuf,
  file: PathBuf,
  tags: &mut Vec<OApiTag>,
  valid: &mut bool,
  skip_cyclic: bool,
) {
  match SparseRoot::new_from_file(path) {
    Ok(open_api_doc) => {
      let doc: OApi = OApi::new(open_api_doc);
      if let Err(e) = doc.check() {
        *valid = false;
        error!(
          "API document {:?} is not valid: {}",
          file.file_name().expect("Failed to get file name"),
          e
        );
      } else {
        debug!(
          "API document {:?} is valid",
          file.file_name().expect("Failed to get file name")
        );
        let root = doc.root_get().expect("Failed to get OpenAPI root");
        if let Some(file_tags) = root.tags() {
          tags.append(&mut file_tags.clone());
        }
      }
    }
    Err(e) => match e {
      SparseError::CyclicRef => {
        if skip_cyclic {
          warn!(
            "API document {:?} was not able to be parsed: {}",
            file.file_name().expect("Failed to get file name"),
            e
          );
          return;
        } else {
          *valid = false;
          error!(
            "API document {:?} was not able to be parsed: {}",
            file.file_name().expect("Failed to get file name"),
            e
          );
        }
      }
      _ => {
        *valid = false;
        error!(
          "API document {:?} was not able to be parsed: {}",
          file.file_name().expect("Failed to get file name"),
          e
        );
      }
    },
  }
}

fn open_file(filename: PathBuf) -> String {
  let mut file = std::fs::File::open(filename).expect("Couldn't find or open the file");
  let mut contents = String::new();
  file
    .read_to_string(&mut contents)
    .expect("Couldn't read the contents of the file");
  contents
}

fn merge(files: Vec<String>) -> String {
  let mut hash = MergeYamlHash::new();
  debug!("Merging OpenAPI documents");
  for file in files {
    debug!("Merging file {:?}", file);
    hash.merge(&file);
  }

  hash.to_string()
}

fn find_files(path: &std::path::Path, extension: &OsStr) -> Vec<PathBuf> {
  debug!("Finding files in {:?}", path);
  let mut files = Vec::new();
  for entry in path.read_dir().expect("Failed to read directory").flatten() {
    if entry.path().is_dir() {
      debug!("Found directory {:?}", entry.path());
      files.append(&mut find_files(&entry.path(), extension));
    } else if entry.path().extension() == Some(extension) {
      debug!("Found file {:?}", entry.path());
      files.push(entry.path());
    }
  }
  files
}

fn extract_api_data_for_item(
  item: &openapiv3::Operation,
  path: &str,
  method: HttpMethod,
) -> anyhow::Result<OpenAPIData> {
  debug!("Method: {}", method);
  let aws = item
    .extensions
    .get("x-amazon-apigateway-integration")
    .expect("Expected 'x-amazon-apigateway-integration' extension");
  let uri = aws
    .get("uri")
    .expect("Expected 'uri' in 'x-amazon-apigateway-integration' extension");
  let uri_path = uri.as_str().expect("Failed to convert URI to string");
  debug!("URI: {}", uri_path);
  match method {
    HttpMethod::Get => {}
    HttpMethod::Post | HttpMethod::Put | HttpMethod::Patch => {
      if item.request_body.is_none() && item.parameters.is_empty() {
        warn!("The {} method for {} does not have a request body or parameters (queries)", method.to_string(), path);
      }
    },
    HttpMethod::Delete => {}
    HttpMethod::Options => warn!("Double check if OPTIONS method for {} should have a request body and/or parameters (queries)", path),
    x => return Err(anyhow!("Http method should not be used: {}", x.to_string())),
  }
  let api_type = match uri_path {
    x if x.contains("states:action") => APIType::StepFunction,
    x if x.contains("lambda:path") => APIType::Lambda,
    x if x.contains("sqs:action") => APIType::SQS,
    _ => {
      return Err(anyhow!(
        "Unknown execution type for URI: {}",
        uri_path.to_string()
      ))
    }
  };
  debug!("API execution type: {}", api_type);
  Ok(OpenAPIData {
    path: path.to_string(),
    method,
    uri: uri_path.to_string(),
    execution_type: api_type,
  })
}

fn extract_api_data(content: String) -> anyhow::Result<Vec<OpenAPIData>> {
  let mut data = Vec::new();
  let doc: openapiv3::OpenAPI = serde_yaml::from_str(&content)?;
  let paths = doc.paths;
  for (path, path_item) in paths.paths {
    debug!("Extracting Path data: {}", path);
    if let Some(get) = &path_item.as_item().unwrap().get {
      data.push(extract_api_data_for_item(get, &path, HttpMethod::Get)?);
    }
    if let Some(post) = &path_item.as_item().unwrap().post {
      data.push(extract_api_data_for_item(post, &path, HttpMethod::Post)?);
    }
    if let Some(put) = &path_item.as_item().unwrap().put {
      data.push(extract_api_data_for_item(put, &path, HttpMethod::Put)?);
    }
    if let Some(patch) = &path_item.as_item().unwrap().patch {
      data.push(extract_api_data_for_item(patch, &path, HttpMethod::Patch)?);
    }
    if let Some(delete) = &path_item.as_item().unwrap().delete {
      data.push(extract_api_data_for_item(
        delete,
        &path,
        HttpMethod::Delete,
      )?);
    }
  }
  Ok(data)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_extract_api_data() {
    let content = r#"
openapi: 3.0.0
info:
  title: Test
  version: 1.0.0
paths:
  /test:
    get:
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
    post:
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                test:
                  type: string
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
    put:
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                test:
                  type: string
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
    patch:
      requestBody:
        content:
          application/json:
            schema:
              type: object
              properties:
                test:
                  type: string
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
    delete:
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
"#;
    let data = extract_api_data(content.to_string()).expect("Failed to extract API data");
    assert_eq!(data.len(), 5);
    assert_eq!(data[0].path, "/test");
    assert_eq!(data[0].method, HttpMethod::Get);
    assert_eq!(
      data[0].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[0].execution_type, APIType::Lambda);
    assert_eq!(data[1].path, "/test");
    assert_eq!(data[1].method, HttpMethod::Post);
    assert_eq!(
      data[1].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[1].execution_type, APIType::Lambda);
    assert_eq!(data[2].path, "/test");
    assert_eq!(data[2].method, HttpMethod::Put);
    assert_eq!(
      data[2].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[2].execution_type, APIType::Lambda);
    assert_eq!(data[3].path, "/test");
    assert_eq!(data[3].method, HttpMethod::Patch);
    assert_eq!(
      data[3].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[3].execution_type, APIType::Lambda);
    assert_eq!(data[4].path, "/test");
    assert_eq!(data[4].method, HttpMethod::Delete);
    assert_eq!(
      data[4].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[4].execution_type, APIType::Lambda);
  }

  #[test]
  fn test_extract_api_data_with_parameters() {
    let content = r#"
openapi: 3.0.0
info:
  title: Test
  version: 1.0.0
paths:
  /test:
    get:
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
    post:
      parameters:
        - name: test
          in: query
          schema:
            type: string
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
    put:
      parameters:
      - name: test
        in: query
        schema:
          type: string
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
    patch:
      parameters:
      - name: test
        in: query
        schema:
          type: string
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
    delete:
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations
        httpMethod: POST
        type: aws_proxy
"#;
    let data = extract_api_data(content.to_string()).expect("Failed to extract API data");
    assert_eq!(data.len(), 5);
    assert_eq!(data[0].path, "/test");
    assert_eq!(data[0].method, HttpMethod::Get);
    assert_eq!(
      data[0].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[0].execution_type, APIType::Lambda);
    assert_eq!(data[1].path, "/test");
    assert_eq!(data[1].method, HttpMethod::Post);
    assert_eq!(
      data[1].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[1].execution_type, APIType::Lambda);
    assert_eq!(data[2].path, "/test");
    assert_eq!(data[2].method, HttpMethod::Put);
    assert_eq!(
      data[2].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[2].execution_type, APIType::Lambda);
    assert_eq!(data[3].path, "/test");
    assert_eq!(data[3].method, HttpMethod::Patch);
    assert_eq!(
      data[3].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[3].execution_type, APIType::Lambda);
    assert_eq!(data[4].path, "/test");
    assert_eq!(data[4].method, HttpMethod::Delete);
    assert_eq!(
      data[4].uri,
      "arn:aws:apigateway:us-east-1:lambda:path/2015-03-31/functions/arn:aws:lambda:us-east-1:123456789012:function:Test/invocations"
    );
    assert_eq!(data[4].execution_type, APIType::Lambda);
  }

  #[test]
  fn test_extract_api_data_post_with_no_request_body() {
    let content = r#"
openapi: 3.0.0
info:
  title: Test
  version: 1.0.0
paths:
  /test:
    post:
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:states:action/StartExecution
        httpMethod: POST
        type: aws_proxy
"#;
    let data = extract_api_data(content.to_string());
    assert!(data.is_err());
    assert_eq!(
      data.err().unwrap().to_string(),
      "The POST method for /test does not have a request body or parameters (queries)"
    );
  }

  #[test]
  fn test_extract_api_data_put_with_no_request_body() {
    let content = r#"
openapi: 3.0.0
info:
  title: Test
  version: 1.0.0
paths:
  /test:
    put:
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:states:action/StartExecution
        httpMethod: POST
        type: aws_proxy
"#;
    let data = extract_api_data(content.to_string());
    assert!(data.is_err());
    assert_eq!(
      data.err().unwrap().to_string(),
      "The PUT method for /test does not have a request body or parameters (queries)"
    );
  }

  #[test]
  fn test_extract_api_data_patch_with_no_request_body() {
    let content = r#"
openapi: 3.0.0
info:
  title: Test
  version: 1.0.0
paths:
  /test:
    patch:
      responses:
        '200':
          description: OK
      x-amazon-apigateway-integration:
        uri: arn:aws:apigateway:us-east-1:states:action/StartExecution
        httpMethod: POST
        type: aws_proxy
"#;
    let data = extract_api_data(content.to_string());
    assert!(data.is_err());
    assert_eq!(
      data.err().unwrap().to_string(),
      "The PATCH method for /test does not have a request body or parameters (queries)"
    );
  }
}
