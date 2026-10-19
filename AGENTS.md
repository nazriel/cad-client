# CAD Client Agent Guide

Read [`docs/architecture.md`](docs/architecture.md) before changing application structure or adding an API domain. It is the repository's source of truth for ownership and file placement. For a new or changed CLI domain, also follow [`.agents/skills/cad-cli-domain/SKILL.md`](.agents/skills/cad-cli-domain/SKILL.md).

## Non-negotiable boundaries

- `cad-api` owns HTTP routes, request/response wire types, typed IDs, authentication scopes, and service quirks. `cad-client` must not construct platform requests directly.
- `cad-client` owns Clap parsing, local input/output, user interaction, workflow orchestration, and binary-specific schema discovery.
- Platform request paths must begin with `/api/`, even when the development OpenAPI path omits that deployment prefix. Presigned object-storage URLs are the exception.
- Keep stdout machine-readable. Commands return values; terminal rendering, status, prompts, progress, logging, and errors use the existing application/terminal layers. Logs belong on stderr.
- The checked-in `openapi/openapi_current.json` is a development authoring input only. Do not read or embed it from delivered runtime code.

## File placement

- Add a small, self-contained SDK domain at `crates/cad-api/src/domains/<plural>.rs`; use `domains/<plural>/mod.rs` once it needs helper modules or focused tests. Register it in `domains/mod.rs`.
- Keep a domain's private request recipes beside its handle. Keep the scoped `Client` accessor in the domain entry module; extract typed public models only into a descriptive, domain-named helper module when it represents a real boundary (for example `reporting_period.rs` or `duplication.rs`), never generic `models.rs`, `input.rs`, `api.rs`, or `requests.rs`. Put resource IDs in `cad-api/src/ids.rs`.
- Add a simple CLI group at `crates/cad-client/src/cmds/<singular>.rs`; promote it to `cmds/<singular>/mod.rs` only when it has command-specific helpers, workflow modules, or focused tests. Register every group and its online/offline dispatch in `cmds/mod.rs`.
- Keep an offline schema builder with the domain command module (for example `cmds/benchmark.rs`). `cmds/schema.rs` is only the registry and renderer; it must not own another domain's fields or types.
- Keep unit/request tests next to the module they exercise. Put application dispatch fixtures in `src/app/tests.rs` and compiled-binary behavior in `crates/cad-client/tests/`.

## Change discipline

- Start from the development OpenAPI contract, but expose clean SDK/CLI semantics rather than transport quirks.
- Use typed `serde` models and `schemars::JsonSchema` for externally supplied or returned JSON. Generate `cad-cli schema <domain>` from the exact CLI input/output types and add metadata only for behavior types cannot express.
- Do not add conventional CRUD commands that the API does not support. For singleton resources, expose `read`, `create`, and/or `update` as appropriate and hide collection wrappers and server-owned IDs.
- Validate focused tests first, then run `./scripts/check.sh` for cross-cutting changes.
