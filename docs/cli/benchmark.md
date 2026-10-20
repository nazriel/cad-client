# cad-cli benchmark

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Read and update the company benchmark.

```console
cad-cli benchmark [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`read`](#cad-cli-benchmark-read) | Read and return the company's benchmark |
| [`create`](#cad-cli-benchmark-create) | Create or initialize the company benchmark from JSON --input <file\|-> and return it |
| [`update`](#cad-cli-benchmark-update) | Update the company benchmark from JSON --input <file\|-> and return it |

Every command also accepts the [global options](README.md#global-options).

## cad-cli benchmark read

Read and return the company's benchmark.

```console
cad-cli benchmark read [OPTIONS]
```

### Examples

Show the company benchmark as a table:

```console
cad-cli benchmark read
```

Print the benchmark as one JSON object:

```console
cad-cli --output json benchmark read
```

## cad-cli benchmark create

Create or initialize the company benchmark from JSON --input <file|-> and return it.

```console
cad-cli benchmark create [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Create the benchmark from an ID-free JSON object:

```console
cad-cli benchmark create --input benchmark.json
```

## cad-cli benchmark update

Update the company benchmark from JSON --input <file|-> and return it.

```console
cad-cli benchmark update [OPTIONS] --input <INPUT>
```

### Options

| Option | Description |
|---|---|
| `--input <INPUT>` | Required. Camel-case JSON input file, or `-` for stdin. |

### Examples

Update the benchmark from a file:

```console
cad-cli benchmark update --input benchmark.json
```

Read the benchmark from stdin and print the result as JSON:

```console
cat benchmark.json | cad-cli --output json benchmark update --input -
```
