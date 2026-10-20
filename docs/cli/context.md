# cad-cli context

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Show the authenticated session context.

```console
cad-cli context [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`show`](#cad-cli-context-show) | Show the authenticated machine identity and company selection |

Every command also accepts the [global options](README.md#global-options).

## cad-cli context show

Show the authenticated machine identity and company selection.

```console
cad-cli context show [OPTIONS]
```

### Examples

Show the machine identity and the company it acts on:

```console
cad-cli context show
```

Check that another company is accessible before using it:

```console
cad-cli --company-id 00000000-0000-0000-0000-000000000001 context show
```
