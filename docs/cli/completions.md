# cad-cli completions

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Print a shell completion script that also completes IDs from the platform.

The script is the registration for dynamic completion and is identical to the output of `COMPLETE=<shell> cad-cli`: on Tab, the shell asks `cad-cli` for candidates. Commands, options and values come from the command definition; `--company-id` and reporting-period IDs come from the platform, using the same `CAD_*` configuration as any other command. Platform answers are cached for an hour and are never fetched for longer than two seconds; without credentials or a reachable server those IDs are simply not offered. Set `CAD_CLI_COMPLETE_OFFLINE=1` to use only the cache. Loading the script at shell startup keeps it in step with upgrades.

```console
cad-cli completions [OPTIONS] <SHELL>
```

## Arguments

| Argument | Description |
|---|---|
| `<SHELL>` | Shell to print the completion registration for. Values: `bash`, `elvish`, `fish`, `powershell`, `zsh`. |

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

Print the zsh script, e.g. to load it with `source <(cad-cli completions zsh)` in ~/.zshrc:

```console
cad-cli completions zsh
```

This command also accepts the [global options](README.md#global-options).
