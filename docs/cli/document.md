# cad-cli document

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Upload and download supporting documents.

```console
cad-cli document [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`upload`](#cad-cli-document-upload) | Stream a local file to CAD object storage |
| [`download`](#cad-cli-document-download) | Download a document using a presigned storage URL |

Every command also accepts the [global options](README.md#global-options).

## cad-cli document upload

Stream a local file to CAD object storage.

```console
cad-cli document upload [OPTIONS] --file <FILE>
```

### Options

| Option | Description |
|---|---|
| `--file <FILE>` | Required. Local file to stream to object storage. |
| `--input <INPUT>` | Optional prepare-upload metadata JSON. Filename and company are injected. |

### Examples

Upload a file and print the new document ID:

```console
cad-cli document upload --file invoice.pdf
```

Upload a file with extra document metadata:

```console
cad-cli document upload --file invoice.pdf --input metadata.json
```

## cad-cli document download

Download a document using a presigned storage URL.

```console
cad-cli document download [OPTIONS] --out <OUT> <ID>
```

### Arguments

| Argument | Description |
|---|---|
| `<ID>` | Identifier of the document. |

### Options

| Option | Description |
|---|---|
| `--out <OUT>` | Required. File to write the document to. |
| `--force` | Replace the --out file if it already exists. |

### Examples

Download a document to a new file:

```console
cad-cli document download 00000000-0000-0000-0000-000000000001 --out invoice.pdf
```

Replace the file if it already exists:

```console
cad-cli document download 00000000-0000-0000-0000-000000000001 --out invoice.pdf --force
```
