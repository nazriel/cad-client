---
name: cad-cli-domain
description: Add or evolve a CAD CLI domain from the OpenAPI contract, including typed SDK methods, CLI command wiring, tests, and LLM-friendly command documentation. Use when introducing a new resource or service command group in this repository.
---

# Add a CAD CLI Domain

Use this skill when adding commands for a new Carbon+Alt+Delete API domain to this repository. Follow existing repository patterns rather than introducing a generic framework.

## Goals

A complete domain addition must:

1. expose only API operations that the current OpenAPI contract supports;
2. provide typed SDK models and a company-scoped client handle in `cad-api`;
3. expose a discoverable CLI command group in `cad-client`;
4. preserve machine-readable stdout and predictable exit behavior;
5. hide service quirks (for example, singleton resources returned as one-item arrays) behind the SDK/CLI boundary;
6. include request-contract and CLI integration tests;
7. document examples so an LLM operating the CLI can discover, inspect, and mutate resources safely;
8. register an offline `cad-cli schema <domain>` document that describes the CLI contract.

## Repository Layout

| Responsibility                    | Location                                                           |
| --------------------------------- | ------------------------------------------------------------------ |
| Latest endpoint contract          | `openapi/openapi_current.json`                                     |
| Typed API domain                  | `crates/cad-api/src/domains/<domain>.rs` or `domains/<domain>/`    |
| Domain registration               | `crates/cad-api/src/domains/mod.rs`                                |
| Resource IDs                      | `crates/cad-api/src/ids.rs`                                        |
| Top-level command group           | `crates/cad-client/src/cmds/<domain>.rs` or `cmds/<domain>/mod.rs` |
| Command registration and dispatch | `crates/cad-client/src/cmds/mod.rs`                                |
| CLI integration tests             | `crates/cad-client/src/app/tests.rs`                               |
| Domain request/model tests        | beside the API domain module                                       |
| Offline CLI schema registry       | `crates/cad-client/src/cmds/schema.rs`                             |

Useful reference implementations:

- Straightforward CRUD: `custom_field`, `custom_option`, and `inventory_line`.
- Nested or asynchronous work: `reporting_period`, `report`, and `document`.
- Singleton service behavior: `benchmark`.

## Workflow

### 1. Start from the current OpenAPI contract

Do not infer routes or request shapes from endpoint names. Inspect the relevant paths and component schemas first. The checked-in OpenAPI file is a **development-time authoring source only**; it is not distributed with the CLI and must never be included by a runtime `include_str!` or read by a delivered command:

```sh
jq '.paths | with_entries(select(.key | test("<domain>"; "i")))' openapi/openapi_current.json
jq '.components.schemas.<SchemaName>' openapi/openapi_current.json
```

Record for every operation:

- exact method and logical service path;
- request schema and required fields;
- success response schema, especially object versus array;
- ID placement (path, body, or both);
- whether the endpoint is company scoped;
- validation and permission failure responses.

### Normalize platform paths to `/api`

The OpenAPI document records logical service paths and may omit the deployment
prefix. Every CAD **platform** URL constructed by this client must start with
`/api/`, including a path that appears in `openapi_current.json` without that
prefix. Normalize each OpenAPI path exactly once:

```text
/api/ + openapi_path with any leading /api/ and / removed
```

Examples:

- OpenAPI `/benchmarks/v1/benchmarks` → client `/api/benchmarks/v1/benchmarks`
- OpenAPI `/api/accounts/v1/companies` → client `/api/accounts/v1/companies`

Do not create root-level service exceptions. Check `ServerConfig::endpoint` in
`crates/cad-api/src/config.rs`; it deliberately accepts only `/api/` paths.
This rule applies to platform API calls, not to presigned object-storage URLs.

### 2. Define the public domain API deliberately

Create models that match the wire format using `#[serde(rename_all = "camelCase")]`. Prefer the typed identifier newtypes in `ids.rs` for resource IDs and add a new one when it represents a distinct platform resource.

Use a dedicated input type when the server owns an ID. For example, expose `BenchmarkInput` without an `id`, obtain the current resource ID internally, then serialize the API-required ID in a private wire structure. This prevents LLM and human callers from accidentally supplying a mismatched path/body ID.

Expose only clean domain semantics:

- `list() -> Result<Vec<Resource>>` for a real collection;
- `get(id) -> Result<Resource>` for an ID-addressed resource;
- `read() -> Result<Resource>` for a singleton;
- `create(&Input) -> Result<Resource>`;
- `update(...)` or `replace(...) -> Result<Resource>`;
- `delete(id) -> Result<DeleteResult<Id>>` only where supported.

Keep raw endpoint peculiarities private. If the server returns a one-element array for a singleton, private `get_*` code may return `Vec<Resource>`, but public `read`, `create`, and `update` must return `Resource` and fail with a protocol error for zero or multiple results.

Avoid adding commands merely because CRUD is conventional. For example, do not add `delete`, `list`, or `get` when OpenAPI only supports a singleton `read` and `update` flow.

### 3. Implement request recipes in `cad-api`

Keep HTTP details in a `requests` module or domain request file:

- define `/api/`-prefixed path constants once, normalizing OpenAPI paths as described above;
- use `RequestSpec` with the exact `reqwest::Method`;
- serialize all path/body ID duplication through private wire types;
- have the public handle call `ClientCore::execute`;
- add `Client<CompanyScoped>::<domain>()` as the domain entry point.

Use public rustdoc for every public `Result` method, including an `# Errors` section. Do not leak API credentials, request URLs, or storage URLs through error messages or debug output.

### 4. Add CLI commands

Create a group with Clap `Args` and `Subcommand` in `crates/cad-client/src/cmds/<domain>.rs`:

```rust
#[derive(Debug, Args)]
#[command(arg_required_else_help = true)]
pub struct Group {
    #[command(subcommand)]
    pub(crate) command: Command,
}
```

Use verbs that describe the resource contract:

- singleton: `read`, `create`, `update`;
- collection: `list`, `get`, `create`, `patch`/`replace`, `delete`;
- jobs: action verb plus `status`, and `--wait` only where it is valuable.

Use the shared argument types from `crate::cli::args`:

- `InputArgs` for `--input <file|->` JSON request bodies;
- `IdArgs<Id>` for an ID positional argument;
- `IdInputArgs<Id>` when an ID and JSON input are both genuinely required.

Read JSON through `CommandContext::read_json`, serialize responses through `output::to_value`, and return `Result<Value, CliError>`. Do not print inside a command group.

For destructive commands, call `context.confirm(...)` before making the request. Creation and updates are normally not confirmation-gated unless the domain has an unusual irreversible effect.

### 5. Wire the group into application dispatch

In `crates/cad-client/src/cmds/mod.rs`:

1. declare `pub mod <domain>;`;
2. add `<Domain>(<domain>::Group)` to the top-level `Command` enum with a concise description;
3. map it in `Command::into_invocation`;
4. add it to `OnlineCommand` and dispatch it with the correct client scope.

Use `platform.scoped().await?.client()` for ordinary company-scoped CRUD. Use `SdkAdapter` only when the command needs an execution seam, such as polling, asynchronous workflows, or transfer coordination.

### 6. Turn development OpenAPI into a generated CLI schema

Follow this pipeline for every schema-enabled domain:

```text
openapi_current.json (development evidence)
  → typed `cad-api` request/response models
  → serde wire behavior used by real commands
  → `JsonSchema` derived from those same models
  → domain-owned `schema()` document
  → `cad-cli schema <domain>` offline dispatch
```

#### Map the OpenAPI contract into Rust types

Use the development OpenAPI to create distinct input and response models. Apply
both serde and Schemars derives to every type that is serialized as command
input or output:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceInput {
    pub name: String,
    pub category: Option<Category>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    #[schemars(with = "String")]
    pub id: ResourceId,
    pub name: String,
    pub category: Option<Category>,
}
```

Use a separate input type whenever the platform owns a field such as `id`; do
not generate the CLI write schema from an API wire type that contains that
field. Add `deny_unknown_fields` when the published CLI schema declares
`additionalProperties: false`.

Model OpenAPI string enums as Rust enums with `Serialize`, `Deserialize`, and
`JsonSchema`, using explicit `#[serde(rename = "WIRE_VALUE")]` values whenever
the Rust spelling differs. This makes invalid CLI input fail locally and makes
the generated enum list match the parser and serializer. For identifier
newtypes that intentionally do not implement `JsonSchema`, use
`#[schemars(with = "String")]` on the field; use a UUID-specific schema only
when the type and validator actually enforce that format.

#### Generate the domain document from command types

Add `schemars` as a workspace dependency and to both `cad-api` and `cad-client`.
Keep the domain's `schema()` function beside its command implementation, for
example `crates/cad-client/src/cmds/<domain>.rs`. Generate root schemas from
the exact types used by the command:

```rust
use schemars::schema_for;

let input = serde_json::to_value(schema_for!(ResourceInput))?;
let output = serde_json::to_value(schema_for!(Resource))?;
```

Remove each generated root's `$schema` and merge its `$defs` into the outer
CLI document's `$defs`. The nested input/output schemas can then retain their
`#/$defs/...` references, which resolve against the outer document. Add only
command semantics manually: resource name, cardinality, operation names,
mutation flags, input transport, server-managed fields, and examples.

Never use `include_str!` for `openapi_current.json`, and never copy OpenAPI
components into a static schema JSON file. The delivered command must construct
its document from compiled Rust types only.

#### Register and test the offline command

In `cmds/schema.rs`, keep only the resource registry and final JSON renderer:

```rust
Command::Example => super::example::schema(),
```

The generic command must not contain domain schema construction. Add the domain
to `schema list`, wire `schema` through `OfflineCommand`, and write the final
JSON bytes directly to stdout without credentials, server configuration, or
status output.

Test all of these boundaries:

1. derived schema input omits hidden/server-managed fields and output includes
   fields returned by the command;
2. generated `$defs` contains constrained enum values and nested references
   resolve to it;
3. the input parser rejects unknown fields and invalid typed enum values;
4. the existing command integration test serializes/returns representative
   domain values; and
5. a black-box `cad-cli schema <domain>` invocation succeeds with no credentials.

## Baseline Functionality Checklist

For each domain, decide explicitly which items apply:

- [ ] **Read/list**: one command returns the service’s natural domain object(s).
- [ ] **Create**: accepts validated JSON via `--input`; returns the created object.
- [ ] **Update**: uses `patch` for partial API updates and `replace`/`update` for complete ones; returns the updated object.
- [ ] **Delete**: requires a confirmation unless `--yes` is supplied; returns the service result.
- [ ] **Get by ID**: include only when the API has a distinct ID-addressed read endpoint.
- [ ] **Pagination/filtering**: expose API-native, deterministic options only when the endpoint supports them.
- [ ] **Async work**: support `status` and an opt-in `--wait` only for actual jobs.
- [ ] **Singleton adaptation**: hide array wrappers and service-owned IDs from callers.

## LLM-Friendly CLI Contract and Documentation

LLM clients work best when the CLI is deterministic, schema-driven, and easily introspected.

### Command interface

- Keep command names stable, lowercase, and action-oriented: `list`, `get`, `read`, `create`, `update`, `delete`, `status`.
- Use one JSON object per write input. Accept `--input -` so an agent can pipe or heredoc JSON without temporary files.
- Return the resource object on successful singular reads/writes. Do not expose transport wrappers, hidden arrays, or success prose in stdout.
- Ensure `--output json` is valid JSON and `--output jsonl` is one object per line. Status/progress messages belong on stderr.
- Use standard non-zero failures with actionable messages; distinguish local input errors from API rejections.
- Never require an agent to scrape table output, parse human prose, or invent a server-generated ID.

### Offline CLI schemas

Every externally useful domain must be discoverable through the credential-free,
JSON-only schema interface:

```sh
cad-cli schema list
cad-cli schema <domain>
```

Use `cad-cli schema <domain>` as the sole canonical schema command; do not add
duplicate `<domain> schema` subcommands. `schema list` advertises available
domains, and domain documents must contain a stable `schemaVersion`, resource
name, cardinality, supported commands, mutation flags, input transport, output
shape, required fields, enum definitions, and a complete example.

The document describes the **CLI contract**, not a raw OpenAPI component. Hide
server-managed fields such as IDs and document any API-to-CLI adaptation. Follow
the complete OpenAPI → Rust type → `JsonSchema` → schema-command pipeline in
[Generate the CLI schema](#6-turn-development-openapi-into-a-generated-cli-schema).
The delivered schema must be generated from the exact Rust input and output
types the commands serialize; do not copy OpenAPI components into a static JSON
artifact or load `openapi_current.json` from delivered code.

Register the new domain in `crates/cad-client/src/cmds/schema.rs`, add unit
coverage for its generated schema, and add a black-box test proving schema
output needs neither credentials nor a reachable server.

Schema commands are offline: wire them through `OfflineCommand`, write final
JSON bytes directly to stdout, and never use the selected presentation format
or emit status messages. Reserve `cad-cli schema <domain> <operation>` for a
future need; do not expose it until operation-specific schemas materially
differ from the resource document.

### Help text

Each command’s Clap doc comment should answer:

1. **What object does it operate on?**
2. **What does it return?**
3. **How does input arrive?** (`--input <file|->`)
4. **Does it mutate or prompt?**
5. **Which input type is expected?**

Prefer “Read the company’s benchmark” over “Get benchmark data.” For special behavior, name it directly: “Create or initialize the company’s benchmark from JSON input.”

### User-facing documentation template

Add a concise README section or domain-specific document when the domain is externally useful. Include:

````markdown
## <Domain>

Inspect the CLI contract:

```sh
cad-cli <domain> --help
cad-cli <domain> <verb> --help
```
````

Read the current resource as JSON:

```sh
cad-cli --output json <domain> read
```

Create or update it from a JSON file:

```sh
cad-cli --output json <domain> update --input request.json
```

Send JSON through stdin:

```sh
printf '%s' '{...}' | cad-cli --output json <domain> update --input -
```

Input schema: `cad-cli schema <domain>` (and a complete JSON example).

````

Document required JSON fields, nullable fields, enum spellings, units, date formats, ID semantics, and whether the response is a singleton or collection. Examples must use `--output json` for copyable LLM workflows.

## Testing Requirements

At minimum, add:

1. **API request-recipe test**: assert method, exact path, and serialized request body.
2. **Model/input test**: validate relevant local invariants or exact JSON casing.
3. **CLI integration test**: mock identity and the domain route, run real Clap parsing and dispatch, then assert JSON stdout and request sequence.

For a singleton whose server endpoint returns `[resource]`, the integration test must assert that `cad-cli --output json <domain> read` produces a JSON object, not an array.

For writes, also test that the selected resource ID is used consistently in the URL and JSON body when the service requires both.

Run, at minimum:

```sh
cargo fmt --check
cargo test --workspace
cargo run -p cad-client --bin cad-cli -- <domain> --help
````

Run Clippy if the repository baseline permits it. Report pre-existing warnings separately from warnings introduced by the change.

## Completion Checklist

- [ ] OpenAPI methods, paths, request schemas, and response shapes verified.
- [ ] API models and identifier newtypes added where appropriate.
- [ ] Endpoint quirks hidden from public SDK and CLI semantics.
- [ ] Command group, command help, and online dispatch wired.
- [ ] JSON input/output behavior verified for LLM use.
- [ ] Offline `cad-cli schema <domain>` document generated from the command's Rust input/output types, ID-safe, and tested without credentials.
- [ ] The delivered schema does not read `openapi_current.json` or a hand-maintained schema artifact.
- [ ] Contract and integration tests added.
- [ ] `fmt`, relevant tests, and command help validated.
- [ ] README/domain documentation includes JSON examples and input schema guidance.
