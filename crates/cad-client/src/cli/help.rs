//! Extra `--help` sections rendered after the options: examples and the
//! environment variables a command reads.
//!
//! The sections are declared as data rather than preformatted text, so the
//! terminal rendering and the generated Markdown reference share one source,
//! and the examples can be parsed back and checked against the real parser.

use std::fmt::Write as _;

use clap::builder::StyledStr;
use clap::builder::styling::Style;

/// One example invocation and what it does.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Example {
    pub(crate) command: &'static str,
    pub(crate) description: &'static str,
}

impl Example {
    pub(crate) const fn new(command: &'static str, description: &'static str) -> Self {
        Self {
            command,
            description,
        }
    }
}

/// One environment variable and what it controls.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Variable {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
}

impl Variable {
    pub(crate) const fn new(name: &'static str, description: &'static str) -> Self {
        Self { name, description }
    }
}

pub(crate) const EXAMPLES_HEADING: &str = "Examples";
pub(crate) const ENVIRONMENT_HEADING: &str = "Environment";

const HEADING: Style = Style::new().bold().underline();
const LITERAL: Style = Style::new().bold();
const NOTE: Style = Style::new().dimmed();

/// Renders an `Examples:` section for `after_long_help`.
pub(crate) fn examples(examples: &[Example]) -> StyledStr {
    let mut help = String::new();
    push_examples(&mut help, examples);
    help.into()
}

/// Renders an `Environment:` section followed by an `Examples:` section.
pub(crate) fn environment_and_examples(variables: &[Variable], examples: &[Example]) -> StyledStr {
    let mut help = String::new();
    push_heading(&mut help, ENVIRONMENT_HEADING);
    for variable in variables {
        // Laid out like clap's own long option help: the term on its own line,
        // the description below it, and a blank line after each entry.
        let _ = writeln!(
            help,
            "  {LITERAL}{}{LITERAL:#}\n          {}\n",
            variable.name, variable.description
        );
    }
    push_examples(&mut help, examples);
    help.into()
}

// Writing to a `String` cannot fail, so the `fmt::Result`s below are ignored.

fn push_heading(help: &mut String, heading: &str) {
    let _ = writeln!(help, "{HEADING}{heading}:{HEADING:#}");
}

fn push_examples(help: &mut String, examples: &[Example]) {
    push_heading(help, EXAMPLES_HEADING);
    for (index, example) in examples.iter().enumerate() {
        if index > 0 {
            help.push('\n');
        }
        let _ = writeln!(
            help,
            "  {LITERAL}{}{LITERAL:#}\n    {NOTE}└─ {}{NOTE:#}",
            example.command, example.description
        );
    }
}

/// A section recovered from the plain-text rendering of `after_long_help`.
#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub(crate) heading: String,
    /// `(term, description)` pairs: a command and what it does, or a variable
    /// and what it controls.
    pub(crate) entries: Vec<(String, String)>,
}

/// Parses the layout written above back into sections.
///
/// Only the documentation generator and the tests read help this way, and they
/// only ever read text this module produced.
#[cfg(test)]
pub(crate) fn parse(plain: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    for line in plain.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(heading) = line.strip_suffix(':').filter(|_| !line.starts_with(' ')) {
            sections.push(Section {
                heading: heading.to_owned(),
                entries: Vec::new(),
            });
            continue;
        }
        let section = sections
            .last_mut()
            .expect("help entries always follow a heading");
        let indent = line.len() - line.trim_start().len();
        if indent == 2 {
            section
                .entries
                .push((line.trim().to_owned(), String::new()));
        } else {
            let entry = section
                .entries
                .last_mut()
                .expect("a description always follows its term");
            let text = line.trim().trim_start_matches("└─ ");
            if !entry.1.is_empty() {
                entry.1.push(' ');
            }
            entry.1.push_str(text);
        }
    }
    sections
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::{Example, Section, Variable};
    use crate::cli::Cli;

    #[test]
    fn sections_render_with_styles_and_parse_back_from_plain_text() {
        let help = super::environment_and_examples(
            &[Variable::new("CAD_ENV", "Select a preset")],
            &[
                Example::new("cad-cli company list", "List companies"),
                Example::new("cad-cli context show", "Show the session"),
            ],
        );

        let styled = help.ansi().to_string();
        assert!(styled.contains("\x1b[1m\x1b[4mExamples:"), "{styled:?}");
        assert!(styled.contains("\x1b[1mcad-cli company list"), "{styled:?}");

        assert_eq!(
            super::parse(&help.to_string()),
            [
                Section {
                    heading: "Environment".to_owned(),
                    entries: vec![("CAD_ENV".to_owned(), "Select a preset".to_owned())],
                },
                Section {
                    heading: "Examples".to_owned(),
                    entries: vec![
                        (
                            "cad-cli company list".to_owned(),
                            "List companies".to_owned()
                        ),
                        (
                            "cad-cli context show".to_owned(),
                            "Show the session".to_owned()
                        ),
                    ],
                },
            ]
        );
    }

    /// Every runnable command documents at least one example, and every
    /// example is a real invocation of the command it documents: a renamed
    /// flag or command breaks this test instead of leaving stale help behind.
    #[test]
    fn every_command_has_examples_that_the_parser_accepts() {
        let mut root = Cli::command();
        root.build();
        let mut checked = 0;
        visit(&root, &mut Vec::new(), &mut checked);
        assert!(checked > 0);
    }

    fn visit(command: &clap::Command, path: &mut Vec<String>, checked: &mut usize) {
        let examples = command
            .get_after_long_help()
            .map(|help| super::parse(&help.to_string()))
            .unwrap_or_default()
            .into_iter()
            .filter(|section| section.heading == super::EXAMPLES_HEADING)
            .flat_map(|section| section.entries)
            .collect::<Vec<_>>();

        let is_leaf = !command.has_subcommands();
        assert!(
            !is_leaf || !examples.is_empty(),
            "`cad-cli {}` has no examples",
            path.join(" ")
        );
        for (example, description) in &examples {
            assert!(!description.is_empty(), "`{example}` has no description");
            assert_invokes(example, path);
            *checked += 1;
        }

        for subcommand in command.get_subcommands() {
            if subcommand.get_name() == "help" {
                continue;
            }
            path.push(subcommand.get_name().to_owned());
            visit(subcommand, path, checked);
            path.pop();
        }
    }

    /// Parses the `cad-cli` invocation in `example` and checks that it runs the
    /// command found at `path` or one of its subcommands.
    ///
    /// A shell pipeline feeding the CLI is allowed: only the words after the
    /// last `|` and before the first redirection are the CLI invocation.
    fn assert_invokes(example: &str, path: &[String]) {
        let words = shlex::split(example).unwrap_or_else(|| panic!("`{example}` is not shell"));
        let start = words
            .iter()
            .rposition(|word| word == "|")
            .map_or(0, |pipe| pipe + 1);
        let invocation = words[start..]
            .iter()
            .take_while(|word| !word.starts_with(['<', '>']))
            .collect::<Vec<_>>();
        assert_eq!(
            invocation.first().map(|word| word.as_str()),
            Some("cad-cli"),
            "`{example}` must invoke cad-cli"
        );

        let matches = Cli::command()
            .try_get_matches_from(&invocation)
            .unwrap_or_else(|error| panic!("`{example}` does not parse:\n{error}"));
        let mut invoked = Vec::new();
        let mut current = &matches;
        while let Some((name, next)) = current.subcommand() {
            invoked.push(name.to_owned());
            current = next;
        }
        assert!(
            invoked.starts_with(path),
            "`{example}` is documented under `cad-cli {}` but runs `cad-cli {}`",
            path.join(" "),
            invoked.join(" ")
        );
    }
}
