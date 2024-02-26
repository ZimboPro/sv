# Command-Line Help for `sv`

This document contains the help content for the `sv` command-line program.

**Command Overview:**

* [`sv`↴](#sv)
* [`sv update`↴](#sv-update)
* [`sv verify`↴](#sv-verify)

## `sv`

**Usage:** `sv <COMMAND>`

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

  Possible values: `true`, `false`

* `--skip-cyclic` — Used to continue even if the CyclicRef error occurs

  Possible values: `true`, `false`




<hr/>

<small><i>
    This document was generated automatically by
    <a href="https://crates.io/crates/clap-markdown"><code>clap-markdown</code></a>.
</i></small>

