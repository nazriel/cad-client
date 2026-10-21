//! The release notes shown on stderr after an update.
//!
//! The notes are the Markdown `mise run release:notes` writes into the GitHub
//! release: a `## Changes` section listing the commits since the previous tag,
//! which it links as ``Since [`v0.1.0`](…compare…)``, and a `## Downloads`
//! section. Only the changes are worth showing after an update.

use semver::Version;

use super::Release;

/// The project's GitHub page, for the compare link.
const REPOSITORY_URL: &str = "https://github.com/nazriel/cad-client";
/// Most lines of notes shown. A release that lists more commits than this is
/// better read on its page, which is linked below the notes.
pub(crate) const MAX_NOTE_LINES: usize = 40;

/// A heading and the text below it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReleaseNotes {
    pub(crate) heading: String,
    pub(crate) body: String,
}

/// The notes to show after updating from `previous` to `release`.
///
/// They are the release's own changes, capped at [`MAX_NOTE_LINES`], then a
/// link to the release page, then, when the update skipped at least one
/// release in between, a link comparing `previous` with the new release.
pub(crate) fn after_update(release: &Release, previous: &Version) -> ReleaseNotes {
    let latest = &release.version;
    let mut lines = release.notes.as_deref().map(changes).unwrap_or_default();
    if lines.len() > MAX_NOTE_LINES {
        let hidden = lines.len() - MAX_NOTE_LINES;
        lines.truncate(MAX_NOTE_LINES);
        lines.push(format!("… {hidden} more lines in the release notes"));
    }

    let mut references = Vec::new();
    if let Some(page) = &release.page {
        references.push(format!("Release notes: {}", sanitize(page)));
    }
    if latest > previous && skips_releases(release.notes.as_deref(), previous) {
        references.push(format!(
            "Every change since {previous}: {REPOSITORY_URL}/compare/v{previous}...v{latest}"
        ));
    }
    if !lines.is_empty() && !references.is_empty() {
        lines.push(String::new());
    }
    lines.extend(references);

    ReleaseNotes {
        heading: format!("Changes in cad-cli {latest}"),
        body: lines.join("\n"),
    }
}

/// The lines of the `## Changes` section, without its heading, the
/// `## Downloads` section, or surrounding blank lines.
fn changes(notes: &str) -> Vec<String> {
    let mut lines: Vec<String> = sanitize(notes)
        .lines()
        .map(str::trim_end)
        .take_while(|line| !line.trim().eq_ignore_ascii_case("## downloads"))
        .skip_while(|line| line.trim().is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if lines
        .first()
        .is_some_and(|line| line.trim().eq_ignore_ascii_case("## changes"))
    {
        lines.remove(0);
    }
    let start = lines.iter().position(|line| !line.trim().is_empty());
    let end = lines.iter().rposition(|line| !line.trim().is_empty());
    match (start, end) {
        (Some(start), Some(end)) => lines[start..=end].to_vec(),
        _ => Vec::new(),
    }
}

/// Whether the release came after another release newer than `previous`.
///
/// The notes name the release they follow (``Since [`v0.1.0`]``). When that
/// cannot be read, the compare link is shown anyway: it is one extra line,
/// and missing it would hide changes.
fn skips_releases(notes: Option<&str>, previous: &Version) -> bool {
    let follows = notes.and_then(|notes| {
        let tag = notes.split_once("Since [`")?.1.split_once('`')?.0;
        super::parse_tag(tag).ok()
    });
    follows.is_none_or(|follows| follows != *previous)
}

/// Removes control characters other than line breaks and tabs, so text from
/// the release cannot move the cursor, recolour or retitle the terminal.
fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|character| matches!(character, '\n' | '\t') || !character.is_control())
        .collect()
}
