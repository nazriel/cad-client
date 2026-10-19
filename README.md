# CAD client for Rust

Proof-of-concept Rust SDK and command-line client for the Carbon+Alt+Delete platform.

The workspace contains exactly two packages:

- `cad-api`: a typed async SDK;
- `cad-client`: application workflows and the `cad-cli` executable.

## Development

[mise](https://mise.jdx.dev/) pins every tool in `mise.toml`: Rust 1.99.1,
[mr boxington](https://mr-boxington.jdx.dev/) (`mbx`), which caches Cargo
builds across worktrees and CI, and the linters. Install them and the Git
hooks once:

```console
mise run setup
```

The hooks come from [hk](https://hk.jdx.dev/) and run the steps in `hk.pkl`:
rustfmt, Clippy, cargo-deny, actionlint, zizmor and shellcheck before each
commit, and the tests as well before each push. The same steps are tasks:

```console
mise run check        # everything CI checks: lint, doc and test
mise run lint         # hk check --all
mise run fix          # hk fix --all
mise run audit        # RustSec advisories and workflow audit
mise run dist         # release archive for the host in dist/
```

CI runs lint and the tests on Linux, macOS and Windows for every pull request,
and builds the release archives. Pushing a tag `v<version>` that matches the
workspace version publishes a GitHub release with archives for Linux (musl),
macOS and Windows on x86_64 and aarch64, plus `SHA256SUMS`.

A daily security workflow checks `Cargo.lock` against the RustSec advisory
database and audits the workflows; Dependabot opens grouped weekly update PRs.

## Install

On macOS and Linux, install the latest release binary into `~/.local/bin`:

```console
curl -fsSL https://raw.githubusercontent.com/nazriel/cad-client/main/install.sh | sh
```

Pass options after `sh -s --`, for example `--version v0.1.0` or
`--bin-dir DIR`.

On Windows, in PowerShell 5.1 or 7, this installs `cad-cli.exe` into
`~\.local\bin` and adds that directory to the user `PATH`:

```powershell
irm https://raw.githubusercontent.com/nazriel/cad-client/main/install.ps1 | iex
```

To pass `-Version`, `-BinDir` or `-NoModifyPath`, run it as a script block:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/nazriel/cad-client/main/install.ps1))) -Version v0.1.0
```

Both installers verify the archive against the release's `SHA256SUMS`. Release
archives cover macOS (Apple Silicon, Intel), Linux (x86_64, aarch64; statically
linked with musl) and Windows (x86_64, ARM64) and are attached to each
[GitHub release](https://github.com/nazriel/cad-client/releases).

From a checkout:

```console
cargo install --path crates/cad-client --bin cad-cli
```

## Configure

Credentials are read only from the environment. Every platform request uses
`Authorization: Bearer <CAD_API_KEY>.<CAD_API_SECRET>`:

```console
export CAD_SERVER_URL=https://staging.carbonaltdelete.eu
export CAD_API_KEY=00000000-0000-0000-0000-000000000000
export CAD_API_SECRET='...'
```

Instead of `CAD_SERVER_URL`, select a preset with `--environment staging` or
`--environment production`, or set `CAD_ENV` to the same value. Both accept the
short aliases `stage` and `prod`. There is deliberately no implicit server
default.

To select another accessible company, pass `--company-id` or set
`CAD_COMPANY_ID`; the CLI sends it as `X-CAD-Company-Id`.

Every API request has a one-minute total budget; change it with
`--request-timeout 90s`. Durations are written with an explicit unit and may
combine several — `1500ms`, `90s`, `1m30s`, `30m` — a bare number is rejected
rather than guessed at. The budget does not apply to document transfers, which
may run as long as they keep making progress. Plain HTTP is rejected unless the
server is a loopback host and `--allow-insecure-http` is passed.

## Examples

```console
cad-cli company list
cad-cli reporting-period list --output json
cad-cli custom-option list --field-id 00000000-0000-0000-0000-000000000000
cad-cli report create --input report.json --wait
```

Mutation bodies use the API's camelCase JSON representation and can be read
from a file or stdin (`--input -`). Human-readable tables are the default;
`--output json` and `--output jsonl` are stable machine-readable alternatives.

Stdout carries data and nothing else. Status messages, prompts, progress
spinners and errors go to stderr, so `cad-cli company list > companies.txt` is
always a clean capture. In a terminal the table is drawn with frames and the
status line is coloured; piped anywhere else the same table is plain ASCII with
no escape sequences, and `NO_COLOR=1` turns colour off everywhere.

Set `RUST_LOG=cad_api=debug` to print each platform request method, URL, and
selected company ID to stderr.
Authorization headers, request bodies, and URL query strings are never logged.
All platform API paths, including machine identity, are rooted at `/api/`.

## Benchmarks

A company has one benchmark. Although the platform endpoint returns a
one-item collection, `benchmark read`, `create`, and `update` return one JSON
object; callers never need to discover or supply the benchmark ID.

Inspect the command contract before use. `schema` is an offline,
credential-free JSON interface designed for automation and LLM clients:

```console
cad-cli schema list
cad-cli schema benchmark > benchmark-cli-schema.json
cad-cli benchmark --help
cad-cli benchmark update --help
```

Read the current benchmark in a machine-readable form:

```console
cad-cli --output json benchmark read
```

Create or update the benchmark using an ID-free JSON object:

```console
cad-cli --output json benchmark update --input benchmark.json
printf '%s' '{"reportingPeriodId":"00000000-0000-0000-0000-000000000001","fullTimeEmployees":42,"grossAnnualRevenue":null,"grossAnnualRevenueUnitId":"00000000-0000-0000-0000-000000000002","highLevelSectorCode":"C","lowLevelSectorCode":null,"location":"DENMARK"}' | \
  cad-cli --output json benchmark update --input -
```

`benchmark.json` must contain `reportingPeriodId` and
`grossAnnualRevenueUnitId`. The remaining fields are optional; when supplied,
`null` is accepted for `fullTimeEmployees`, `grossAnnualRevenue`, and each
sector/location code:

```json
{
  "reportingPeriodId": "00000000-0000-0000-0000-000000000001",
  "fullTimeEmployees": 42,
  "grossAnnualRevenue": null,
  "grossAnnualRevenueUnitId": "00000000-0000-0000-0000-000000000002",
  "highLevelSectorCode": "C",
  "lowLevelSectorCode": null,
  "location": "DENMARK"
}
```

`reportingPeriodId` and `grossAnnualRevenueUnitId` are platform UUIDs.
`fullTimeEmployees` and `grossAnnualRevenue` must be positive when present;
`grossAnnualRevenue` is expressed in the selected revenue unit. Enum spellings
are case-sensitive; inspect `cad-cli schema benchmark` for the complete
`HighLevelSectorCode`, `LowLevelSectorCode`, and `Location` value sets. For
example, `"C"` and `"DENMARK"` are valid. `sectorGroup` appears only in
responses.

## Reporting periods

Reporting periods are a collection. Inspect the credential-free CLI contract
before automating writes:

```console
cad-cli reporting-period --help
cad-cli reporting-period create --help
cad-cli schema reporting-period > reporting-period-cli-schema.json
```

List periods or read one period as JSON:

```console
cad-cli --output json reporting-period list
cad-cli --output json reporting-period get 00000000-0000-0000-0000-000000000001
```

Create a period from a JSON file or stdin. `update` takes the reporting-period
UUID as its first argument and receives the same ID-free JSON object; the CLI
puts that UUID into the API request body where the platform requires it.

```console
cad-cli --output json reporting-period create --input reporting-period.json
printf '%s' '{"description":"Financial year 2026","shortName":"FY26","startDate":"2026-01-01","endDate":"2026-12-31","isLocked":false}' | \
  cad-cli --output json reporting-period create --input -
cad-cli --output json reporting-period update 00000000-0000-0000-0000-000000000001 --input reporting-period.json
```

Every write requires `description`, `shortName`, `startDate`, `endDate`, and
`isLocked`. Dates use ISO 8601 `YYYY-MM-DD` and are inclusive; `shortName` is
at most six characters; `startDate` cannot be later than `endDate`. The server
assigns `id`, so it must not be supplied in input. Use `details` for aggregate
measurement/emissions data. `duplicate --input request.json [--wait]` starts a
copy job; its input extends the create object with `duplicatedReportingPeriodId`
and the required booleans `duplicateEntries`, `duplicateNotes`, and
`duplicateAttachments`. Poll an existing job with `duplication-status <id>`.

## Organizational units

Organizational units form the company hierarchy. Inspect the credential-free
contract before creating or moving a unit:

```console
cad-cli organizational-unit --help
cad-cli schema organizational-unit > organizational-unit-cli-schema.json
```

List the hierarchy, inspect one unit, or count its assigned inventory entries:

```console
cad-cli --output json organizational-unit list
cad-cli --output json organizational-unit get 00000000-0000-0000-0000-000000000001
cad-cli --output json organizational-unit count-entries 00000000-0000-0000-0000-000000000001
```

Create or update a unit using an ID-free JSON object. `parentId` is required on
writes and identifies the parent organizational-unit UUID. The server provides
the new unit's `id` in the response; it must not be included in input.

```console
printf '%s' '{"name":"Operations","position":0,"parentId":"00000000-0000-0000-0000-000000000001"}' | \
  cad-cli --output json organizational-unit create --input -
cad-cli --output json organizational-unit update 00000000-0000-0000-0000-000000000002 --input unit.json
```

`name` is limited to 255 characters and `position` is a zero-based non-negative
sibling order. `duplicate <id> --include-children` returns the duplicated unit
or units; omit the flag to duplicate only the selected unit. `delete <id>` asks
for confirmation unless `--yes` is supplied.

## Shell completions and manual page

Both are generated from the command definition itself, so they never drift from
the binary. Neither needs credentials or a reachable server.

```console
cad-cli completions zsh > "${fpath[1]}/_cad-cli"
cad-cli completions bash > /usr/local/etc/bash_completion.d/cad-cli
cad-cli man > /usr/local/share/man/man1/cad-cli.1
```

`completions` accepts `bash`, `elvish`, `fish`, `powershell` and `zsh`.

Inventory filters use the platform's `attribute_operator=value` form. List
operators are comma-delimited, so a literal comma inside a value is written as
`,,`:

```console
cad-cli inventory-line list --filter 'country_inList=PL,,DE' --sort date_desc
```

## License

MIT
