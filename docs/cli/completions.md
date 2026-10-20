# cad-cli completions

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Print a shell completion script.

```console
cad-cli completions [OPTIONS] <SHELL>
```

## Arguments

| Argument | Description |
|---|---|
| `<SHELL>` | Shell to generate a completion script for. Values: `bash`, `elvish`, `fish`, `powershell`, `zsh`. |

## Examples

Install zsh completions:

```console
cad-cli completions zsh > "${fpath[1]}/_cad-cli"
```

Install bash completions:

```console
cad-cli completions bash > /usr/local/etc/bash_completion.d/cad-cli
```

Install fish completions:

```console
cad-cli completions fish > ~/.config/fish/completions/cad-cli.fish
```

This command also accepts the [global options](README.md#global-options).
