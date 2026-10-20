# cad-cli company

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Inspect and create companies available to the current API key.

```console
cad-cli company [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`list`](#cad-cli-company-list) | List companies accessible to the authenticated user |
| [`create`](#cad-cli-company-create) | Create a company from JSON --input <file\|-> after confirmation; this may affect the next invoice |

Every command also accepts the [global options](README.md#global-options).

## cad-cli company list

List companies accessible to the authenticated user.

```console
cad-cli company list [OPTIONS]
```

### Examples

List the companies the API key can access:

```console
cad-cli company list
```

Print one company per line for scripts:

```console
cad-cli company list --output jsonl
```

## cad-cli company create

Create a company from JSON --input <file|-> after confirmation; this may affect the next invoice.

```console
cad-cli company create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create a company after a confirmation prompt:

```console
cad-cli company create --input company.json
```

Create a company without prompting, e.g. from a script:

```console
cad-cli company create --input company.json --yes
```
