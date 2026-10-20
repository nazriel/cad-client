# cad-cli custom-option

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Manage options belonging to custom fields.

```console
cad-cli custom-option [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-custom-option-list) | List custom options, optionally filtering by field |
| [`create`](#cad-cli-custom-option-create) | Create a custom option from JSON input |
| [`create-batch`](#cad-cli-custom-option-create-batch) | Create up to 50 custom options from a JSON array |
| [`replace`](#cad-cli-custom-option-replace) | Fully replace a custom option from JSON input |
| [`delete`](#cad-cli-custom-option-delete) | Delete a custom option |

Every command also accepts the [global options](README.md#global-options).

## cad-cli custom-option list

List custom options, optionally filtering by field.

```console
cad-cli custom-option list [OPTIONS]
```

### Options

| Option | Description |
|---|---|
| `--field-id <FIELD_ID>` | Keep only options of this field. The API has no option filter, so every option is fetched and the selection happens locally. |

### Examples

List the options of every custom field:

```console
cad-cli custom-option list
```

List the options of one custom field:

```console
cad-cli custom-option list --field-id 00000000-0000-0000-0000-000000000002
```

## cad-cli custom-option create

Create a custom option from JSON input.

```console
cad-cli custom-option create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create a custom option from a file:

```console
cad-cli custom-option create --input option.json
```

## cad-cli custom-option create-batch

Create up to 50 custom options from a JSON array.

```console
cad-cli custom-option create-batch [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create up to 50 options from a JSON array:

```console
cad-cli custom-option create-batch --input options.json
```

## cad-cli custom-option replace

Fully replace a custom option from JSON input.

```console
cad-cli custom-option replace [OPTIONS] --input <INPUT> <ID>
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

Replace every property of a custom option:

```console
cad-cli custom-option replace 00000000-0000-0000-0000-000000000001 --input option.json
```

## cad-cli custom-option delete

Delete a custom option.

```console
cad-cli custom-option delete [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Delete a custom option after a confirmation prompt:

```console
cad-cli custom-option delete 00000000-0000-0000-0000-000000000001
```

Delete without prompting, e.g. from a script:

```console
cad-cli custom-option delete 00000000-0000-0000-0000-000000000001 --yes
```
