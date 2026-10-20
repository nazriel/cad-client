# cad-cli schema

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Print a machine-readable schema for a CLI resource.

```console
cad-cli schema [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-schema-list) | List resources with schemas available from this CLI |
| [`activity-category`](#cad-cli-schema-activity-category) | Print the CLI schema for company activity categories |
| [`activity-group`](#cad-cli-schema-activity-group) | Print the CLI schema for company activity groups |
| [`company`](#cad-cli-schema-company) | Print the CLI schema for session-level companies |
| [`company-template`](#cad-cli-schema-company-template) | Print the CLI schema for reusable session-level company templates |
| [`benchmark`](#cad-cli-schema-benchmark) | Print the CLI schema for the singleton company benchmark |
| [`reporting-period`](#cad-cli-schema-reporting-period) | Print the CLI schema for company reporting periods |
| [`organizational-unit`](#cad-cli-schema-organizational-unit) | Print the CLI schema for company organizational units |

Every command also accepts the [global options](README.md#global-options).

## cad-cli schema list

List resources with schemas available from this CLI.

```console
cad-cli schema list [OPTIONS]
```

### Examples

List the resources that have a CLI schema:

```console
cad-cli schema list
```

## cad-cli schema activity-category

Print the CLI schema for company activity categories.

```console
cad-cli schema activity-category [OPTIONS]
```

### Examples

Print the activity-category command contract:

```console
cad-cli schema activity-category
```

## cad-cli schema activity-group

Print the CLI schema for company activity groups.

```console
cad-cli schema activity-group [OPTIONS]
```

### Examples

Print the activity-group command contract:

```console
cad-cli schema activity-group
```

## cad-cli schema company

Print the CLI schema for session-level companies.

```console
cad-cli schema company [OPTIONS]
```

### Examples

Print the company command contract:

```console
cad-cli schema company
```

## cad-cli schema company-template

Print the CLI schema for reusable session-level company templates.

```console
cad-cli schema company-template [OPTIONS]
```

### Examples

Print the company-template command contract:

```console
cad-cli schema company-template
```

## cad-cli schema benchmark

Print the CLI schema for the singleton company benchmark.

```console
cad-cli schema benchmark [OPTIONS]
```

### Examples

Save the benchmark command contract for an automation or LLM client:

```console
cad-cli schema benchmark > benchmark-cli-schema.json
```

## cad-cli schema reporting-period

Print the CLI schema for company reporting periods.

```console
cad-cli schema reporting-period [OPTIONS]
```

### Examples

Print the reporting-period command contract:

```console
cad-cli schema reporting-period
```

## cad-cli schema organizational-unit

Print the CLI schema for company organizational units.

```console
cad-cli schema organizational-unit [OPTIONS]
```

### Examples

Print the organizational-unit command contract:

```console
cad-cli schema organizational-unit
```
