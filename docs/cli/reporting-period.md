# cad-cli reporting-period

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Manage reporting periods and duplication jobs.

```console
cad-cli reporting-period [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-reporting-period-list) | List reporting periods |
| [`get`](#cad-cli-reporting-period-get) | Get one reporting period |
| [`details`](#cad-cli-reporting-period-details) | Return aggregate information for one reporting period |
| [`create`](#cad-cli-reporting-period-create) | Create a reporting period from JSON --input <file\|-> and return it |
| [`update`](#cad-cli-reporting-period-update) | Update a reporting period from JSON --input <file\|-> and return it |
| [`delete`](#cad-cli-reporting-period-delete) | Delete a reporting period |
| [`duplicate`](#cad-cli-reporting-period-duplicate) | Start a reporting-period duplication job from JSON --input <file\|-> |
| [`duplication-status`](#cad-cli-reporting-period-duplication-status) | Inspect a duplication job |

Every command also accepts the [global options](README.md#global-options).

## cad-cli reporting-period list

List reporting periods.

```console
cad-cli reporting-period list [OPTIONS]
```

### Examples

List the reporting periods of the company:

```console
cad-cli reporting-period list
```

Print the reporting periods as JSON:

```console
cad-cli reporting-period list --output json
```

## cad-cli reporting-period get

Get one reporting period.

```console
cad-cli reporting-period get [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the reporting period. |

### Examples

Show one reporting period:

```console
cad-cli reporting-period get 00000000-0000-0000-0000-000000000001
```

## cad-cli reporting-period details

Return aggregate information for one reporting period.

```console
cad-cli reporting-period details [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the reporting period. |

### Examples

Show aggregate information for one reporting period:

```console
cad-cli reporting-period details 00000000-0000-0000-0000-000000000001
```

## cad-cli reporting-period create

Create a reporting period from JSON --input <file|-> and return it.

```console
cad-cli reporting-period create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create a reporting period from a file:

```console
cad-cli reporting-period create --input period.json
```

## cad-cli reporting-period update

Update a reporting period from JSON --input <file|-> and return it.

```console
cad-cli reporting-period update [OPTIONS] --input <INPUT> <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the reporting period. |

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Update a reporting period from a file:

```console
cad-cli reporting-period update 00000000-0000-0000-0000-000000000001 --input period.json
```

## cad-cli reporting-period delete

Delete a reporting period.

```console
cad-cli reporting-period delete [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the reporting period. |

### Examples

Delete a reporting period after a confirmation prompt:

```console
cad-cli reporting-period delete 00000000-0000-0000-0000-000000000001
```

Delete without prompting, e.g. from a script:

```console
cad-cli reporting-period delete 00000000-0000-0000-0000-000000000001 --yes
```

## cad-cli reporting-period duplicate

Start a reporting-period duplication job from JSON --input <file|->.

```console
cad-cli reporting-period duplicate [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |
| `--wait` | Wait until the remote job reaches a terminal state. |
| `--poll-interval <POLL_INTERVAL>` | Delay between job status requests. Default: `1500ms`. |
| `--timeout <TIMEOUT>` | Maximum time to wait for a terminal job state. Default: `30m`. |

### Examples

Start a duplication job and print its status:

```console
cad-cli reporting-period duplicate --input duplicate.json
```

Wait until the duplication job finishes:

```console
cad-cli reporting-period duplicate --input duplicate.json --wait
```

Wait up to an hour instead of the default 30 minutes:

```console
cad-cli reporting-period duplicate --input duplicate.json --wait --timeout 1h
```

## cad-cli reporting-period duplication-status

Inspect a duplication job.

```console
cad-cli reporting-period duplication-status [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Show the state of a duplication job:

```console
cad-cli reporting-period duplication-status 00000000-0000-0000-0000-000000000001
```
