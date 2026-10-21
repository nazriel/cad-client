//! Command groups exposed by the CLI.

pub mod activity_category;
pub mod activity_group;
pub mod benchmark;
pub mod company;
pub mod company_template;
pub mod context;
pub mod custom_field;
pub mod custom_option;
pub mod document;
pub mod generate;
pub mod inventory_line;
pub mod organizational_unit;
pub mod report;
pub mod reporting_period;
pub mod schema;
pub mod update;

use std::time::Duration;

use clap::Subcommand;
use serde_json::Value;

use crate::app::context::CommandContext;
use crate::app::platform::Platform;
use crate::cli::help::{self, Example};
use crate::config::Environment;
use crate::error::CliError;
use crate::support::terminal::Terminal;

const COMPLETIONS_EXAMPLES: &[Example] = &[
    Example::new(
        "cad-cli completions zsh > \"${fpath[1]}/_cad-cli\"",
        "Install zsh completions",
    ),
    Example::new(
        "cad-cli completions bash > /usr/local/etc/bash_completion.d/cad-cli",
        "Install bash completions",
    ),
    Example::new(
        "cad-cli completions fish > ~/.config/fish/completions/cad-cli.fish",
        "Install fish completions",
    ),
    Example::new(
        "cad-cli completions zsh",
        "Print the zsh script, e.g. to load it with `source <(cad-cli completions zsh)` in ~/.zshrc",
    ),
];

const MAN_EXAMPLES: &[Example] = &[Example::new(
    "cad-cli man > /usr/local/share/man/man1/cad-cli.1",
    "Install the manual page",
)];

/// Top-level `cad-cli` command.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Manage the company's activity categories.
    ActivityCategory(activity_category::Group),
    /// Manage the company's activity groups.
    ActivityGroup(activity_group::Group),
    /// Read and update the company benchmark.
    Benchmark(benchmark::Group),
    /// Show the authenticated session context.
    Context(context::Group),
    /// Inspect and create companies available to the current API key.
    Company(company::Group),
    /// Inspect reusable company templates available to the current API key.
    CompanyTemplate(company_template::Group),
    /// Manage reporting periods and duplication jobs.
    ReportingPeriod(reporting_period::Group),
    /// Read and mutate inventory lines.
    InventoryLine(inventory_line::Group),
    /// Manage the company's organizational units.
    OrganizationalUnit(organizational_unit::Group),
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
    /// Print a shell completion script that also completes IDs from the platform.
    ///
    /// The script is the registration for dynamic completion and is identical
    /// to the output of `COMPLETE=<shell> cad-cli`: on Tab, the shell asks
    /// `cad-cli` for candidates. Commands, options and values come from the
    /// command definition; `--company-id` and reporting-period IDs come from
    /// the platform, using the same `CAD_*` configuration as any other command.
    /// Platform answers are cached for an hour and are never fetched for longer
    /// than two seconds; without credentials or a reachable server those IDs
    /// are simply not offered. Set `CAD_CLI_COMPLETE_OFFLINE=1` to use only the
    /// cache. Loading the script at shell startup keeps it in step with
    /// upgrades.
    #[command(after_long_help = help::examples(COMPLETIONS_EXAMPLES))]
    Completions(generate::CompletionsArgs),
    /// Print the manual page in roff format.
    #[command(after_long_help = help::examples(MAN_EXAMPLES))]
    Man,
    /// Update cad-cli to the latest release.
    Update(update::UpdateArgs),
}

impl Command {
    /// Whether the background check for a newer release may run alongside
    /// this command.
    ///
    /// The generated artefacts are redirected into files and must stay
    /// byte-for-byte reproducible, and `update` reports on releases itself.
    pub(crate) const fn checks_for_updates(&self) -> bool {
        !matches!(
            self,
            Self::Schema(_) | Self::Completions(_) | Self::Man | Self::Update(_)
        )
    }

    pub(crate) fn into_invocation(self) -> Invocation {
        match self {
            Self::ActivityCategory(group) => {
                Invocation::Online(OnlineCommand::ActivityCategory(group))
            }
            Self::ActivityGroup(group) => Invocation::Online(OnlineCommand::ActivityGroup(group)),
            Self::Benchmark(group) => Invocation::Online(OnlineCommand::Benchmark(group)),
            Self::Context(group) => Invocation::Online(OnlineCommand::Context(group)),
            Self::Company(group) => Invocation::Online(OnlineCommand::Company(group)),
            Self::CompanyTemplate(group) => {
                Invocation::Online(OnlineCommand::CompanyTemplate(group))
            }
            Self::ReportingPeriod(group) => {
                Invocation::Online(OnlineCommand::ReportingPeriod(group))
            }
            Self::InventoryLine(group) => Invocation::Online(OnlineCommand::InventoryLine(group)),
            Self::OrganizationalUnit(group) => {
                Invocation::Online(OnlineCommand::OrganizationalUnit(group))
            }
            Self::Document(group) => Invocation::Online(OnlineCommand::Document(group)),
            Self::CustomField(group) => Invocation::Online(OnlineCommand::CustomField(group)),
            Self::CustomOption(group) => Invocation::Online(OnlineCommand::CustomOption(group)),
            Self::Report(group) => Invocation::Online(OnlineCommand::Report(group)),
            Self::Schema(group) => Invocation::Offline(OfflineCommand::Schema(group)),
            Self::Completions(args) => Invocation::Offline(OfflineCommand::Completions(args)),
            Self::Man => Invocation::Offline(OfflineCommand::Man),
            Self::Update(args) => Invocation::Standalone(StandaloneCommand::Update(args)),
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
    /// Needs neither the platform nor its configuration, but returns data in
    /// the selected output format like an online command.
    Standalone(StandaloneCommand),
    Online(OnlineCommand),
}

pub(crate) enum StandaloneCommand {
    Update(update::UpdateArgs),
}

impl StandaloneCommand {
    pub(crate) async fn run(
        self,
        context: &CommandContext<'_>,
        environment: &impl Environment,
        request_timeout: Duration,
    ) -> Result<update::Report, CliError> {
        match self {
            Self::Update(args) => args.run(context, environment, request_timeout).await,
        }
    }
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
    ActivityCategory(activity_category::Group),
    ActivityGroup(activity_group::Group),
    Benchmark(benchmark::Group),
    Context(context::Group),
    Company(company::Group),
    CompanyTemplate(company_template::Group),
    ReportingPeriod(reporting_period::Group),
    InventoryLine(inventory_line::Group),
    OrganizationalUnit(organizational_unit::Group),
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
            Self::ActivityCategory(group) => {
                group.run(context, platform.scoped().await?.client()).await
            }
            Self::ActivityGroup(group) => {
                group.run(context, platform.scoped().await?.client()).await
            }
            Self::Benchmark(group) => group.run(context, platform.scoped().await?.client()).await,
            // Session-level commands neither prompt nor read local input, so
            // they ignore the [`CommandContext`].
            Self::Context(group) => {
                group
                    .run(platform.connected(), platform.requested_company_id())
                    .await
            }
            Self::Company(group) => group.run(context, platform.connected()).await,
            Self::CompanyTemplate(group) => group.run(platform.connected()).await,
            Self::ReportingPeriod(group) => group.run(context, &platform.scoped().await?).await,
            Self::InventoryLine(group) => {
                group.run(context, platform.scoped().await?.client()).await
            }
            Self::OrganizationalUnit(group) => {
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
