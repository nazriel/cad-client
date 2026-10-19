//! Command groups exposed by the CLI.

pub mod benchmark;
pub mod company;
pub mod context;
pub mod custom_field;
pub mod custom_option;
pub mod document;
pub mod generate;
pub mod inventory_line;
pub mod report;
pub mod reporting_period;
pub mod schema;

use clap::Subcommand;
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::app::platform::Platform;
use crate::error::CliError;
use crate::support::terminal::Terminal;

/// Top-level `cad-cli` command.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Read and update the company benchmark.
    Benchmark(benchmark::Group),
    /// Show the authenticated session context.
    Context(context::Group),
    /// Inspect companies available to the current API key.
    Company(company::Group),
    /// Manage reporting periods and duplication jobs.
    ReportingPeriod(reporting_period::Group),
    /// Read and mutate inventory lines.
    InventoryLine(inventory_line::Group),
    /// Upload and download supporting documents.
    Document(document::Group),
    /// Manage custom fields.
    CustomField(custom_field::Group),
    /// Manage options belonging to custom fields.
    CustomOption(custom_option::Group),
    /// Create and inspect generated reports.
    Report(report::Group),
    /// Print a machine-readable schema for a CLI resource.
    Schema(schema::Group),
    /// Print a shell completion script.
    Completions(generate::CompletionsArgs),
    /// Print the manual page in roff format.
    Man,
}

impl Command {
    pub(crate) fn into_invocation(self) -> Invocation {
        match self {
            Self::Benchmark(group) => Invocation::Online(OnlineCommand::Benchmark(group)),
            Self::Context(group) => Invocation::Online(OnlineCommand::Context(group)),
            Self::Company(group) => Invocation::Online(OnlineCommand::Company(group)),
            Self::ReportingPeriod(group) => {
                Invocation::Online(OnlineCommand::ReportingPeriod(group))
            }
            Self::InventoryLine(group) => Invocation::Online(OnlineCommand::InventoryLine(group)),
            Self::Document(group) => Invocation::Online(OnlineCommand::Document(group)),
            Self::CustomField(group) => Invocation::Online(OnlineCommand::CustomField(group)),
            Self::CustomOption(group) => Invocation::Online(OnlineCommand::CustomOption(group)),
            Self::Report(group) => Invocation::Online(OnlineCommand::Report(group)),
            Self::Schema(group) => Invocation::Offline(OfflineCommand::Schema(group)),
            Self::Completions(args) => Invocation::Offline(OfflineCommand::Completions(args)),
            Self::Man => Invocation::Offline(OfflineCommand::Man),
        }
    }
}

/// Whether an invocation needs the platform at all.
///
/// Splitting this out at the type level is what lets the bootstrap dispatch the
/// offline commands before it resolves configuration, without a match arm that
/// can never be taken.
pub(crate) enum Invocation {
    /// Runs without a server, credentials, or any local input file.
    Offline(OfflineCommand),
    Online(OnlineCommand),
}

pub(crate) enum OfflineCommand {
    Schema(schema::Group),
    Completions(generate::CompletionsArgs),
    Man,
}

impl OfflineCommand {
    /// Writes an artefact that is already in its final form, so it bypasses the
    /// `--output` formats entirely.
    pub(crate) fn run(self, terminal: &dyn Terminal) -> Result<(), CliError> {
        match self {
            Self::Schema(group) => group.run(terminal),
            Self::Completions(args) => args.run(terminal),
            Self::Man => generate::man(terminal),
        }
    }
}

/// Every command that needs the platform, session-level and company-scoped
/// alike.
///
/// The two tiers are not separate types: what distinguishes them is which
/// resource the arm asks [`Platform`] for. A group that takes a company-scoped
/// client or an `SdkAdapter` cannot be run without scoping, because scoping is
/// the only way to obtain either.
pub(crate) enum OnlineCommand {
    Benchmark(benchmark::Group),
    Context(context::Group),
    Company(company::Group),
    ReportingPeriod(reporting_period::Group),
    InventoryLine(inventory_line::Group),
    Document(document::Group),
    CustomField(custom_field::Group),
    CustomOption(custom_option::Group),
    Report(report::Group),
}

impl OnlineCommand {
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        platform: &Platform,
    ) -> Result<Value, CliError> {
        match self {
            Self::Benchmark(group) => group.run(context, platform.scoped().await?.client()).await,
            // Session-level commands neither prompt nor read local input, so
            // they ignore the [`CommandContext`].
            Self::Context(group) => {
                group
                    .run(platform.connected(), platform.requested_company_id())
                    .await
            }
            Self::Company(group) => group.run(platform.connected()).await,
            Self::ReportingPeriod(group) => group.run(context, &platform.scoped().await?).await,
            Self::InventoryLine(group) => {
                group.run(context, platform.scoped().await?.client()).await
            }
            Self::Document(group) => {
                let sdk = platform.scoped().await?;
                let company_id = sdk.company_id();
                group.run(context, &sdk, company_id).await
            }
            Self::CustomField(group) => group.run(context, platform.scoped().await?.client()).await,
            Self::CustomOption(group) => {
                group.run(context, platform.scoped().await?.client()).await
            }
            Self::Report(group) => group.run(context, &platform.scoped().await?).await,
        }
    }
}
