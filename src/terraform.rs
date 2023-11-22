use std::{collections::btree_map::Keys, path::PathBuf};

use anyhow::anyhow;
use anyhow::Ok;

use paris::error;
use paris::info;
use paris::warn;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Lambda {
    key: String,
    handler: String,
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

pub fn validate_terraform(terraform: PathBuf) -> anyhow::Result<Vec<String>> {
    let lambda = terraform.join("lambda.tf");
    let lambda_permissions = terraform.join("lambda_permissions.tf");
    let api_gw = terraform.join("api_gateway.tf");
    let mut lambda_metadata: Vec<Lambda> = Vec::new();
    let mut keys = Vec::new();
    if lambda.exists() {
        lambda_metadata = validate_lambda(lambda)?;
    } else {
        return Err(anyhow!("File lambda.tf doesn't exist in {:?}", terraform));
    }
    if lambda_permissions.exists() {
        validate_lambda_permissions(lambda_permissions, &lambda_metadata)?;
    } else {
        return Err(anyhow!(
            "File lambda_permissions.tf doesn't exist in {:?}",
            terraform
        ));
    }
    if api_gw.exists() {
        keys = extract_api_gw(api_gw, lambda_metadata)?;
    } else {
        return Err(anyhow!(
            "File api_gateway.tf doesn't exist in {:?}",
            terraform
        ));
    }
    Ok(keys)
}

fn validate_lambda(lambda: PathBuf) -> anyhow::Result<Vec<Lambda>> {
    info!("Validating lambda.tf");
    let mut lambda_metadata: Vec<Lambda> = Vec::new();
    let mut valid = true;
    let lambda_contents = std::fs::read_to_string(lambda)?;
    let body = hcl::parse(&lambda_contents)?;
    let locals = body
        .blocks()
        .find(|x| x.identifier.to_string() == "locals".to_string())
        .unwrap();
    let lambdas = locals
        .body
        .attributes()
        .find(|x| x.key.to_string() == "lambdas".to_string())
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
                                    if data_key.to_string().to_lowercase()
                                        == "handler".to_lowercase()
                                    {
                                        return Some(data_item.1.to_string().replace("\"", ""));
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
            // if lambda_contents.matches(meta.key).count() > 1 {}
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
    keys: &Vec<Lambda>,
) -> anyhow::Result<()> {
    info!("Validating lambda_permissions.tf");
    let mut valid = true;
    let lambda_contents = std::fs::read_to_string(lambda_permissions)?;
    let body = hcl::parse(&lambda_contents)?;
    let locals = body
        .blocks()
        .find(|x| x.identifier.to_string() == "locals".to_string())
        .unwrap();
    let lambdas = locals
        .body
        .attributes()
        .find(|x| x.key.to_string() == "lambdas_permissions".to_string())
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
            for key in keys {
                if !p_keys.contains(&key.key) {
                    valid = false;
                    error!("'lambda_permissions' doesn't have {}", key.key);
                }
            }
            for key in p_keys {
                if keys.iter().find(|x| x.key == key).is_none() {
                    valid = false;
                    error!("'lambda_permissions' has extra key '{}'", key);
                }
                if lambda_contents.matches(&key).count() > 1 {
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

struct ArnLambda {
    key: String,
    lambda_key: String,
}

fn extract_api_gw(api_gw: PathBuf, lambda: Vec<Lambda>) -> anyhow::Result<Vec<String>> {
    let contents = std::fs::read_to_string(api_gw)?;
    {
        let _ = hcl::parse(&contents)?;
    }
    let lines = contents.lines();
    let mut template_names = Vec::new();
    let names: Vec<String> = lambda.iter().map(|x| x.key.clone()).collect();
    for line in lines {
        for name in &names {
            if line.contains(name)
                && !line.trim().starts_with("#")
                && !line.trim().starts_with("//")
            {
                let parts: Vec<&str> = line.split(":").collect();
                template_names.push(ArnLambda {
                    key: parts[0].trim().to_string(),
                    lambda_key: name.clone(),
                });
                break;
            }
        }
    }
    let mut valid = true;
    for name in names {
        let len = template_names
            .iter()
            .filter(|x| x.lambda_key == name)
            .count();
        if len > 1 {
            valid = false;
            error!("The lambda key '{}' is used more than once", name);
        } else if len == 0 {
            warn!("WARNING: The lambda key is not used at all: {}", name);
        }
    }
    if !valid {
        return Err(anyhow!("Invalid api_gateway.tf"));
    }
    Ok(template_names.into_iter().map(|x| x.key).collect())
}
