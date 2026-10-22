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
under the user cache directory: `$XDG_CACHE_HOME` when it is set, else
`~/.cache` on Linux, `~/Library/Caches` on macOS and `%LOCALAPPDATA%` on
Windows.

## Configure

Every platform request uses `Authorization: Bearer <Key ID>.<Secret>`, the
API key you create on the platform's API keys page
(`<server>/app/user-account-settings/api-keys`). `cad-cli use` keeps the
servers you work with, the key for each and the company to act on, so that a
command needs no flags.

### Profiles

A profile is a name for a server, the login made there and the company to act
on. `cad-cli use profile <name>` is the first step:

```console
cad-cli use profile production
```

A name that is new creates the profile and asks for its server. `production`,
`staging` and `dev` suggest `https://app.carbonaltdelete.eu`,
`https://staging.carbonaltdelete.eu` and `https://dev.carbonaltdelete.eu`:
press Enter to take it. For any other name, or to answer in advance, pass
`--url`. A server on `localhost` or another loopback host needs plain HTTP,
which is refused unless you also pass `--allow-insecure-http`; the profile
remembers it:

```console
cad-cli use profile local --url http://localhost:5001 --allow-insecure-http
```

A profile name is 1 to 32 characters of `a-z`, `0-9`, `-` and `_`. Several
profiles may use the same server. Naming a profile that exists makes it the
active one, which every command uses unless `--profile` or `CAD_PROFILE` names
another.

A profile with no login is offered one. This prints the API keys page and opens
it in your browser. The page shows the **Key ID** (a UUID) and the **Secret**
(`cad_aks_` followed by 43 letters, digits, `-` or `_`) as two values; paste
each when asked. A malformed value is refused with the reason and asked again,
and the Secret is masked with `*` as you type. Esc or Ctrl-C cancels. The key is
checked with the server before anything is saved, and a rejected key is not
stored. Create the key on the page first: it cannot be created from the command
line.

- `--no-browser` only prints the page address.
- `--with-token` reads `<Key ID>.<Secret>` from standard input instead of
  asking, for scripts and machines without a terminal:
  `printf '%s.%s' "$KEY_ID" "$SECRET" | cad-cli use profile staging --with-token`.
- `--login` logs in again, for a profile that already has a login.
- Without a terminal nothing is asked: a new profile of another name than
  `production`, `staging` or `dev` stops with exit status 2 and asks for
  `--url`, and a profile with no login is left without one and says how to give
  it one.

Once the login is made, the company is chosen: the only company your login can
act on is taken, and with several you pick one in a terminal. See
[Companies](#companies) to choose or change it.

```console
cad-cli use                      # every profile, the active one marked
cad-cli use --check              # check what a command would use with the server
cad-cli use profile staging --forget
```

`cad-cli use` lists the profiles, sorted by name, with the server, the account
and the company of each; `active` marks the profile `use profile` last switched
to, `selected` the one a command run now would use, which differs when
`--profile`, `CAD_PROFILE` or a server given with `--server-url` or
`CAD_SERVER_URL` picks another. It reads only your files and makes no request.
`cad-cli use --check` resolves the profile, server, login and company the way a
command does, connects once, and prints who the login is for and which company
is selected, with where each came from; it does not fail for a missing
company. `use profile <name> --forget` forgets the profile and the key stored
for it. The key itself stays valid until you revoke it on the API keys page.

### Companies

`cad-cli use company <name|ID>` chooses the company the profile acts on:

```console
cad-cli use company "Acme GmbH"
cad-cli use company 00000000-0000-0000-0000-000000000001
```

A name is matched exactly, ignoring case and surrounding spaces; a name several
companies share is refused with their IDs. `use company` offers, and accepts,
only the companies your login can act on: your own company and those you hold a
role in. Any other company is refused at once, with the ones you can choose
listed; `cad-cli company list` still shows every company the key can see. The
company is checked with the server before it is saved, and it is kept in the
profile, so `cad-cli --profile staging use company …` chooses it for another
profile without switching to it.

A command that acts on a company stops with exit status 2 when none is
selected, rather than guess, and says how to choose one. Pass `--company-id`
for one command, or set `CAD_COMPANY_ID` for one shell; both take precedence
over the profile's company. `company` and `company-template` act on the
session, not on a company, and need none.

### Where the login is kept

Each profile has its own stored login, which is used only for the server it
was made for.

| Where | Used when |
|---|---|
| The system keyring: macOS Keychain, Windows Credential Manager, or the Secret Service (GNOME Keyring, KWallet) on Linux | It works. This is tried first. |
| A file, `cad-cli/credentials.json` in the user configuration directory | The keyring cannot be used and you agree to the file. |

The configuration directory is `$XDG_CONFIG_HOME` when it is set to an absolute
path, else `~/.config` on Linux, `~/Library/Application Support` on macOS and
`%APPDATA%` on Windows. On Unix the directory is created readable only by you
(`0700`) and the file is written `0600`; a file that anyone else can read is
refused, and the error gives the `chmod 600` command that fixes it. The file
holds the Key ID and the Secret as plain text, which is why the keyring is
preferred.

When the keyring cannot be used (no Secret Service is running, it is locked, or
this is a headless machine), `use profile` says why and asks
`Store the credentials in <path> instead, readable only by you? (y/N)`. `--yes`
does not answer it. Without a terminal it stops with exit status 2 and stores
nothing, unless you pass `--insecure-storage`, which skips the keyring and
writes the file. A profile has one stored login: saving it in one place removes
it from the other.

Each stored login records the server origin (`scheme://host[:port]`) it was
made for. A key for `https://staging.carbonaltdelete.eu` is never sent to
another host, nor to another port of the same host, whatever the profile is
called.

`profiles.json` sits beside `credentials.json`. It holds each profile's server,
whether it may use plain HTTP, the Key ID, account ID, e-mail and storage place
of its login, and its company, plus which profile is active. It holds no
secret.

Set `CAD_CLI_KEYRING=0` (also `false`, `no` or `off`) to never touch the system
keyring and use only the file, for example on a server without a desktop
session.

### Choosing per command or per shell, and in scripts

`cad-cli use` changes the active profile and the profile's company for every
shell you have, until the next `use`. To choose for one command or one shell
instead, use:

| For | Per command | Per shell |
|---|---|---|
| The profile | `--profile <name>` | `CAD_PROFILE` |
| The server | `--server-url <origin>` | `CAD_SERVER_URL` |
| The login | | `CAD_API_KEY` and `CAD_API_SECRET` |
| The company | `--company-id <ID>` | `CAD_COMPANY_ID` |

A run with no profile file at all, as in CI, needs only the variables:

```console
export CAD_SERVER_URL=https://staging.carbonaltdelete.eu
export CAD_API_KEY=00000000-0000-0000-0000-000000000000
export CAD_API_SECRET='...'
export CAD_COMPANY_ID=00000000-0000-0000-0000-000000000001
```

**Programs and AI agents** should not call `cad-cli use` to choose what a
command acts on: it changes the selection for every shell of the user, a person
at another terminal included. Pass `--profile` and `--company-id`, or set the
variables above, on each command instead. Nothing prompts without a terminal,
and a failure names the command or flag that fixes it; with `--output json`
that is the `hint` of the error (see [Input and output](#input-and-output)).

The order is fixed. Each choice is made from the first source that has it:

The profile:

1. `--profile`, then `CAD_PROFILE`. The profile must exist, and a server given
   as well must be its own: `profile <name> logs in to <origin>, not <other>`
   is an error.
2. Otherwise, with a server given (`--server-url`, then `CAD_SERVER_URL`), the
   profile whose server it is: the active profile if it matches, else the only
   profile that does. Several matching profiles are an error asking for
   `--profile`; none means no profile, so no stored login and no stored company.
3. Otherwise the active profile.

The server:

1. `--server-url`, then `CAD_SERVER_URL`.
2. The server of the selected profile.
3. Otherwise the command stops with exit status 2 and asks you to run
   `cad-cli use profile <name>`; there is deliberately no implicit server
   default.

The credentials:

1. If `CAD_API_KEY` or `CAD_API_SECRET` is set, the environment decides alone,
   and a missing half is an error. A stored login is not consulted.
2. Otherwise the selected profile's login in the system keyring, unless
   `CAD_CLI_KEYRING=0`. A keyring that cannot answer is not an error: the lookup
   goes on.
3. Then its login in the credentials file.
4. Otherwise the command stops with exit status 2 and asks you to run
   `cad-cli use profile <name> --login` or set the variables. A login made for
   another server than the one selected is never used.

The company:

1. `--company-id`.
2. `CAD_COMPANY_ID`.
3. The company of the selected profile.
4. Otherwise a command that acts on a company stops with exit status 2.

Shell completion selects the same way.

Every API request has a one-minute total budget; change it with
`--request-timeout 90s`. Durations are written with an explicit unit and may
combine several — `1500ms`, `90s`, `1m30s`, `30m` — a bare number is rejected
rather than guessed at. The budget does not apply to document transfers, which
may run as long as they keep making progress. Plain HTTP is rejected unless the
server is a loopback host and `--allow-insecure-http` is passed (or the profile
was created with it).

## Commands

Every command is described, with examples, in the
[command reference](docs/cli/README.md); `cad-cli <command> --help` prints the
same in the terminal.

<!-- cli-commands:start -->
| Command | Description |
|---|---|
| [`use`](docs/cli/use.md) | Show and switch the active profile, log in, and choose the company |
| [`activity-category`](docs/cli/activity-category.md) | Manage the company's activity categories |
| [`activity-group`](docs/cli/activity-group.md) | Manage the company's activity groups |
| [`benchmark`](docs/cli/benchmark.md) | Read and update the company benchmark |
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

With `--output json` or `jsonl` a failure is one JSON object on a single line
of stderr, and stdout stays empty:

```json
{"error":{"kind":"config","message":"no company is selected for profile staging","hint":"run `cad-cli use company <name|id>`, or pass --company-id or set CAD_COMPANY_ID","exitCode":2,"status":null,"details":null}}
```

- `kind` is one of `config`, `authConfig`, `authentication`, `api`,
  `transport`, `timeout`, `input`, `io` or `update`. A cancelled prompt is not
  reported as JSON.
- `message` says what went wrong, without the hint.
- `hint` is the command or flag that fixes it, or `null`.
- `exitCode` is the process exit status, the same number the shell sees.
- `status` is the HTTP status when the platform rejected a request, else
  `null`; `details` is the error body the platform sent, if it sent one, else
  `null`.

Errors of the argument parser itself, such as an unknown command or option,
come before the output format is known and stay plain text.

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
lookups use the same configuration as any other command (the selected profile
and its login, or `CAD_API_KEY` and `CAD_API_SECRET`, the server, and the
company for reporting periods) and follow strict rules:

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

MIT.

The Linux builds link libdbus statically to reach the system keyring. libdbus
is licensed under the Academic Free License 2.1 or the GPL 2.0 or later, and
cad-cli uses it under the former; its license ships in the Linux archives as
`LICENSE-dbus`.
