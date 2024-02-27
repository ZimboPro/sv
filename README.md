# Service Validation

This tool helps to validate AWS Lambda, Terraform and OpenAPI configuration.

## Usage

`sv <COMMAND>`

###### **Subcommands:**

* `update` — Update the binary to the latest version
* `verify` — Verify the OpenAPI and Terraform files

## `sv update`

Update the binary to the latest version

**Usage:** `sv update`

## `sv verify`

Verify the OpenAPI and Terraform files

**Usage:** `sv verify [OPTIONS] --api-path <API_PATH> --terraform <TERRAFORM>`

###### **Options:**

* `-a`, `--api-path <API_PATH>` — The path to the OpenAPI files
* `-t`, `--terraform <TERRAFORM>` — The path to the Terraform files
* `-v`, `--verbose` — Verbose mode

* `--skip-cyclic` — Used to continue even if the CyclicRef error occurs


## Assumptions

### OpenAPI

The OpenAPI docs can a single one or several. The tool will validate them individually and then temporarily merge them into a single file and validate it. It is assumed that the *merged* OpenAPI file will be used as a template file by Terraform. It expects OpenAPI v3, v3.1 might be supported

### Terraform

It will be assumed that the following files will exist and have the following structure in the folder containing all the Terraform files. The order of the content doesn't really matter

 * lambda.tf will exist and have the following content
```terraform
locals {
  ...
  lambdas = {
    lambda-1 = { # <- This line
      ...
      handler     = "lambda_1.lambda_handler" # <- This line
      ...
    }
    lambda-2 = { # <- This line
      ...
      handler     = "lambda_get_faq.lambda_handler" # <- This line
      ...
    }
  }
  ...
}

module "lambda" {
  for_each = local.lambdas
  ...
}
```
 * lambda_permissions.tf will exist and have the following content
```terraform
locals {
  lambdas_permissions = {
    lambda-1 = [
      {
        statement_id = "AllowExecutionFromAPIGateway"
        principal    = "apigateway.amazonaws.com"
        source_arn   = "${module.service_api.rest_api_execution_arn}/*/POST/v1/lambda/endpoint1"
      }
    ],
    lambda-2 = [
      {
        statement_id = "AllowExecutionFromAPIGateway"
        principal    = "apigateway.amazonaws.com"
        source_arn   = "${module.service_api.rest_api_execution_arn}/*/POST/v1/lambda/endpoint2"
      }
    ],
  }
}
```
 * api_gateway.tf will exist and have the following content and reference the lambdas as shown below
```terraform
module "service_api" {
    ...

  api_config = {
    ...
    body = templatefile("${path.module}/../apis/out/service-api.yaml", { #
      region : var.region
      api_credentials : module.service_api_policy.role_arn
      lambda_1_arn : module.lambda["lambda-1"].lambda_arn, # <- This line
      lambda_2_arn : module.lambda["lambda-2"].lambda_arn, # <- This line
    })
    ...
  }
  ...
}
```

The OpenAPI docs can be multiple or a single file and have the following structure once merged

```yaml
openapi: "3.0.1"
info:
  ...
paths:
  /v1/lambda/endpoint1:
    post:
      ...
      x-amazon-apigateway-request-validator: validate-all
      x-amazon-apigateway-integration:
        httpMethod: "POST"
        uri: "arn:aws:apigateway:${region}:lambda:path/2015-03-31/functions/${lambda_1_arn}/invocations" # <- This line, the `${lambda_1_arn}` section
        passthroughBehavior: "when_no_match"
        timeoutInMillis: 5000
        type: "aws_proxy"

  /v1/lambda/endpoint2:
    post:
      ...
      x-amazon-apigateway-request-validator: validate-all
      x-amazon-apigateway-integration:
        httpMethod: "POST"
        uri: "arn:aws:apigateway:${region}:lambda:path/2015-03-31/functions/${lambda_2_arn}/invocations" # <- This line, the `${lambda_2_arn}` section
        passthroughBehavior: "when_no_match"
        timeoutInMillis: 5000
        type: "aws_proxy"
```
