//! Markdown command reference generated from the command definition.
//!
//! The reference lives in `docs/cli/`, one page per top-level command, and the
//! command table in `README.md` links to it. The test below fails whenever
//! either is out of date; `mise run docs:cli` rewrites both.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Arg, Command, CommandFactory};

use super::Cli;
use super::help::{self, ENVIRONMENT_HEADING, EXAMPLES_HEADING};

/// Set to rewrite the reference instead of checking it.
const UPDATE_VARIABLE: &str = "CAD_CLI_UPDATE_DOCS";
/// Directory of the reference, relative to the repository root.
const DOCS_DIR: &str = "docs/cli";
const README_START: &str = "<!-- cli-commands:start -->";
const README_END: &str = "<!-- cli-commands:end -->";
const GENERATED_NOTE: &str =
    "<!-- Generated from the command definitions by `mise run docs:cli`. Do not edit. -->";

// Writing to a `String` cannot fail, so the `fmt::Result`s below are ignored.

/// Renders every reference page as `(path relative to DOCS_DIR, Markdown)`.
fn render_pages() -> Vec<(String, String)> {
    let root = built_root();
    let mut pages = vec![("README.md".to_owned(), index_page(&root))];
    for command in subcommands(&root) {
        pages.push((format!("{}.md", command.get_name()), command_page(command)));
    }
    pages
}

/// The command tree after clap has propagated global options and bin names.
fn built_root() -> Command {
    let mut root = Cli::command();
    root.build();
    root
}

fn subcommands(command: &Command) -> impl Iterator<Item = &Command> {
    command
        .get_subcommands()
        .filter(|subcommand| subcommand.get_name() != "help" && !subcommand.is_hide_set())
}

fn index_page(root: &Command) -> String {
    let mut page = String::new();
    let _ = writeln!(page, "# cad-cli command reference\n\n{GENERATED_NOTE}\n");
    push_paragraph(&mut page, &about(root));
    push_usage(&mut page, root);

    page.push_str("## Commands\n\n");
    push_command_table(&mut page, root, |command| {
        format!("{}.md", command.get_name())
    });

    page.push_str("## Global options\n\nEvery command accepts these options.\n\n");
    let globals = root
        .get_arguments()
        .filter(|arg| arg.is_global_set())
        .collect::<Vec<_>>();
    push_arg_table(&mut page, "Option", &globals);

    let sections = after_help_sections(root);
    if let Some(variables) = section(&sections, ENVIRONMENT_HEADING) {
        page.push_str("## Environment\n\n| Variable | Description |\n|---|---|\n");
        for (name, description) in variables {
            let _ = writeln!(page, "| `{name}` | {} |", cell(&sentence(description)));
        }
        page.push('\n');
    }
    if let Some(examples) = section(&sections, EXAMPLES_HEADING) {
        page.push_str("## Examples\n\n");
        push_examples(&mut page, examples);
    }
    finish(page)
}

fn command_page(command: &Command) -> String {
    let mut page = String::new();
    let _ = writeln!(page, "# {}\n\n{GENERATED_NOTE}\n", path(command));
    push_paragraph(&mut page, &about(command));
    if command.has_subcommands() {
        push_usage(&mut page, command);
        push_command_table(&mut page, command, |subcommand| {
            format!("#{}", anchor(&path(subcommand)))
        });
        page.push_str(
            "Every command also accepts the [global options](README.md#global-options).\n\n",
        );
        for subcommand in subcommands(command) {
            push_command_section(&mut page, subcommand, 2);
        }
    } else {
        push_command_body(&mut page, command, 1);
        page.push_str(
            "This command also accepts the [global options](README.md#global-options).\n",
        );
    }
    finish(page)
}

fn push_command_section(page: &mut String, command: &Command, level: usize) {
    let _ = writeln!(page, "{} {}\n", "#".repeat(level), path(command));
    push_paragraph(page, &about(command));
    if command.has_subcommands() {
        push_usage(page, command);
        push_command_table(page, command, |subcommand| {
            format!("#{}", anchor(&path(subcommand)))
        });
        for subcommand in subcommands(command) {
            push_command_section(page, subcommand, level + 1);
        }
    } else {
        push_command_body(page, command, level);
    }
}

/// Usage, arguments, options and examples of a runnable command.
fn push_command_body(page: &mut String, command: &Command, level: usize) {
    let heading = "#".repeat(level + 1);
    push_usage(page, command);

    let (positionals, options): (Vec<_>, Vec<_>) = command
        .get_arguments()
        .filter(|arg| !arg.is_global_set() && !arg.is_hide_set() && !is_builtin(arg))
        .partition(|arg| arg.is_positional());
    if !positionals.is_empty() {
        let _ = writeln!(page, "{heading} Arguments\n");
        push_arg_table(page, "Argument", &positionals);
    }
    if !options.is_empty() {
        let _ = writeln!(page, "{heading} Options\n");
        push_arg_table(page, "Option", &options);
    }

    if let Some(examples) = section(&after_help_sections(command), EXAMPLES_HEADING) {
        let _ = writeln!(page, "{heading} Examples\n");
        push_examples(page, examples);
    }
}

fn push_usage(page: &mut String, command: &Command) {
    let usage = command.clone().render_usage().to_string();
    let usage = usage.trim().trim_start_matches("Usage:").trim();
    let _ = writeln!(page, "```console\n{usage}\n```\n");
}

fn push_command_table(page: &mut String, command: &Command, link: impl Fn(&Command) -> String) {
    page.push_str("| Command | Description |\n|---|---|\n");
    for subcommand in subcommands(command) {
        let _ = writeln!(
            page,
            "| [`{}`]({}) | {} |",
            subcommand.get_name(),
            link(subcommand),
            cell(&about(subcommand))
        );
    }
    page.push('\n');
}

fn push_arg_table(page: &mut String, kind: &str, args: &[&Arg]) {
    let _ = writeln!(page, "| {kind} | Description |\n|---|---|");
    for arg in args {
        let _ = writeln!(page, "| `{}` | {} |", term(arg), cell(&describe(arg)));
    }
    page.push('\n');
}

fn push_examples(page: &mut String, examples: &[(String, String)]) {
    for (command, description) in examples {
        let _ = writeln!(page, "{description}:\n\n```console\n{command}\n```\n");
    }
}

fn push_paragraph(page: &mut String, text: &str) {
    if !text.is_empty() {
        let _ = writeln!(page, "{}\n", sentence(text));
    }
}

/// The help and version flags clap adds to every command.
fn is_builtin(arg: &Arg) -> bool {
    matches!(arg.get_id().as_str(), "help" | "version")
}

/// How the argument is written on the command line, e.g. `--limit <LIMIT>`.
fn term(arg: &Arg) -> String {
    let value = arg
        .get_value_names()
        .and_then(|names| names.first())
        .map_or_else(|| arg.get_id().as_str().to_uppercase(), ToString::to_string);
    if arg.is_positional() {
        return format!("<{value}>");
    }

    let mut term = String::new();
    if let Some(short) = arg.get_short() {
        let _ = write!(term, "-{short}, ");
    }
    if let Some(long) = arg.get_long() {
        let _ = write!(term, "--{long}");
    }
    if arg.get_action().takes_values() {
        let _ = write!(term, " <{value}>");
    }
    term
}

/// The argument's help followed by what `--help` shows in brackets.
fn describe(arg: &Arg) -> String {
    let mut parts = Vec::new();
    if arg.is_required_set() && !arg.is_positional() {
        parts.push("Required.".to_owned());
    }
    if let Some(help) = arg.get_long_help().or_else(|| arg.get_help()) {
        parts.push(sentence(&help.to_string()));
    }
    let values = arg
        .get_possible_values()
        .into_iter()
        .filter(|value| !value.is_hide_set())
        .map(|value| format!("`{}`", value.get_name()))
        .collect::<Vec<_>>();
    if !values.is_empty() {
        parts.push(format!("Values: {}.", values.join(", ")));
    }
    let defaults = arg
        .get_default_values()
        .iter()
        .map(|value| format!("`{}`", value.to_string_lossy()))
        .collect::<Vec<_>>();
    if arg.get_action().takes_values() && !defaults.is_empty() {
        parts.push(format!("Default: {}.", defaults.join(", ")));
    }
    parts.join(" ")
}

fn about(command: &Command) -> String {
    command
        .get_long_about()
        .or_else(|| command.get_about())
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// The full invocation, e.g. `cad-cli reporting-period list`.
fn path(command: &Command) -> String {
    command
        .get_bin_name()
        .unwrap_or_else(|| command.get_name())
        .to_owned()
}

/// The anchor GitHub generates for a heading made of words and hyphens.
fn anchor(heading: &str) -> String {
    heading.to_lowercase().replace(' ', "-")
}

/// Clap drops the final period of a one-line doc comment; prose wants it back.
fn sentence(text: &str) -> String {
    let text = text.trim();
    if text.ends_with(['.', '!', '?', ':']) {
        text.to_owned()
    } else {
        format!("{text}.")
    }
}

/// Makes text safe inside one Markdown table cell.
fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn after_help_sections(command: &Command) -> Vec<help::Section> {
    command
        .get_after_long_help()
        .map(|help| help::parse(&help.to_string()))
        .unwrap_or_default()
}

fn section<'a>(sections: &'a [help::Section], heading: &str) -> Option<&'a [(String, String)]> {
    sections
        .iter()
        .find(|section| section.heading == heading)
        .map(|section| section.entries.as_slice())
}

/// Ends the page with exactly one newline.
fn finish(mut page: String) -> String {
    page.truncate(page.trim_end().len());
    page.push('\n');
    page
}

/// The command table for `README.md`, linking into the reference.
fn readme_table() -> String {
    let root = built_root();
    let mut table = String::new();
    push_command_table(&mut table, &root, |command| {
        format!("{DOCS_DIR}/{}.md", command.get_name())
    });
    format!("{}\n", table.trim_end())
}

/// Replaces the text between the README markers with `table`.
fn splice_readme(readme: &str, table: &str) -> String {
    let start = readme
        .find(README_START)
        .unwrap_or_else(|| panic!("README.md has no {README_START} marker"))
        + README_START.len();
    let end = readme
        .find(README_END)
        .unwrap_or_else(|| panic!("README.md has no {README_END} marker"));
    format!("{}\n{table}{}", &readme[..start], &readme[end..])
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Git may check text out with CRLF line endings on Windows.
fn read_normalized(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|text| text.replace("\r\n", "\n"))
}

#[test]
fn command_reference_is_up_to_date() {
    let root = repository_root();
    let docs = root.join(DOCS_DIR);
    let readme_path = root.join("README.md");
    let readme = read_normalized(&readme_path).expect("README.md is readable");

    let mut expected = render_pages()
        .into_iter()
        .map(|(name, content)| (docs.join(name), content))
        .collect::<Vec<_>>();
    expected.push((readme_path, splice_readme(&readme, &readme_table())));

    // Pages of commands that no longer exist.
    let stale = fs::read_dir(&docs)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "md")
                && expected.iter().all(|(expected, _)| expected != path)
        })
        .collect::<Vec<_>>();

    if std::env::var_os(UPDATE_VARIABLE).is_some() {
        fs::create_dir_all(&docs).expect("docs/cli can be created");
        for path in &stale {
            fs::remove_file(path).expect("a stale page can be removed");
        }
        for (path, content) in &expected {
            fs::write(path, content).expect("a page can be written");
        }
        return;
    }

    let mut outdated = expected
        .iter()
        .filter(|(path, content)| read_normalized(path).as_ref() != Some(content))
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    outdated.extend(stale);
    assert!(
        outdated.is_empty(),
        "the command reference is out of date; run `mise run docs:cli`:\n{}",
        outdated
            .iter()
            .map(|path| format!("  {}", path.strip_prefix(&root).unwrap_or(path).display()))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn readme_splice_replaces_only_the_marked_block() {
    let readme = format!("intro\n{README_START}\nold\n{README_END}\noutro\n");
    assert_eq!(
        splice_readme(&readme, "new\n"),
        format!("intro\n{README_START}\nnew\n{README_END}\noutro\n")
    );
}
