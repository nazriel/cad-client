# CAD client for Rust

Proof-of-concept Rust SDK and command-line client for the Carbon+Alt+Delete platform.

The workspace contains exactly two packages:

- `cad-api`: a typed async SDK;
- `cad-client`: application workflows and the `cad-cli` executable.

## Toolchain

The repository is pinned to Rust 1.96.0. Run the complete local check with:

```console
./scripts/check.sh
```

## Install

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
The platform validates positive `fullTimeEmployees` and `grossAnnualRevenue`
values when present; `grossAnnualRevenue` is expressed in the selected revenue
unit. Enum spellings
are case-sensitive; inspect `cad-cli schema benchmark` for the complete
`HighLevelSectorCode`, `LowLevelSectorCode`, and `Location` value sets. For
example, `"C"` and `"DENMARK"` are valid. `sectorGroup` is derived by the
platform and appears only in responses.

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
