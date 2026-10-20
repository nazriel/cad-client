# cad-cli organizational-unit

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Manage the company's organizational units.

```console
cad-cli organizational-unit [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-organizational-unit-list) | List organizational units for the selected company |
| [`get`](#cad-cli-organizational-unit-get) | Get one organizational unit |
| [`create`](#cad-cli-organizational-unit-create) | Create an organizational unit from JSON --input <file\|-> and return it |
| [`update`](#cad-cli-organizational-unit-update) | Update an organizational unit from JSON --input <file\|-> and return it |
| [`delete`](#cad-cli-organizational-unit-delete) | Delete an organizational unit after confirmation |
| [`count-entries`](#cad-cli-organizational-unit-count-entries) | Count inventory entries assigned to one organizational unit |
| [`duplicate`](#cad-cli-organizational-unit-duplicate) | Duplicate an organizational unit and optionally all descendants |

Every command also accepts the [global options](README.md#global-options).

## cad-cli organizational-unit list

List organizational units for the selected company.

```console
cad-cli organizational-unit list [OPTIONS]
```

### Examples

List the organizational units of the company:

```console
cad-cli organizational-unit list
```

## cad-cli organizational-unit get

Get one organizational unit.

```console
cad-cli organizational-unit get [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Show one organizational unit:

```console
cad-cli organizational-unit get 00000000-0000-0000-0000-000000000001
```

## cad-cli organizational-unit create

Create an organizational unit from JSON --input <file|-> and return it.

```console
cad-cli organizational-unit create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create an organizational unit from a file:

```console
cad-cli organizational-unit create --input unit.json
```

## cad-cli organizational-unit update

Update an organizational unit from JSON --input <file|-> and return it.

```console
cad-cli organizational-unit update [OPTIONS] --input <INPUT> <ID>
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

Update an organizational unit from a file:

```console
cad-cli organizational-unit update 00000000-0000-0000-0000-000000000001 --input unit.json
```

## cad-cli organizational-unit delete

Delete an organizational unit after confirmation.

```console
cad-cli organizational-unit delete [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Delete an organizational unit after a confirmation prompt:

```console
cad-cli organizational-unit delete 00000000-0000-0000-0000-000000000001
```

Delete without prompting, e.g. from a script:

```console
cad-cli organizational-unit delete 00000000-0000-0000-0000-000000000001 --yes
```

## cad-cli organizational-unit count-entries

Count inventory entries assigned to one organizational unit.

```console
cad-cli organizational-unit count-entries [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the resource. |

### Examples

Count the inventory entries assigned to a unit:

```console
cad-cli organizational-unit count-entries 00000000-0000-0000-0000-000000000001
```

## cad-cli organizational-unit duplicate

Duplicate an organizational unit and optionally all descendants.

```console
cad-cli organizational-unit duplicate [OPTIONS] <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Organizational unit to duplicate. |

### Options

| Option | Description |
|---|---|
| `--include-children` | Also duplicate descendant organizational units. |

### Examples

Copy one organizational unit:

```console
cad-cli organizational-unit duplicate 00000000-0000-0000-0000-000000000001
```

Copy a unit together with all its descendants:

```console
cad-cli organizational-unit duplicate 00000000-0000-0000-0000-000000000001 --include-children
```
