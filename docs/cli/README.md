# cad-cli command reference

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

```console
cad-cli [OPTIONS] <COMMAND>
```

## Commands

| Command | Description |
|---|---|
| [`use`](use.md) | Show and switch the active profile, log in, and choose the company |
| [`activity-category`](activity-category.md) | Manage the company's activity categories |
| [`activity-group`](activity-group.md) | Manage the company's activity groups |
| [`benchmark`](benchmark.md) | Read and update the company benchmark |
| [`company`](company.md) | Inspect and create companies available to the current API key |
| [`company-template`](company-template.md) | Inspect reusable company templates available to the current API key |
| [`reporting-period`](reporting-period.md) | Manage reporting periods and duplication jobs |
| [`inventory-line`](inventory-line.md) | Read and mutate inventory lines |
| [`organizational-unit`](organizational-unit.md) | Manage the company's organizational units |
| [`document`](document.md) | Upload and download supporting documents |
| [`custom-field`](custom-field.md) | Manage custom fields |
| [`custom-option`](custom-option.md) | Manage options belonging to custom fields |
| [`report`](report.md) | Create and inspect generated reports |
| [`schema`](schema.md) | Print a machine-readable schema for a CLI resource |
| [`completions`](completions.md) | Print a shell completion script that also completes IDs from the platform |
| [`man`](man.md) | Print the manual page in roff format |
| [`update`](update.md) | Update cad-cli to the latest release |

## Global options

Every command accepts these options.

| Option | Description |
|---|---|
| `--profile <NAME>` | Use this profile instead of the active one (see `cad-cli use`). |
| `--server-url <SERVER_URL>` | Exact application origin, instead of the profile's. A stored login is used only with the profile whose server this is. |
| `--company-id <COMPANY_ID>` | Accessible company to act on, instead of the profile's (see `cad-cli use company`). |
| `--allow-insecure-http` | Permit HTTP for localhost/loopback mock servers. |
| `--request-timeout <REQUEST_TIMEOUT>` | Total time allowed for one API request, including its response body. Default: `60s`. |
| `--output <OUTPUT>` | Select human-readable or machine-readable output. Values: `table`, `json`, `jsonl`. Default: `table`. |
| `--yes` | Skip confirmation prompts for consequential operations. |

## Environment

| Variable | Description |
|---|---|
| `CAD_PROFILE` | Profile to use instead of the active one. --profile has precedence over it. |
| `CAD_API_KEY` | Machine API key ID (a UUID). When it or CAD_API_SECRET is set, the environment alone decides which key commands use; otherwise they use the login of the profile. |
| `CAD_API_SECRET` | Secret of the machine API key. Used with CAD_API_KEY, and instead of a stored login. |
| `CAD_SERVER_URL` | Exact application origin, instead of the profile's. --server-url has precedence over it. A stored login is used only with the profile whose server this is. |
| `CAD_COMPANY_ID` | Accessible company to act on when --company-id is not given. It has precedence over the company of the profile. |
| `CAD_CLI_KEYRING` | Set to 0 (or false, no, off) to never use the system keyring: logins are then kept in the credentials file only. For machines without a keyring. |
| `CAD_CLI_COMPLETE_OFFLINE` | Set to 1 to make shell completion answer from its cache only, without platform requests. |
| `RUST_LOG` | Diagnostic logging on stderr; cad_api=debug logs one line per platform request. |
| `NO_COLOR` | Disable colour in tables and status messages. |
| `CAD_CLI_NO_UPDATE_CHECK` | Set to 1 (or true, yes, on) to stop the once-a-day check for a newer release; empty, 0, false, no and off leave it on. The check is also off when CI is set to a true value, with --output json or jsonl, and when stderr is not a terminal. |

## Examples

List the profiles and show which one commands use:

```console
cad-cli use
```

Check with the server who the CLI acts as and on which company:

```console
cad-cli use --check
```

Choose the company the profile in use acts on:

```console
cad-cli use company "Acme GmbH"
```

Print the accessible companies as JSON:

```console
cad-cli company list --output json
```

Run a command for another accessible company:

```console
cad-cli --company-id 00000000-0000-0000-0000-000000000001 reporting-period list
```
