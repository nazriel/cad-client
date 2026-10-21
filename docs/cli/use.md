# cad-cli use

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Show and switch the active profile, and log in.

Without a subcommand it lists the profiles, sorted by name, with the server and the login of each. `active` marks the profile `use profile` last switched to; `selected` marks the one a command run now would use, which differs when `--profile`, `CAD_PROFILE` or a server given with `--server-url` or `CAD_SERVER_URL` picks another. It makes no request.

`use profile <name>` creates, switches to, logs in to and forgets a profile.

```console
cad-cli use [OPTIONS] [COMMAND]
```

| Command | Description |
|---|---|
| [`profile`](#cad-cli-use-profile) | Create a profile, switch to it, log in to it, or forget it |

Every command also accepts the [global options](README.md#global-options).

## cad-cli use profile

Create a profile, switch to it, log in to it, or forget it.

A profile is a name for a server and the API key you log in with there. Naming a profile that does not exist yet creates it: its server is `--url`, or is asked for (production, staging and dev suggest the address of their server). Naming one that exists makes it the active profile, which commands use unless `--profile` or `CAD_PROFILE` names another.

A profile with no login is offered one. The API keys page of the server is printed and opened; it shows the Key ID and the Secret as two values: enter them when asked. The key is checked with the server before anything is saved, and it is saved for that server only.

It is kept in the system keyring (macOS Keychain, Windows Credential Manager, or the Secret Service on Linux). When the keyring cannot be used, you are asked whether to keep it in a file only you can read instead; `--insecure-storage` answers yes in advance.

`CAD_API_KEY` and `CAD_API_SECRET` take precedence over a stored login.

```console
cad-cli use profile [OPTIONS] <NAME>
```

### Arguments

| Argument | Description |
|---|---|
| `<NAME>` | Name of the profile: 1 to 32 characters of a-z, 0-9, - and _. |

### Options

| Option | Description |
|---|---|
| `--url <URL>` | Address of the profile's server, for a profile that does not exist yet. |
| `--login` | Log in, even if the profile has a login already. |
| `--with-token` | Log in with `<Key ID>.<Secret>` read from standard input instead of asking for the two values. Nothing is asked and no browser is opened. |
| `--no-browser` | Print the API keys page instead of opening it in a browser. |
| `--insecure-storage` | Write the key to the credentials file without trying the system keyring. The file is readable only by you. |
| `--forget` | Forget the profile and the key stored for it. The key itself stays valid until it is revoked on the API keys page. |

### Examples

Switch to a profile; a new one asks for its server and offers to log in:

```console
cad-cli use profile production
```

Log in again, opening the API keys page and asking for the Key ID and the Secret:

```console
cad-cli use profile staging --login
```

Log in without a terminal, from the Key ID and Secret on standard input:

```console
printf '%s.%s' "$KEY_ID" "$SECRET" | cad-cli use profile staging --with-token
```

Create a profile for a local server, logging in without opening a browser:

```console
cad-cli use profile local --url http://localhost:5001 --allow-insecure-http --no-browser
```

Keep the key in a private file instead of the system keyring:

```console
cad-cli use profile staging --login --insecure-storage
```

Forget the profile and the key stored for it:

```console
cad-cli use profile staging --forget
```
