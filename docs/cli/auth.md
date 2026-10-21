# cad-cli auth

<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->

Log in to a server with an API key, check the login and log out.

```console
cad-cli auth [OPTIONS] <COMMAND>
```

| Command | Description |
|---|---|
| [`login`](#cad-cli-auth-login) | Log in to the selected server with an API key and store it |
| [`status`](#cad-cli-auth-status) | Show the credentials commands use for the selected server, and check them |
| [`logout`](#cad-cli-auth-logout) | Forget the key stored for the selected server |

Every command also accepts the [global options](README.md#global-options).

## cad-cli auth login

Log in to the selected server with an API key and store it.

The key is created on the server's API keys page, which this command prints and opens. The page shows the Key ID and the Secret as two values: enter them when asked. The key is checked with the server before anything is saved, and it is saved for that server only.

It is kept in the system keyring (macOS Keychain, Windows Credential Manager, or the Secret Service on Linux). When the keyring cannot be used, you are asked whether to keep it in a file only you can read instead; `--insecure-storage` answers yes in advance.

```console
cad-cli auth login [OPTIONS]
```

### Options

| Option | Description |
|---|---|
| `--no-browser` | Print the API keys page instead of opening it in a browser. |
| `--with-token` | Read `<Key ID>.<Secret>` from standard input instead of asking for the two values. Nothing is asked and no browser is opened. |
| `--insecure-storage` | Write the key to the credentials file without trying the system keyring. The file is readable only by you. |

### Examples

Open the API keys page, then enter the Key ID and the Secret it shows:

```console
cad-cli --environment production auth login
```

Log in to a local server without opening a browser:

```console
cad-cli --server-url http://localhost:5001 --allow-insecure-http auth login --no-browser
```

Log in without a terminal, from the Key ID and Secret on standard input:

```console
printf '%s.%s' "$KEY_ID" "$SECRET" | cad-cli --environment staging auth login --with-token
```

Keep the key in a private file instead of the system keyring:

```console
cad-cli --environment staging auth login --insecure-storage
```

## cad-cli auth status

Show the credentials commands use for the selected server, and check them.

`CAD_API_KEY` and `CAD_API_SECRET` take precedence over a stored login.

```console
cad-cli auth status [OPTIONS]
```

### Examples

Show which credentials commands use for this server, and check them:

```console
cad-cli --environment production auth status
```

The same, as JSON:

```console
cad-cli --environment production auth status --output json
```

## cad-cli auth logout

Forget the key stored for the selected server.

The key itself stays valid until it is revoked on the API keys page.

```console
cad-cli auth logout [OPTIONS]
```

### Examples

Forget the key stored for this server:

```console
cad-cli --environment production auth logout
```
