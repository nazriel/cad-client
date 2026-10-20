# cad-cli activity-group

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Manage the company's activity groups.

```console
cad-cli activity-group [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-activity-group-list) | List activity groups for the selected company |
| [`create`](#cad-cli-activity-group-create) | Create an activity group from JSON --input <file\|-> and return it |
| [`update`](#cad-cli-activity-group-update) | Update an activity group from JSON --input <file\|-> and return it |
| [`delete`](#cad-cli-activity-group-delete) | Delete an activity group after confirmation |

Every command also accepts the [global options](README.md#global-options).

## cad-cli activity-group list

List activity groups for the selected company.

```console
cad-cli activity-group list [OPTIONS]
```

### Examples

List the activity groups of the company:

```console
cad-cli activity-group list
```

## cad-cli activity-group create

Create an activity group from JSON --input <file|-> and return it.

```console
cad-cli activity-group create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create an activity group from a file:

```console
cad-cli activity-group create --input group.json
```

## cad-cli activity-group update

Update an activity group from JSON --input <file|-> and return it.

```console
cad-cli activity-group update [OPTIONS] --input <INPUT> <ID>
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

Update an activity group from a file:

```console
cad-cli activity-group update 00000000-0000-0000-0000-000000000001 --input group.json
```

## cad-cli activity-group delete

Delete an activity group after confirmation.

```console
cad-cli activity-group delete [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Delete an activity group after a confirmation prompt:

```console
cad-cli activity-group delete 00000000-0000-0000-0000-000000000001
```

Delete without prompting, e.g. from a script:

```console
cad-cli activity-group delete 00000000-0000-0000-0000-000000000001 --yes
```
