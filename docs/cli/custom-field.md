# cad-cli custom-field

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Manage custom fields.

```console
cad-cli custom-field [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-custom-field-list) | List custom fields ordered by position |
| [`create`](#cad-cli-custom-field-create) | Create a custom field from JSON input |
| [`replace`](#cad-cli-custom-field-replace) | Fully replace a custom field from JSON input |
| [`delete`](#cad-cli-custom-field-delete) | Delete a custom field |

Every command also accepts the [global options](README.md#global-options).

## cad-cli custom-field list

List custom fields ordered by position.

```console
cad-cli custom-field list [OPTIONS]
```

### Examples

List the custom fields in display order:

```console
cad-cli custom-field list
```

## cad-cli custom-field create

Create a custom field from JSON input.

```console
cad-cli custom-field create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create a custom field from a file:

```console
cad-cli custom-field create --input field.json
```

## cad-cli custom-field replace

Fully replace a custom field from JSON input.

```console
cad-cli custom-field replace [OPTIONS] --input <INPUT> <ID>
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

Replace every property of a custom field:

```console
cad-cli custom-field replace 00000000-0000-0000-0000-000000000002 --input field.json
```

## cad-cli custom-field delete

Delete a custom field.

```console
cad-cli custom-field delete [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Delete a custom field after a confirmation prompt:

```console
cad-cli custom-field delete 00000000-0000-0000-0000-000000000002
```

Delete without prompting, e.g. from a script:

```console
cad-cli custom-field delete 00000000-0000-0000-0000-000000000002 --yes
```
