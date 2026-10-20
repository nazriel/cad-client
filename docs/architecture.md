# Architecture

## Dependency direction

```text
cad-cli binary -> cad-client library -> cad-api library -> CAD HTTP API
```

The binary only starts Tokio and delegates to `cad_client::run(args)`. Parsing,
composition, workflows, output and error-to-exit-code mapping live in the
application library.

## Application slices

The application crate follows the command tree instead of centralizing every
Clap type and handler in one dispatcher:

```text
cli/                 root parser and genuinely shared arguments
cmds/<domain>/       domain group, leaf options and typed dispatch
exec/<domain>/       capability port and multi-step workflow, when needed
adapters/sdk/        one production port implementation per API domain
app/                 configuration, authentication and typestate composition
support/             process terminal, confirmation and cancellation
```

`app` resolves configuration, checks local preconditions, connects with the
machine credentials and renders the result. Preconditions are checked by walking
Clap's `ArgMatches` for every argument whose value is a `LocalInputPath`, so a
mistyped `--input` or `--file` fails before the identity request. The selection is by type rather
than by argument name: a new command inherits the check by declaring that type,
and cannot silently miss it by naming its argument something else.

Identifiers are parsed straight into their domain newtypes by Clap, so handlers
receive `ReportId` or `DocumentId` rather than a bare `Uuid`. Passing an
identifier from the wrong domain is a compile error instead of a request the
platform rejects.

Dispatch splits exactly once, into `Invocation::{Offline, Online}`. That seam
earns its keep: the generated completions and manual page are dispatched before
configuration is resolved, so they work on a machine with no credentials
configured. Everything below it is one flat `OnlineCommand`.

Session-level and company-scoped commands are therefore not separate types.
What tells them apart is which resource the arm asks `Platform` for: the
connected client, or `Platform::scoped()`, which binds the company and returns
the `SdkAdapter`. Nothing is lost by asking late — a group that takes an
`SdkAdapter` still cannot run without scoping, because scoping is the only way
to obtain one, and binding another company is a real network call whose
failure belongs to the command that needed it rather than to the bootstrap. Session-level
commands scope no more and no less than before. The alternative — a third and
fourth enum repeating the same variants, or a `needs_scope()` predicate —
restates the same axis in a second place that can fall out of sync.

A domain group owns its nested match and a leaf owns non-trivial argument
conversion. Simple SDK CRUD remains in the domain group; polling, transfer and
presentation rules live in the corresponding `exec` slice. This avoids parallel
three-line `cmds` and `exec` wrappers while keeping changes to reports,
documents or inventory local.

Ports are capability-sized, defined beside their consuming workflow, and exist
only where a workflow is worth testing against a fake: report generation with
`--wait`, reporting-period duplication, and document transfers. Those three have
a wait loop, a `select!` against Ctrl-C, or a temporary file to clean up, and
each is covered by a hand-written fake with no HTTP and no typestate client in
sight.

Plain CRUD has no port. A trait method and an adapter line that only forward one
SDK call add a layer without adding a seam — the "parallel three-line wrappers"
this document warns about elsewhere. Those command groups take
`Client<CompanyScoped>` and call it directly; what is worth testing in them is
local shaping — ordering, filtering, snapshot exclusion — and that already lives
in pure functions in `exec/<domain>/mod.rs`, testable with no seam at all.

`SdkAdapter` is both the production implementation of the three ports and, for
everyone else, the company-scoped client itself.

Waiting on a remote job is split the same way. `exec::polling::poll_until` owns
the mechanics — total budget, poll interval, the race against Ctrl-C — and takes
the terminal condition as a callback; `exec/<domain>/polling.rs` owns what
"finished" means there. The split is deliberate and stops at that line: a
report counts as finished only once its status and its file counters both say
so, while a duplication is finished on its status alone. One job abstraction
would have to choose between those rules, and the stricter report rule is the
one that would quietly disappear.

Terminal access is injected through `CommandContext`; command handlers do not
read process stdin, write progress, or prompt directly. Final result rendering
remains a single application boundary so stdout contains data only.

## Presentation

The `Terminal` trait is the only point at which the process touches its own I/O,
and everything the user sees goes through it: semantic statuses
(`StatusKind::{Success, Info, Warning, Error}`), the confirmation prompt, stdin,
the rendered result, and generated artefacts that are already in final form.
Presentation crates — `comfy-table`, `console`, `indicatif` — are dependencies of
`cad-client` only; `cad-api` stays on its minimal runtime diet and emits no
output at all.

Decoration is decided once, from the state of the stream being written to.
`--output json` and `--output jsonl` are a machine contract and are never
decorated. The `table` format is drawn with box frames on a terminal and as
plain ASCII everywhere else, and colour is additionally gated on `console`'s
`NO_COLOR`/`CLICOLOR` handling, so a coloured terminal and a redirected file are
two different decisions rather than one.

Long operations — waiting on a report or a duplication job, and both document
transfers — run under a progress indicator whose whole interface is its
lifetime. Dropping it clears the line, so a Ctrl-C that unwinds out of a
`tokio::select!` cannot leave a spinner behind, and the temporary-file cleanup
on that same path is unaffected. Where stderr cannot be redrawn the indicator
degrades to the single line the plain status call printed before.

## SDK state machine

The API client encodes protocol prerequisites in its type:

```text
Client<Disconnected> --connect--> Client<Connected>
Client<Connected> --scope_current_company/for_company--> Client<CompanyScoped>
```

Only `Connected` and `CompanyScoped` can list companies, read the session
context or bind another company. Only `CompanyScoped` exposes business
resources.

Each state owns the data that state actually has: `Disconnected` holds the
transport, `Connected` holds an authenticated core, `CompanyScoped` adds the
bound company. `Client<State>` stores that state directly, so an impossible
combination is unrepresentable rather than rejected at runtime. Both the state
fields and `Client`'s own field are private, so downstream code cannot forge a
state, and a private `ClientState` trait keeps the set of states closed.

Operations shared by the authenticated states — `companies`, `context`,
`for_company`, `disconnect`, `server_config` — are written once, in
`impl<State: Authenticated> Client<State>`, instead of once per state. A state
joins that set by supplying its core through the private `Authenticated` trait,
so `Disconnected` cannot reach the operations and no downstream type can add
itself. The methods stay inherent, so callers name nothing new. Rust's
`private_bounds` lint flags the unnameable bound; that unnameability is the
sealing mechanism, so the impl carries a documented `allow`.

Typestate models only what the client establishes itself — a validated key and
a bound company — and leaves every authorization decision to the platform.

## No blocking client, for now

The SDK is async-only. An earlier revision shipped a `blocking` feature with a
mirrored `blocking::Client<State>` on top of `reqwest::blocking`: its own
client, core, session and transport, plus one hand-written wrapper per domain
operation. Sharing stopped at the DTOs and request recipes, so every new
endpoint had to be written twice and nothing but review kept the two surfaces
identical.

It was removed rather than kept in that shape. A blocking client returns behind
a `blocking` feature gate once one definition can produce both surfaces —
generated facades, or an executor abstraction parameterized over `RequestSpec` —
so drift becomes impossible instead of merely discouraged. Until then, callers
that need a synchronous entry point drive the async client from their own
runtime.

Whatever the mechanism, the constraints that shaped the removed implementation
still hold: no `block_on` inside the SDK, no Tokio runtime created on a
consumer's behalf, and no public generic runtime mode leaking into type
signatures.

## SDK module ownership

Each endpoint contract has one private, runtime-independent `RequestSpec` that
owns its HTTP method, path, query, body and response type. Domain modules build
these recipes; the execution core only runs them against its `reqwest` client.
The recipes stay runtime-independent so a future blocking executor can reuse
them unchanged.

Complex domains use private vertical modules behind stable public facades:

```text
domains/inventory_lines/   models, validated input, query language, recipes, APIs
domains/reporting_periods/ models, input, duplication, recipes, APIs
domains/documents/         models, control plane, recipes, native transfers
```

Smaller domains remain single files. Internal module topology is not part of
the SDK surface; public types are re-exported from the facade.

Pure protocol code — the identity and companies recipes, `(status, bytes)`
decoding — lives in `protocol/` and knows nothing about a runtime. Locking,
cancellation and file streaming are runtime-specific and stay in `transport.rs`
and the document transfer module.

## Session semantics

Authentication uses a machine API key. Every platform request carries
`Authorization: Bearer <api key>.<secret>`, so there is no token to exchange,
refresh or store. `connect` validates the credentials with one request to the
machine identity endpoint and keeps what it returns: the machine principal and
the company selected for the key.

A company is bound locally or with one more identity request.
`scope_current_company` adopts the company from the connection context without
a network call, and fails when there is none. `for_company` repeats the
identity request with `X-CAD-Company-Id` and accepts the answer only when it
selects that same company; the resulting client sends the header on every
request. Clients for different companies share the transport and the
credentials but no mutable state. A rejected request is reported, never
retried.

## HTTP and storage boundary

Platform calls use a same-origin API transport with redirects disabled. Error
bodies support both `{msg}` and `{detail}` envelopes. The SDK prints
nothing; it reports request diagnostics as `tracing` debug events, which reach
an output only when the application installs a subscriber, and which carry the
URL without its query string. Secrets, bearer tokens and presigned URLs are
redacted by the `Debug` implementations of the types that hold them.

Timeouts distinguish the two kinds of traffic. Platform calls carry a total
per-request budget from `ServerConfig::with_request_timeout`, defaulting to one
minute and surfaced as `--request-timeout`. Object-storage transfers are
unbounded in total duration by design and are instead bounded by a stall
timeout, so a large but progressing download is never cut short while a silent
peer still fails.

Presigned transfers use a second HTTP client without CAD authentication or CAD
default headers. This prevents bearer leakage to object storage. Its redirect
policy allows same-host hops only: a presigned URL addresses one object on one
host, and a cross-host redirect would hand its query string to an unrelated
origin. Such a hop fails loudly, because a merely stopped 3xx is not an error
status and would read as a successful upload.

The application writes downloads to a temporary sibling file and renames it into
place only after success; the SDK itself streams to whatever path it is given.
Transfers are interruptible, so Ctrl-C unwinds and removes that temporary file
instead of leaving it beside the destination.

Inside the SDK, document control-plane calls are separate from object-storage
transfer orchestration. Private storage capabilities make ordering and header
isolation testable without exposing `reqwest` or allowing consumers to replace
the platform transport.

## Error sources

`cad_api::Error` keeps the cause it was built from. `Transport` carries the
`reqwest` error with `without_url()` applied: the wrapper message stays
deliberately coarse, but flattening the cause entirely had made DNS, TLS and
proxy failures indistinguishable, and stripping the URL is what removes the
presigned query string that motivated the flattening in the first place.

`cad-client` is the application crate of `cad-cli`, not a UI-agnostic workflow
library, and its workflows and ports return `CliError` on purpose. The
alternative — an `exec` error type that `CliError` maps at the boundary — is a
conversion layer with exactly one consumer today. The trigger for adding it is
concrete rather than aesthetic: a second front end. Should one appear, `exec`
gets its own error and `CliError` becomes the CLI's mapping of it, which is a
contained change precisely because the ports are few.

`CliError` stays a typed enum rather than an opaque error, because it is the
carrier of the exit-code contract. Variants that can originate in the SDK keep
the `cad_api::Error` they were converted from, so `details` from a rejected API
call and the whole chain below it remain reachable through `CliError::api_error`
instead of being discarded at the crate boundary. What the user sees on stderr is
unchanged and still plain text; the structure is available for a future
`--verbose` without another conversion.

## Test placement

Tests that need infrastructure — a mock HTTP server, a full application run,
assertions on protocol bytes — live in the crate's `tests/` directory. Beside
the code stay simple unit tests and tests that genuinely must see private items.

In-crate tests live in a sibling `tests.rs`, not in the implementation file. A
`#[cfg(test)]` block that grows past a couple of small cases starts to
outweigh the code it covers — `cmds/document/download.rs` was two thirds test
by line count before the split — and the module it belongs to is the natural
place for it: `exec/tests.rs`, `exec/<domain>/tests.rs`,
`cmds/document/tests.rs`, `app/tests.rs`. A handful of genuinely small cases,
like the `RequestSpec` recipe assertions, stay where they are.

That distinction is a consequence, not a preference: an integration test sees
only the crate's public API, and `cad-client` exports exactly `run` and
`CliError`. A workflow test written in `tests/` would therefore not be the same
test moved — it would be a black-box run of the binary with an HTTP fixture,
which is what `tests/cli.rs` already does for the contract that deserves it: exit
codes and the stdout/stderr split. So the `RequestSpec` contract tests, the parser and
output tests, and the ordering test that proves a transfer opens its local source
before creating a remote record — which works by making private fakes panic —
have to stay inline. `cad-client`'s `app/tests.rs` likewise drives the private
`execute()` seam with substituted process I/O.

```text
cad-api/tests/session.rs     machine-key identity, company scoping
cad-api/tests/documents.rs   presigned transfer, through the public facade
cad-api/tests/support/       fixtures the integration tests share
cad-client/tests/cli.rs      the shipped binary: exit codes, stdout/stderr split
```

`wiremock` replaces the hand-rolled `TcpListener` fixtures that were duplicated
across three modules. Assertions on the wire are made against its recorded
requests, which keeps them exact: JSON field casing, the machine-key bearer on
every platform request and its absence on the storage PUT, the company header
on a scoped client, and the precise path and query of a presigned URL.

## Contract policy

The published OpenAPI document is the contract the client is written against.

OpenAPI code generation is deferred: models and request recipes are written by
hand, and each implemented route is tracked in `api-coverage.md` until a tested
generation approach can replace them.
