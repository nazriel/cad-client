# cad-cli activity-category

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Manage the company's activity categories.

```console
cad-cli activity-category [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-activity-category-list) | List activity categories for the selected company, including measurement counts |
| [`update`](#cad-cli-activity-category-update) | Update an activity category from JSON --input <file\|-> and return it |
| [`delete`](#cad-cli-activity-category-delete) | Delete an activity category after confirmation |
| [`duplicate`](#cad-cli-activity-category-duplicate) | Duplicate an activity category and return the copied category |

Every command also accepts the [global options](README.md#global-options).

## cad-cli activity-category list

List activity categories for the selected company, including measurement counts.

```console
cad-cli activity-category list [OPTIONS]
```

### Examples

List activity categories with their measurement counts:

```console
cad-cli activity-category list
```

## cad-cli activity-category update

Update an activity category from JSON --input <file|-> and return it.

```console
cad-cli activity-category update [OPTIONS] --input <INPUT> <ID>
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

Update an activity category from a file:

```console
cad-cli activity-category update 00000000-0000-0000-0000-000000000001 --input category.json
```

## cad-cli activity-category delete

Delete an activity category after confirmation.

```console
cad-cli activity-category delete [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Delete an activity category after a confirmation prompt:

```console
cad-cli activity-category delete 00000000-0000-0000-0000-000000000001
```

Delete without prompting, e.g. from a script:

```console
cad-cli activity-category delete 00000000-0000-0000-0000-000000000001 --yes
```

## cad-cli activity-category duplicate

Duplicate an activity category and return the copied category.

```console
cad-cli activity-category duplicate [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Copy an activity category and print the copy:

```console
cad-cli activity-category duplicate 00000000-0000-0000-0000-000000000001
```
