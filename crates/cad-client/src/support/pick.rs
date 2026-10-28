//! `-` where a profile or a company is expected: a list to pick from.
//!
//! The arguments that take `-` parse it into a typed choice, so the commands
//! never see the string. What they share is here: the refusal without a
//! terminal, and the company list.

use cad_api::ids::CompanyId;

use super::profiles::Company;
use super::terminal::Terminal;
use crate::error::CliError;

/// What `-` was given for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Picking {
    /// `use profile -`.
    Profile,
    /// `use company -`.
    CompanyArgument,
    /// `--company-id -`.
    CompanyFlag,
}

impl Picking {
    /// The refusal for a `-` given where no one can answer a list: what to
    /// pass instead.
    fn without_terminal(self) -> &'static str {
        match self {
            Self::Profile => {
                "`-` picks a profile at a terminal, and there is none; name the profile"
            }
            Self::CompanyArgument => {
                "`-` picks a company at a terminal, and there is none; pass its ID or name"
            }
            Self::CompanyFlag => {
                "`-` picks a company at a terminal, and there is none; pass --company-id <ID> or \
                 set CAD_COMPANY_ID"
            }
        }
    }
}

/// Refuses `-` when no one can answer a list, before anything is requested.
///
/// # Errors
///
/// [`CliError::Input`] naming what to pass instead.
pub(crate) fn require_terminal(terminal: &dyn Terminal, picking: Picking) -> Result<(), CliError> {
    if terminal.is_interactive() {
        Ok(())
    } else {
        Err(CliError::Input(picking.without_terminal().to_owned()))
    }
}

/// Asks the person to pick one of `companies`, `<name>  <id>` each, with the
/// cursor on `current` when it is among them.
///
/// # Errors
///
/// [`CliError::Cancelled`] when the person gives up.
///
/// # Panics
///
/// When `companies` is empty: the caller refuses that first.
pub(crate) fn company(
    terminal: &dyn Terminal,
    companies: &[Company],
    current: Option<CompanyId>,
) -> Result<Company, CliError> {
    let options: Vec<_> = companies
        .iter()
        .map(|company| match &company.name {
            Some(name) => format!("{name}  {}", company.id),
            None => company.id.to_string(),
        })
        .collect();
    let start = current
        .and_then(|current| companies.iter().position(|company| company.id == current))
        .unwrap_or(0);
    let index = terminal.select("Company:", &options, start)?;
    Ok(companies[index].clone())
}
