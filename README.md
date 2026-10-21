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
mise run docs:cli     # regenerate docs/cli after changing a command
```

CI runs lint and the tests on Linux, macOS and Windows for every pull request,
and builds the release archives. Pushing a tag `v<version>` that matches the
workspace version publishes a GitHub release with archives for Linux (musl),
macOS and Windows on x86_64 and aarch64, plus `SHA256SUMS`.

`cad-cli --version` prints `cad-cli <version>`. Archives CI builds from
anything but a release tag name their commit, for example
`cad-cli 0.1.0-dev+0123456789ab`: the build reads it from
`CAD_CLI_BUILD_COMMIT`, which a local build can set the same way.

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

### Update

An installed binary updates itself in place. `cad-cli update` downloads the
latest release archive for the running platform, verifies it against the
release's `SHA256SUMS` like the installers do, runs the new binary once to
check it reports the release version, and only then atomically replaces the
executable. In a terminal it then prints the release's changes on stderr. It
needs no credentials, but it does need write access to the directory the
binary lives in.

```console
cad-cli update            # install the latest release if it is newer
cad-cli update --check    # only report: exit 0 when up to date, 10 when not
cad-cli update --force    # reinstall the latest release regardless
```

In a terminal, other commands also check for a newer release in the
background, at most once a day (or an hour after a check that got no answer),
and print a one-line notice on stderr after they finish. The check never delays a command and stays silent when it fails.
It is off when stderr is not a terminal, with `--output json` or `jsonl`, and
when `CI` or `CAD_CLI_NO_UPDATE_CHECK` is set to a true value: anything but
empty, `0`, `false`, `no` or `off`, so `CAD_CLI_NO_UPDATE_CHECK=1` turns it off
and `CAD_CLI_NO_UPDATE_CHECK=0` leaves it on. The time of
the last check and the version it found are kept in `cad-cli/update-check.json`
under the user cache directory: `$XDG_CACHE_HOME` (default `~/.cache`) on
Linux, `~/Library/Caches` on macOS and `%LOCALAPPDATA%` on Windows.

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

## Commands

Every command is described, with examples, in the
[command reference](docs/cli/README.md); `cad-cli <command> --help` prints the
same in the terminal.

<!-- cli-commands:start -->
| Command | Description |
|---|---|
| [`activity-category`](docs/cli/activity-category.md) | Manage the company's activity categories |
| [`activity-group`](docs/cli/activity-group.md) | Manage the company's activity groups |
| [`benchmark`](docs/cli/benchmark.md) | Read and update the company benchmark |
| [`context`](docs/cli/context.md) | Show the authenticated session context |
| [`company`](docs/cli/company.md) | Inspect and create companies available to the current API key |
| [`company-template`](docs/cli/company-template.md) | Inspect reusable company templates available to the current API key |
| [`reporting-period`](docs/cli/reporting-period.md) | Manage reporting periods and duplication jobs |
| [`inventory-line`](docs/cli/inventory-line.md) | Read and mutate inventory lines |
| [`organizational-unit`](docs/cli/organizational-unit.md) | Manage the company's organizational units |
| [`document`](docs/cli/document.md) | Upload and download supporting documents |
| [`custom-field`](docs/cli/custom-field.md) | Manage custom fields |
| [`custom-option`](docs/cli/custom-option.md) | Manage options belonging to custom fields |
| [`report`](docs/cli/report.md) | Create and inspect generated reports |
| [`schema`](docs/cli/schema.md) | Print a machine-readable schema for a CLI resource |
| [`completions`](docs/cli/completions.md) | Print a shell completion script that also completes IDs from the platform |
| [`man`](docs/cli/man.md) | Print the manual page in roff format |
| [`update`](docs/cli/update.md) | Update cad-cli to the latest release |
<!-- cli-commands:end -->

## Input and output

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

## Companies

Companies and reusable company templates are session-level resources; they do not
use `--company-id`. Inspect their credential-free contracts before automating
creation:

```console
cad-cli company --help
cad-cli company-template --help
cad-cli schema company > company-cli-schema.json
cad-cli schema company-template > company-template-cli-schema.json
```

List templates before creating a company from one. `company create` receives a
camel-case JSON object through `--input <file|->` and asks for confirmation
before sending the request:

```console
cad-cli --output json company-template list
printf '%s' '{"name":"Acme GmbH","template":"TEMPLATE_COMPANY","companyTemplateId":"00000000-0000-0000-0000-000000000001"}' | \
  cad-cli --output json company create --input -
```

Creating a company may affect the next invoice. The confirmation prompt shows
`(y/N)`, so declining is the default. Pass `--yes` only when that consequence
has been reviewed. Non-interactive creation without `--yes` fails before making
the request. `name` is required by the API but may be explicitly `null`;
`template` defaults to `DEFAULT`. The CLI permits `DEFAULT` and
`TEMPLATE_COMPANY`; internal demonstration templates are intentionally not
available. The optional `companyTemplateId` is a template UUID returned by
`company-template list`.

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

## Activity categories

Activity categories describe the sources included in the company inventory. Inspect
the credential-free contract before automating writes:

```console
cad-cli activity-category --help
cad-cli schema activity-category > activity-category-cli-schema.json
```

List categories, update one from stdin, or duplicate an existing category:

```console
cad-cli --output json activity-category list
printf '%s' '{"name":"Purchased Electricity","description":"Electricity purchased for operations.","inclusionCriteria":"Includes purchased electricity.","exclusionCriteria":"Excludes onsite generation.","activityCategoryType":"PURCHASED_ELECTRICITY","activityGroupName":"Scope 2","activityGroupId":"00000000-0000-0000-0000-000000000001","position":0,"isIncluded":true}' | \
  cad-cli --output json activity-category update 00000000-0000-0000-0000-000000000002 --input -
cad-cli --output json activity-category duplicate 00000000-0000-0000-0000-000000000002
```

Writes require the category name, description, inclusion and exclusion criteria,
`activityCategoryType`, parent `activityGroupId`, zero-based `position`, and
`isIncluded`. `activityGroupName` is optional. Category names are limited to 128
characters. The deprecated API create route is intentionally not exposed by the
CLI. The platform returns server-managed fields such as `id`, measurement counts,
default status, and scope/reference metadata. `delete <id>` asks for confirmation
unless `--yes` is supplied.

## Activity groups

Activity groups organize activity categories, such as Scope 1 and Scope 2. Inspect
the credential-free CLI contract before automating writes:

```console
cad-cli activity-group --help
cad-cli schema activity-group > activity-group-cli-schema.json
```

List groups or create and update one with an ID-free JSON object:

```console
cad-cli --output json activity-group list
printf '%s' '{"position":0,"name":"Scope 1","description":"Direct emissions from owned sources."}' | \
  cad-cli --output json activity-group create --input -
cad-cli --output json activity-group update 00000000-0000-0000-0000-000000000001 --input activity-group.json
```

Every write requires a zero-based non-negative `position`, a `name` of at most
128 characters, and a `description`. The platform assigns the `id` returned by
create and update; it must not be included in input. `delete <id>` asks for
confirmation unless `--yes` is supplied.

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

## Inventory lines

Inventory filters use the platform's `attribute_operator=value` form. List
operators are comma-delimited, so a literal comma inside a value is written as
`,,`:

```console
cad-cli inventory-line list --filter 'country_inList=PL,,DE' --sort date_desc
```

## Shell completions and manual page

Completion is dynamic: on Tab, the shell runs `cad-cli` to ask for candidates,
so commands, options and values never drift from the binary. Load the
registration from your shell's startup file, which keeps it in step with
upgrades:

```console
# ~/.zshrc
source <(cad-cli completions zsh)
# ~/.bashrc (bash 3.2, the macOS default, cannot `source <(...)`)
eval "$(cad-cli completions bash)"
# ~/.config/fish/config.fish
cad-cli completions fish | source
```

`cad-cli completions <shell>` prints exactly what `COMPLETE=<shell> cad-cli`
prints and needs no credentials or server. Writing the script to a file works
too; regenerate it after upgrading. The [completions](docs/cli/completions.md)
and [man](docs/cli/man.md) references show where to install the script and the
roff manual page.

`--company-id` and the reporting-period IDs of `reporting-period get`,
`details`, `update` and `delete` are completed from the platform, with the
company or period name shown beside each ID where the shell supports it. These
lookups use the same configuration as any other command (`CAD_API_KEY`,
`CAD_API_SECRET`, the server, and `--company-id` or `CAD_COMPANY_ID` for
reporting periods) and follow strict rules:

- they never prompt and print nothing but candidates; without credentials or a
  reachable server the IDs are simply not offered;
- a live lookup is abandoned after two seconds;
- answers are cached for an hour in `cad-cli/completions/` under the user cache
  directory (`$XDG_CACHE_HOME`, else `~/Library/Caches` on macOS, `~/.cache` on
  Linux and `%LOCALAPPDATA%` on Windows), one file per server, API key, company
  and resource. When a lookup fails, the last cached answer is used however old
  it is. The files hold IDs and names only: no secret, and the API key ID only
  as a hash;
- `CAD_CLI_COMPLETE_OFFLINE=1` answers from the cache alone and never contacts
  the platform.

## License

MIT
