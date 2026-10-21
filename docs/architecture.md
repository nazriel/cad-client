# CAD Client Architecture

This document fixes ownership and file-placement conventions for the Rust workspace. Follow it when adding or evolving API domains, CLI commands, workflows, or offline schemas.

## Workspace boundaries

```text
cad-api       typed asynchronous platform SDK
cad-client    CLI application, command workflows, and executable
openapi/      development-time API contract only
scripts/      repository validation and developer automation
```

### `cad-api`: transport-facing SDK

`cad-api` owns everything needed to describe and call the CAD platform:

- authentication, endpoint configuration, transport, and protocol primitives;
- typed resource IDs in `src/ids.rs`;
- scoped client state in `src/client.rs`;
- API-domain models, request recipes, and public handles in `src/domains/`.

The SDK is the only layer that builds platform HTTP requests. Its public handles
must turn protocol details into domain semantics. For example, a singleton
endpoint that serializes as a one-element array remains an implementation
detail; the public SDK method returns one resource or a protocol error.

### `cad-client`: application-facing CLI

`cad-client` owns the executable experience:

- argument parsing and bootstrap in `src/cli/` and `src/app/`;
- dynamic shell completion in `src/completion/`: the `COMPLETE=<shell>`
  protocol, value completers that list IDs through the SDK, and their on-disk
  candidate cache;
- command groups in `src/cmds/`;
- input parsing, output rendering, errors, and terminal interaction;
- multi-request workflows and adapters where a command needs them.

A command invokes typed SDK handles rather than using `reqwest`, route strings,
or wire-level request structs. A command returns `serde_json::Value`; output
formatting is centralized. Stdout contains only the requested artifact or data,
while errors, logs, prompts, status, and progress use stderr.

### Self-update: the one non-platform HTTP client

`cad-cli update` and the background release notice talk to GitHub, which is
not the CAD platform, so they do not belong in `cad-api`. All of it lives in
`crates/cad-client/src/exec/update/`, the only place `cad-client` uses
`reqwest` directly:

```text
crates/cad-client/src/exec/update/
├── mod.rs        # release lookup, size-capped downloads, URL policy
├── target.rs     # OS/architecture -> published target and archive names
├── archive.rs    # SHA256SUMS verification and executable extraction
├── install.rs    # `--version` probe of the new binary, executable replacement
├── notes.rs      # release notes shown on stderr after an update
├── notice.rs     # background check, daily throttle, state file, disable rules
└── tests.rs      # contract tests and release fixtures shared with the command
crates/cad-client/src/cmds/update/
├── mod.rs        # Clap arguments and the command workflow
└── tests.rs
```

The updater follows only HTTPS URLs; plain HTTP is accepted only for a
loopback `CAD_CLI_UPDATE_URL`, the seam tests use to serve fake releases. Asset
names and checksum rules must stay in step with `install.sh`, `install.ps1` and
`mise run dist`, and the notes parser with `mise run release:notes`. `update`
is a standalone invocation in `cmds/mod.rs`: it resolves no platform
configuration, yet renders its result through the selected output format like
an online command. Its release notes are reading matter, not data: they go to
stderr through `Terminal::notes`, only with table output and only at a
terminal.

### Where credentials live

`cad-cli use profile <name>` creates a profile and stores an API key for it,
and every other command finds both again; the whole of that lives in
`cad-client`:

```text
crates/cad-client/src/support/credentials/
├── mod.rs              # the stored entry, the store, keyed by profile
├── resolve.rs          # environment > keyring > file
├── system_keyring.rs   # the OS keyring, each call on its own thread
├── file.rs             # credentials.json: 0600 file in a 0700 directory
├── memory.rs           # in-memory keyring double (tests only)
└── tests.rs
crates/cad-client/src/support/profiles/       # profiles.json: the profiles, the active one
crates/cad-client/src/support/config_dir.rs   # the per-user configuration directory
crates/cad-client/src/cmds/use/               # list, profile (create, switch, login), forget, company, check
```

A profile is a name, a server origin, the facts about its login and its
company, and `support/profiles/` keeps them in `profiles.json` beside
`credentials.json`: typed serde, camelCase, written like the credentials file.
It holds no secret. The secret is stored per profile, in the keyring under
`service = "cad-cli"`, `account = <profile name>`, or in `credentials.json`
keyed by the same name. Each stored entry also records the origin it was
created for, and `resolve.rs` refuses an entry whose origin is not the one
selected, so a login is never sent to another server, whatever the profile is
called. `config.rs` owns the selection: which profile, server, credentials and
company a command uses (see the README's "Configure" for the order).

`cad-api` does not know where credentials are kept. It owns what a credential
is (`ApiKeyV2Credentials`), how its two halves are written and checked
(`parse_key_id`, `parse_secret`, `parse_token`) and how it authenticates; where
the Key ID and Secret come from, and how they are kept between runs, is a
property of the application. A library consumer therefore decides that for
itself, and the SDK stays free of keyring, file and prompt dependencies.

The stored value is JSON tagged by `kind` (`apiKey` today), so another kind of
credential can be added without a new layout; an entry of a kind this version
does not know is an error naming the kind and the profile, never ignored. The
secret is a `SecretString` from the moment it is read until the bearer token is
built. `use` is its own invocation in `cmds/mod.rs`: `use profile` must not
resolve stored credentials before it has any, and listing and `--forget` never
connect. Keyring calls block, so each runs on a short-lived thread of its own
and never on a thread of the application's runtime. The keyring is `keyring-core` with one store per
platform (Keychain, Credential Manager, and on other Unix systems the Secret
Service through libdbus, vendored so that static musl builds need no system
library); the workspace `Cargo.toml` says why that is not the pure-Rust zbus
store.

## Platform routes and OpenAPI

`openapi/openapi_current.json` is the current development contract. Inspect it
to learn operations, schemas, and logical paths, but never bundle it or read it
at runtime.

Every platform route used by `cad-api` is normalized to `/api/...`, including a
logical OpenAPI route that omits the prefix:

```text
/benchmarks/v1/benchmarks      -> /api/benchmarks/v1/benchmarks
/api/accounts/v1/companies     -> /api/accounts/v1/companies
```

Define the normalized path once in its domain module. Do not make root-level
exceptions. Presigned object-storage transfer URLs are outside this rule.

## Domain module layout

Domains are plural because they represent API capabilities. New or materially
changed domains use a directory module, even when the initial implementation is
small. Existing one-file domains may remain as legacy layout until they next need
substantial work.

```text
crates/cad-api/src/domains/activity_groups/
├── mod.rs                 # public API, handle, route constants, private request recipes
└── tests.rs               # request, JSON-casing, and semantic tests

crates/cad-api/src/domains/reporting_periods/
├── mod.rs                 # public API, handle, route constants, private request recipes
├── reporting_period.rs    # reporting-period models, inputs, and validation
├── duplication.rs         # asynchronous duplication capability models
└── tests.rs               # request, JSON-casing, and semantic tests
```

`mod.rs` remains the public entry point and owns the scoped handle, route
constants, and private request recipes. Every directory domain keeps focused
SDK contract tests in a sibling `tests.rs`, declared from `mod.rs` with
`#[cfg(test)] mod tests;`. Extract a helper only for a meaningful,
domain-named boundary—such as `reporting_period.rs` for one resource's models
and validation or `duplication.rs` for an asynchronous capability. Avoid
generic `models.rs`, `input.rs`, `api.rs`, or `requests.rs` splits; private
request recipes normally stay next to the handle that calls them.

Register every domain in `crates/cad-api/src/domains/mod.rs`. Add a distinct
identifier newtype to `src/ids.rs` when it represents a distinct CAD resource.

## CLI command layout

CLI command names are singular resource-oriented groups, even when their SDK
domain is plural:

```text
crates/cad-client/src/cmds/
├── activity_group/
│   ├── mod.rs             # command group and domain-owned schema builder
│   └── tests.rs           # schema and command-local tests
├── reporting_period/
│   ├── mod.rs
│   └── ... workflow-specific modules
└── schema.rs
```

`use` is the one exception. It is a verb, not a resource: it shows and
switches the profile commands run with, logs in, and chooses the company, and
it is the only command that changes that selection. It is named so that it
cannot collide with a platform domain, which are all nouns. Its group lives in
`cmds/use/` like any other.

New or materially changed command groups use `<resource>/mod.rs` with a
sibling `tests.rs`, declared from `mod.rs` with `#[cfg(test)] mod tests;`.
Existing one-file command groups may remain until they next need substantial
work. Add descriptive helper modules only for command-specific workflows,
adapters, or other meaningful boundaries; do not create generic helper files.

Each command group owns:

1. its Clap `Group` and `Command` types;
2. parsing of its input argument types;
3. calls into the SDK and conversion through `output::to_value`;
4. any schema document for the resource's CLI contract.

`cmds/mod.rs` owns only cross-domain registration and dispatch:

- module declaration;
- top-level Clap command variant and concise description;
- classification as online or offline;
- dispatch with the correct client scope.

Use an ordinary company-scoped `Client` for straightforward operations. Use an
adapter only where it creates a real test seam or coordinates a multi-step,
asynchronous, or transfer workflow.

## Offline CLI schemas

The schema command is the credential-free machine interface for callers that
need to discover JSON contracts before invoking a domain:

```console
cad-cli schema list
cad-cli schema <domain>
```

The resource domain owns its schema builder. The generic `cmds/schema.rs` module
only lists schema-capable resources, routes the selected command, and writes
final JSON to stdout. It must not duplicate a resource's fields, definitions,
or examples.

Generate field-level schema from the exact Rust types used by the CLI:

```text
development OpenAPI
  -> typed `serde` API models
  -> `JsonSchema` on CLI input/output types
  -> domain-owned `schema()` builder
  -> `cad-cli schema <domain>`
```

Use `schemars` output for names, nullability, required fields, constraints, and
enums. Add hand-authored metadata only for command behavior that types cannot
express: cardinality, mutation, input transport, server-managed fields, or API
to CLI adaptations. This prevents delivered schema output from drifting from
serialized command input and output.

Schemas are offline commands: they must run before configuration or credential
resolution, bypass presentation-format selection, emit JSON only, and never
require a server. They must not depend on the development OpenAPI file or a
copied static artifact.

## Testing placement

Use the narrowest test layer that proves the contract:

| Concern                                               | Location                                  |
| ----------------------------------------------------- | ----------------------------------------- |
| SDK request method, normalized path, body, model JSON | `cad-api` domain module or its `tests.rs` |
| Command parsing and dispatch with a mock server       | `crates/cad-client/src/app/tests.rs`      |
| Shipped binary behavior and stdout/stderr boundary    | `crates/cad-client/tests/`                |
| Domain schema structure and CLI adaptations           | owning command directory's `tests.rs`     |

New domains require request-recipe/model tests and a CLI integration test. Test
singleton adaptation explicitly when the service returns an array. A schema
command needs a black-box test demonstrating it works without credentials or a
reachable server.

## Growth rules

When adding a domain:

1. inspect the OpenAPI contract and normalize its platform route to `/api`;
2. add/extend the SDK domain first, including types, IDs, request recipes, and
   semantic adaptation;
3. add the matching CLI group and register it centrally;
4. add an offline, domain-owned schema when the domain accepts or returns JSON
   useful to automation;
5. update user documentation and tests;
6. run focused tests, then `mise run docs:cli` and `mise run check`.

The authoritative detailed checklist for API-domain work is
[`.agents/skills/cad-cli-domain/SKILL.md`](../.agents/skills/cad-cli-domain/SKILL.md).
This document supplies architectural constraints; the skill supplies the
implementation workflow.
