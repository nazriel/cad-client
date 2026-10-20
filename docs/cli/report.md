# cad-cli report

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Create and inspect generated reports.

```console
cad-cli report [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-report-list) | List reports ordered from newest to oldest |
| [`get`](#cad-cli-report-get) | Get one report and its progress counters |
| [`create`](#cad-cli-report-create) | Create a report from JSON input |

Every command also accepts the [global options](README.md#global-options).

## cad-cli report list

List reports ordered from newest to oldest.

```console
cad-cli report list [OPTIONS]
```

### Options

| Option | Description |
|---|---|
| `--include-snapshots` | Also show snapshot-generated reports. The API returns the full list, so this filter and the newest-first ordering are applied locally. |

### Examples

List reports, newest first:

```console
cad-cli report list
```

Include reports generated from snapshots:

```console
cad-cli report list --include-snapshots
```

## cad-cli report get

Get one report and its progress counters.

```console
cad-cli report get [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Show one report and its progress:

```console
cad-cli report get 00000000-0000-0000-0000-000000000001
```

## cad-cli report create

Create a report from JSON input.

```console
cad-cli report create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |
| `--wait` | Wait until the remote job reaches a terminal state. |
| `--poll-interval <POLL_INTERVAL>` | Delay between job status requests. Default: `1500ms`. |
| `--timeout <TIMEOUT>` | Maximum time to wait for a terminal job state. Default: `30m`. |

### Examples

Start a report and print it right away:

```console
cad-cli report create --input report.json
```

Wait until the report is generated:

```console
cad-cli report create --input report.json --wait
```

Check the report every five seconds while waiting:

```console
cad-cli report create --input report.json --wait --poll-interval 5s
```
