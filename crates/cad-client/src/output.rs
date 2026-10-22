use std::borrow::Cow;
use std::collections::BTreeSet;
use std::io::{IsTerminal, Write};

use comfy_table::{Attribute, Cell, ColumnConstraint, ContentArrangement, Table, Width, presets};
use serde::Serialize;
use serde_json::{Map, Value};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::cli::OutputFormat;
use crate::error::{CliError, ErrorKind};

/// Longest cell body rendered before it is shortened.
const MAX_CELL_WIDTH: usize = 45;
const ELLIPSIS: &str = "...";

/// Narrowest a framed column may become while fitting the table to the terminal.
///
/// Without a floor, a result with many columns in a narrow terminal is squeezed
/// down to one character per column, which is unreadable. At the floor the table
/// overflows instead, and the terminal wraps it.
const MIN_COLUMN_WIDTH: u16 = 12;

/// How much decoration the `table` format may use.
///
/// Only this type knows about terminals; the renderer itself is a pure function
/// of the value and the style, which is what keeps it testable — including the
/// width, which is passed in rather than read from the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TableStyle {
    /// Terminal width the framed table is fitted to. `None` renders the plain
    /// ASCII form, which is never reflowed.
    width: Option<u16>,
    /// Emit ANSI attributes such as a bold header row.
    colour: bool,
}

impl TableStyle {
    /// ASCII only, no ANSI, one line per row: safe to pipe into `grep` or `cut`.
    pub(crate) const PLAIN: Self = Self {
        width: None,
        colour: false,
    };

    /// Reads the state of stdout: a redirected stream gets exactly [`Self::PLAIN`],
    /// and a terminal gets frames sized to it, plus colour if that terminal
    /// permits it — `console` already honours `NO_COLOR` and `CLICOLOR`, so that
    /// decision is not reimplemented here.
    pub(crate) fn detect() -> Self {
        if std::io::stdout().is_terminal() {
            Self {
                width: Some(console::Term::stdout().size().1),
                colour: console::colors_enabled(),
            }
        } else {
            Self::PLAIN
        }
    }
}

/// Converts a command result into the single `Value` the renderer consumes.
///
/// # Errors
///
/// Returns [`CliError`] when the result cannot be serialized.
pub(crate) fn to_value(value: impl Serialize) -> Result<Value, CliError> {
    serde_json::to_value(value)
        .map_err(|error| CliError::Input(format!("could not serialize command result: {error}")))
}

/// Serializes a command result, decorating the `table` format as `style` allows.
///
/// `json` and `jsonl` ignore the style entirely: those formats are a machine
/// contract and must stay byte-for-byte identical wherever they are written.
///
/// # Errors
///
/// Returns [`CliError`] when serialization or writing fails.
pub(crate) fn write_styled<T: Serialize>(
    value: &T,
    format: OutputFormat,
    destination: &mut dyn Write,
    style: TableStyle,
) -> Result<(), CliError> {
    let value = serde_json::to_value(value)
        .map_err(|error| CliError::Input(format!("could not serialize output: {error}")))?;
    match format {
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut *destination, &value)
                .map_err(|error| CliError::Io(std::io::Error::other(error)))?;
            writeln!(destination)?;
        }
        OutputFormat::Jsonl => write_jsonl(&value, destination)?,
        OutputFormat::Table => write_table(&value, destination, style)?,
    }
    Ok(())
}

/// The failure as `--output json` and `jsonl` report it on stderr.
#[derive(Serialize)]
struct ErrorReport<'a> {
    error: ErrorBody<'a>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody<'a> {
    kind: ErrorKind,
    message: Cow<'a, str>,
    /// How to fix the failure, when the error says so.
    hint: Option<&'a str>,
    exit_code: u8,
    /// The HTTP status, when the platform rejected a request.
    status: Option<u16>,
    /// The error body the platform sent, if it sent one.
    details: Option<&'a Value>,
}

/// Writes `error` as one line of JSON, the form a program reads on stderr.
///
/// # Errors
///
/// Returns [`CliError`] when serialization or writing fails.
pub(crate) fn write_error(error: &CliError, destination: &mut dyn Write) -> Result<(), CliError> {
    let report = ErrorReport {
        error: ErrorBody {
            kind: error.kind(),
            message: error.message(),
            hint: error.hint(),
            exit_code: error.exit_status(),
            status: error.status(),
            details: error.details(),
        },
    };
    serde_json::to_writer(&mut *destination, &report)
        .map_err(|error| CliError::Io(std::io::Error::other(error)))?;
    writeln!(destination)?;
    Ok(())
}

fn write_jsonl(value: &Value, destination: &mut dyn Write) -> Result<(), CliError> {
    if let Value::Array(items) = value {
        for item in items {
            serde_json::to_writer(&mut *destination, item)
                .map_err(|error| CliError::Io(std::io::Error::other(error)))?;
            writeln!(destination)?;
        }
    } else {
        serde_json::to_writer(&mut *destination, value)
            .map_err(|error| CliError::Io(std::io::Error::other(error)))?;
        writeln!(destination)?;
    }
    Ok(())
}

fn write_table(
    value: &Value,
    destination: &mut dyn Write,
    style: TableStyle,
) -> Result<(), CliError> {
    let rows: Vec<Map<String, Value>> = match value {
        Value::Array(items) => items.iter().map(as_row).collect(),
        other => vec![as_row(other)],
    };
    if rows.is_empty() {
        writeln!(destination, "(no results)")?;
        return Ok(());
    }

    // Sorted keys give the same column order for every payload, so a script
    // that reads a column by position keeps working across runs.
    let headers: Vec<String> = rows
        .iter()
        .flat_map(|row| row.keys().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let mut table = Table::new();
    table.load_style(if style.width.is_some() {
        presets::UTF8_FULL.with_rounded_corners()
    } else {
        // The plain preset is borderless, so it has no corners to round.
        presets::ASCII_NO_BORDERS
    });
    if style.colour {
        table.set_header(
            headers
                .iter()
                .map(|header| Cell::new(header).add_attribute(Attribute::Bold)),
        );
    } else {
        // Suppresses every ANSI attribute comfy-table would otherwise add.
        table.force_no_tty();
        table.set_header(headers.iter().map(Cell::new));
    }
    for row in &rows {
        table.add_row(
            headers
                .iter()
                .map(|header| cell(row.get(header).unwrap_or(&Value::Null))),
        );
    }

    if let Some(width) = style.width {
        table.set_width(width);
        table.set_content_arrangement(ContentArrangement::Dynamic);
        for column in table.column_iter_mut() {
            column.set_constraint(ColumnConstraint::LowerBoundary(Width::Fixed(
                MIN_COLUMN_WIDTH,
            )));
        }
    } else {
        // Cells were shortened before they reached the table and nothing
        // reflows them, so a piped row always occupies exactly one line.
        table.set_content_arrangement(ContentArrangement::Disabled);
    }

    writeln!(destination, "{table}")?;
    Ok(())
}

fn as_row(value: &Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map.clone(),
        other => Map::from_iter([("value".to_owned(), other.clone())]),
    }
}

fn cell(value: &Value) -> String {
    let rendered = match value {
        Value::Null => "-".to_owned(),
        Value::String(value) => value.replace(['\n', '\r'], " "),
        other => other.to_string(),
    };
    match truncate_to_width(&rendered, MAX_CELL_WIDTH) {
        Some(shortened) => format!("{shortened}{ELLIPSIS}"),
        None => rendered,
    }
}

/// Returns the prefix of `text` that fits in `width` terminal columns, or
/// `None` when the whole string already fits.
///
/// Measuring in columns rather than in `char`s is what keeps wide scripts and
/// emoji from shearing the table.
fn truncate_to_width(text: &str, width: usize) -> Option<String> {
    if text.width() <= width {
        return None;
    }
    let mut used = 0;
    let mut shortened = String::new();
    for character in text.chars() {
        let next = character.width().unwrap_or(0);
        if used + next > width {
            break;
        }
        used += next;
        shortened.push(character);
    }
    Some(shortened)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(value: &Value, format: OutputFormat, style: TableStyle) -> String {
        let mut bytes = Vec::new();
        write_styled(value, format, &mut bytes, style).expect("rendering succeeds");
        String::from_utf8(bytes).expect("output is UTF-8")
    }

    fn error_json(error: &CliError) -> Value {
        let mut bytes = Vec::new();
        write_error(error, &mut bytes).expect("writing succeeds");
        let text = String::from_utf8(bytes).expect("output is UTF-8");
        assert_eq!(text.lines().count(), 1, "{text}");
        assert!(text.ends_with('\n'));
        serde_json::from_str(&text).expect("one JSON object")
    }

    #[test]
    fn a_hinted_error_keeps_its_message_and_hint_apart() {
        assert_eq!(
            error_json(&CliError::config_with_hint(
                "no profile is selected",
                "run `cad-cli use profile <name>`"
            )),
            serde_json::json!({"error": {
                "kind": "config",
                "message": "no profile is selected",
                "hint": "run `cad-cli use profile <name>`",
                "exitCode": 2,
                "status": null,
                "details": null
            }})
        );
    }

    #[test]
    fn an_api_error_carries_its_status_and_details() {
        let error = CliError::from(cad_api::Error::Api {
            status: 403,
            message: "forbidden".to_owned(),
            details: Some(serde_json::json!({"msg": "not your company"})),
        });
        assert_eq!(
            error_json(&error),
            serde_json::json!({"error": {
                "kind": "api",
                "message": "HTTP 403: forbidden",
                "hint": null,
                "exitCode": 4,
                "status": 403,
                "details": {"msg": "not your company"}
            }})
        );
    }

    #[test]
    fn an_io_error_has_no_hint_status_or_details() {
        let error = CliError::Io(std::io::Error::other("disk full"));
        assert_eq!(
            error_json(&error),
            serde_json::json!({"error": {
                "kind": "io",
                "message": "disk full",
                "hint": null,
                "exitCode": 5,
                "status": null,
                "details": null
            }})
        );
    }

    #[test]
    fn jsonl_emits_one_object_per_line() {
        let mut bytes = Vec::new();
        write_styled(
            &vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})],
            OutputFormat::Jsonl,
            &mut bytes,
            TableStyle::PLAIN,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "{\"id\":1}\n{\"id\":2}\n"
        );
    }

    #[test]
    fn machine_formats_ignore_the_table_style() {
        let value = serde_json::json!([{"id": 1}, {"id": 2}]);
        for format in [OutputFormat::Json, OutputFormat::Jsonl] {
            assert_eq!(
                rendered(&value, format, TableStyle::PLAIN),
                rendered(
                    &value,
                    format,
                    TableStyle {
                        width: Some(80),
                        colour: true
                    }
                ),
                "decoration must never reach a machine-readable format"
            );
        }
    }

    #[test]
    fn wide_characters_do_not_shear_the_table() {
        let table = rendered(
            &serde_json::json!([{"name": "東京", "id": 1}, {"name": "ab", "id": 2}]),
            OutputFormat::Table,
            TableStyle::PLAIN,
        );
        let widths: Vec<usize> = table.lines().map(UnicodeWidthStr::width).collect();

        assert!(
            widths.windows(2).all(|pair| pair[0] == pair[1]),
            "every row must occupy the same terminal width:\n{table}"
        );
    }

    #[test]
    fn table_has_stable_sorted_headers() {
        let table = rendered(
            &serde_json::json!([{"z": 1, "a": "x", "m": true}]),
            OutputFormat::Table,
            TableStyle::PLAIN,
        );
        let header = table
            .lines()
            .find(|line| line.contains('a') && line.contains('z'))
            .expect("the header row lists every column");

        assert!(header.find('a') < header.find('m'));
        assert!(header.find('m') < header.find('z'));
    }

    #[test]
    fn a_narrow_terminal_overflows_rather_than_shredding_the_columns() {
        // Eight columns cannot honestly fit in eighty cells. Fitting them anyway
        // collapses every column to a single character, which is unreadable, so
        // the table is allowed to exceed the width instead.
        let table = rendered(
            &serde_json::json!([{
                "availableClientCompanies": null, "companyStatus": "ACTIVE",
                "createdAt": "2026-01-01T00:00:00Z", "displayName": "Acme NV",
                "id": "00000000-0000-0000-0000-000000000042",
                "isConsultingCompany": false, "isDemoCompany": false, "name": "Acme"
            }]),
            OutputFormat::Table,
            TableStyle {
                width: Some(80),
                colour: false,
            },
        );

        let narrowest = table
            .lines()
            .next()
            .expect("the table has a top border")
            .split(['┬', '╭', '╮'])
            .filter(|segment| !segment.is_empty())
            .map(UnicodeWidthStr::width)
            .min()
            .expect("the border describes every column");
        assert!(
            narrowest >= usize::from(MIN_COLUMN_WIDTH),
            "columns collapsed to {narrowest} cells:\n{table}"
        );
    }

    #[test]
    fn a_wide_terminal_fits_the_table_to_its_width() {
        let table = rendered(
            &serde_json::json!([{"note": "x".repeat(200)}]),
            OutputFormat::Table,
            TableStyle {
                width: Some(60),
                colour: false,
            },
        );

        assert!(
            table.lines().all(|line| line.width() <= 60),
            "a single wide column must be reflowed to fit:\n{table}"
        );
    }

    #[test]
    fn a_piped_table_carries_no_ansi_and_stays_line_parseable() {
        let table = rendered(
            &serde_json::json!([{"id": 1, "name": "Acme"}]),
            OutputFormat::Table,
            TableStyle::PLAIN,
        );

        assert!(!table.contains('\u{1b}'), "no escape sequence may appear");
        assert!(table.is_ascii(), "a plain table stays ASCII:\n{table}");
        let data = table
            .lines()
            .find(|line| line.contains("Acme"))
            .expect("the row is on one line of its own");
        assert!(data.contains('1'), "the whole row shares a line: {data}");
    }
}
