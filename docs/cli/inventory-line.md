# cad-cli inventory-line

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Read and mutate inventory lines.

```console
cad-cli inventory-line [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-inventory-line-list) | List inventory lines using dynamic filters and sorting |
| [`get`](#cad-cli-inventory-line-get) | Get one inventory line |
| [`create`](#cad-cli-inventory-line-create) | Create an inventory line from JSON input |
| [`patch`](#cad-cli-inventory-line-patch) | Partially update an inventory line from JSON input |
| [`delete`](#cad-cli-inventory-line-delete) | Delete an inventory line |

Every command also accepts the [global options](README.md#global-options).

## cad-cli inventory-line list

List inventory lines using dynamic filters and sorting.

```console
cad-cli inventory-line list [OPTIONS]
```

### Options

| Option | Description |
|---|---|
| `--page <PAGE>` | Page to fetch, starting at 1. Default: `1`. |
| `--limit <LIMIT>` | Lines per page, at most 250. Default: `100`. |
| `--filter <FILTERS>` | Filter in `attribute_operator=value` form, e.g. `country_inList=PL,DE`; repeat as needed. |
| `--sort <SORT>` | Sort in `attribute_asc` or `attribute_desc` form, e.g. `date_desc`; repeat as needed. |

### Examples

List the first 100 inventory lines:

```console
cad-cli inventory-line list
```

Fetch the second page of 250 lines:

```console
cad-cli inventory-line list --page 2 --limit 250
```

Keep lines from Poland or Germany, newest first:

```console
cad-cli inventory-line list --filter country_inList=PL,DE --sort date_desc
```

## cad-cli inventory-line get

Get one inventory line.

```console
cad-cli inventory-line get [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Show one inventory line:

```console
cad-cli inventory-line get 00000000-0000-0000-0000-000000000001
```

## cad-cli inventory-line create

Create an inventory line from JSON input.

```console
cad-cli inventory-line create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create an inventory line from a file:

```console
cad-cli inventory-line create --input line.json
```

## cad-cli inventory-line patch

Partially update an inventory line from JSON input.

```console
cad-cli inventory-line patch [OPTIONS] --input <INPUT> <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Change only the fields present in the file:

```console
cad-cli inventory-line patch 00000000-0000-0000-0000-000000000001 --input changes.json
```

## cad-cli inventory-line delete

Delete an inventory line.

```console
cad-cli inventory-line delete [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Delete an inventory line after a confirmation prompt:

```console
cad-cli inventory-line delete 00000000-0000-0000-0000-000000000001
```

Delete without prompting, e.g. from a script:

```console
cad-cli inventory-line delete 00000000-0000-0000-0000-000000000001 --yes
```
