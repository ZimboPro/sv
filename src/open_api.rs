use anyhow::anyhow;

use merge_yaml_hash::MergeYamlHash;
use oapi::OApi;
use paris::{error, info};
use sppparse::SparseRoot;

use std::{ffi::OsStr, io::Read, path::PathBuf};

use crate::util::HttpMethod;

#[derive(Debug, Clone, PartialEq, Eq)]
struct OpenAPIData {
  path: String,
  method: HttpMethod,
  uri: String,
  execution_type: APIType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum APIType {
  ARN,
  StepFunction,
}

pub fn validate_open_api(api_path: PathBuf) -> anyhow::Result<String> {
  info!("Validating OpenAPI documents");

  let mut files = find_files(api_path.as_path(), OsStr::new("yml"));
  files.append(&mut find_files(api_path.as_path(), OsStr::new("yaml")));
  let mut tags = Vec::new();
  let mut valid = true;
  for file in &files {
    match SparseRoot::new_from_file(PathBuf::from_iter([
      std::env::current_dir().expect("Failed to get current directory"),
      file.to_path_buf(),
    ])) {
      Ok(open_api_doc) => {
        let doc: OApi = OApi::new(open_api_doc);
        if let Err(e) = doc.check() {
          valid = false;
          error!(
            "API document {:?} is not valid: {}",
            file.file_name().expect("Failed to get file name"),
            e
          );
        } else {
          let root = doc.root_get().expect("Failed to get OpenAPI root");
          if let Some(file_tags) = root.tags() {
            tags.append(&mut file_tags.clone());
          }
        }
      }
      Err(e) => {
        valid = false;
        error!(
          "API document {:?} was not able to be parsed: {}",
          file.file_name().expect("Failed to get file name"),
          e
        );
      }
    }
  }

  if !valid {
    return Err(anyhow!("Invalid OpenAPI documents"));
  }

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
    let doc: OApi = OApi::new(
      SparseRoot::new_from_file(merged_file.path().to_path_buf()).expect("to parse the OpenAPI"),
    );

    doc.check().expect("not to have logic errors");
    Ok(merged_content)
  } else {
    Ok(open_file(
      files.get(0).expect("Failed to get file path").to_path_buf(),
    ))
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

  for file in files {
    hash.merge(&file);
  }

  hash.to_string()
}

fn find_files(path: &std::path::Path, extension: &OsStr) -> Vec<PathBuf> {
  let mut files = Vec::new();
  for entry in path.read_dir().expect("Failed to read directory").flatten() {
    if entry.path().is_dir() {
      files.append(&mut find_files(&entry.path(), extension));
    } else if entry.path().extension() == Some(extension) {
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
  let aws = item
    .extensions
    .get("x-amazon-apigateway-integration")
    .expect("Expected 'x-amazon-apigateway-integration' extension");
  let uri = aws
    .get("uri")
    .expect("Expected 'uri' in 'x-amazon-apigateway-integration' extension");
  let uri_path = uri.as_str().expect("Failed to convert URI to string");
  if uri_path.contains("states:action") {
    Ok(OpenAPIData {
      path: path.to_string(),
      method,
      uri: uri_path.to_string(),
      execution_type: APIType::StepFunction,
    })
  } else {
    Ok(OpenAPIData {
      path: path.to_string(),
      method,
      uri: uri_path.to_string(),
      execution_type: APIType::ARN,
    })
  }
}

fn extract_api_data(content: String) -> anyhow::Result<Vec<OpenAPIData>> {
  let mut data = Vec::new();
  let doc: openapiv3::OpenAPI = serde_yaml::from_str(&content)?;
  let paths = doc.paths;
  for (path, path_item) in paths.paths {
    if let Some(get) = &path_item.as_item().unwrap().get {
      data.push(extract_api_data_for_item(&get, &path, HttpMethod::Get)?);
    }
    if let Some(post) = &path_item.as_item().unwrap().post {
      data.push(extract_api_data_for_item(&post, &path, HttpMethod::Post)?);
    }
    if let Some(put) = &path_item.as_item().unwrap().put {
      data.push(extract_api_data_for_item(&put, &path, HttpMethod::Put)?);
    }
    if let Some(patch) = &path_item.as_item().unwrap().patch {
      data.push(extract_api_data_for_item(&patch, &path, HttpMethod::Patch)?);
    }
    if let Some(delete) = &path_item.as_item().unwrap().delete {
      data.push(extract_api_data_for_item(
        &delete,
        &path,
        HttpMethod::Delete,
      )?);
    }
  }
  Ok(data)
}
