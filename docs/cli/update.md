# cad-cli update

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Update cad-cli to the latest release.

```console
cad-cli update [OPTIONS]
```

## Options

| Option | Description |
|---|---|
| `--check` | Only report whether a newer release exists: exit 10 if one does, 0 if this binary is up to date. |
| `--force` | Reinstall the latest release even when it is not newer than this binary. |

## Examples

Install the latest release over the running binary:

```console
cad-cli update
```

Report whether a newer release exists; exits 10 if one does:

```console
cad-cli update --check
```

Reinstall the latest release even if it is not newer:

```console
cad-cli update --force --output json
```

This command also accepts the [global options](README.md#global-options).
