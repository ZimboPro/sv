use anyhow::anyhow;
use clap::Parser;
use merge_yaml_hash::MergeYamlHash;
use oapi::{OApi, OApiDocument};
use paris::{error, info};
use sppparse::SparseRoot;
use std::io::{self, Write};
use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
};

pub fn validate_open_api(api_path: PathBuf) -> anyhow::Result<String> {
    info!("Validating OpenAPI documents");

    let mut files = find_files(api_path.as_path(), OsStr::new("yml"));
    files.append(&mut find_files(api_path.as_path(), OsStr::new("yaml")));
    let mut tags = Vec::new();
    let mut valid = true;
    for file in &files {
        match SparseRoot::new_from_file(PathBuf::from_iter([
            std::env::current_dir().unwrap(),
            file.to_path_buf(),
        ])) {
            Ok(open_api_doc) => {
                let doc: OApi = OApi::new(open_api_doc);
                if let Err(e) = doc.check() {
                    valid = false;
                    error!(
                        "API document {:?} is not valid: {}",
                        file.file_name().unwrap(),
                        e
                    );
                } else {
                    let root = doc.root_get().unwrap();
                    if let Some(file_tags) = root.tags() {
                        tags.append(&mut file_tags.clone());
                    }
                }
            }
            Err(e) => {
                valid = false;
                error!(
                    "API document {:?} was not able to be parsed: {}",
                    file.file_name().unwrap(),
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
            let tag = tags.get(index).unwrap();
            let mut j = index + 1;
            while j < tags.len() {
                let t = tags.get(j).unwrap();
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
            SparseRoot::new_from_file(merged_file.path().to_path_buf())
                .expect("to parse the OpenAPI"),
        );

        doc.check().expect("not to have logic errors");
        Ok(merged_content)
    } else {
        Ok(open_file(files.get(0).unwrap().to_path_buf()))
    }
}

fn open_file(filename: PathBuf) -> String {
    let mut file = std::fs::File::open(filename).expect("Couldn't find or open the file");
    let mut contents = String::new();
    file.read_to_string(&mut contents)
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
    for entry in path.read_dir().expect("Failed to read directory") {
        if let Ok(entry) = entry {
            if entry.path().is_dir() {
                files.append(&mut find_files(&entry.path(), extension));
            } else if entry.path().extension() == Some(extension) {
                files.push(entry.path());
            }
        }
    }
    files
}
