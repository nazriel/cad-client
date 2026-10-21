use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use cad_api::ids::CompanyId;
use clap::{Args, ValueEnum, ValueHint};

/// Options that apply to every command.
///
/// They get a heading of their own so that a subcommand's `--help` keeps its
/// own options apart from the ones every command accepts.
#[derive(Debug, Args)]
#[command(next_help_heading = "Global options")]
pub struct GlobalArgs {
    /// Exact application origin. Has precedence over every preset.
    #[arg(long, global = true, value_hint = ValueHint::Url)]
    pub server_url: Option<String>,

    /// Select the production or staging application origin.
    #[arg(long, value_enum, global = true)]
    pub environment: Option<EnvironmentName>,

    /// Use another accessible company instead of the company from the API-key token.
    #[arg(long, global = true, add = crate::completion::company_ids())]
    pub company_id: Option<CompanyId>,

    /// Permit HTTP for localhost/loopback mock servers.
    #[arg(long, global = true)]
    pub allow_insecure_http: bool,

    /// Total time allowed for one API request, including its response body.
    #[arg(long, value_parser = parse_duration, default_value = "60s", global = true)]
    pub request_timeout: Duration,

    /// Select human-readable or machine-readable output.
    #[arg(long, value_enum, default_value_t = OutputFormat::Table, global = true)]
    pub output: OutputFormat,

    /// Skip confirmation prompts for consequential operations.
    #[arg(long, global = true)]
    pub yes: bool,
}

/// Deployment preset selectable with `--environment`.
///
/// The aliases mirror the ones [`cad_api::Environment`] accepts for `CAD_ENV`,
/// so the flag and the variable never disagree about what `prod` means.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum EnvironmentName {
    #[value(alias = "prod")]
    Production,
    #[value(alias = "stage")]
    Staging,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
    Jsonl,
}

/// A path naming a local file the command must be able to read.
///
/// Its *type*, not the name of the argument carrying it, is what makes
/// [`crate::app`] check the file before any request is sent. A new
/// command inherits that precondition by declaring this type; it cannot forget
/// to name its argument `input` or `file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalInputPath(PathBuf);

impl LocalInputPath {
    /// The conventional stand-in for standard input.
    const STDIN: &'static str = "-";

    /// Whether the path names standard input rather than a file on disk.
    pub(crate) fn is_stdin(&self) -> bool {
        self.0 == Path::new(Self::STDIN)
    }

    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }
}

impl FromStr for LocalInputPath {
    type Err = Infallible;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(Self(PathBuf::from(value)))
    }
}

/// A single positional resource identifier, parsed into its domain newtype.
///
/// Handlers receive the identifier already typed, so passing a reporting-period
/// id where a document id belongs stops compiling instead of reaching the API.
#[derive(Debug, Args)]
pub(crate) struct IdArgs<T>
where
    T: FromStr + Clone + Send + Sync + 'static,
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    /// Identifier of the resource.
    pub(crate) id: T,
}

#[derive(Debug, Args)]
pub(crate) struct InputArgs {
    /// Camel-case JSON input file, or `-` for stdin.
    #[arg(long, value_hint = ValueHint::FilePath)]
    pub(crate) input: LocalInputPath,
}

#[derive(Debug, Args)]
pub(crate) struct IdInputArgs<T>
where
    T: FromStr + Clone + Send + Sync + 'static,
    <T as FromStr>::Err: std::error::Error + Send + Sync + 'static,
{
    /// Identifier of the resource.
    pub(crate) id: T,

    #[command(flatten)]
    pub(crate) input: InputArgs,
}

/// Smallest delay accepted between job status requests.
///
/// Without a floor, `--poll-interval 0` would turn waiting into a tight loop
/// against the platform.
const MIN_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug, Args)]
pub(crate) struct WaitArgs {
    /// Wait until the remote job reaches a terminal state.
    #[arg(long = "wait")]
    pub(crate) enabled: bool,

    /// Delay between job status requests.
    #[arg(long, value_parser = parse_poll_interval, default_value = "1500ms")]
    pub(crate) poll_interval: Duration,

    /// Maximum time to wait for a terminal job state.
    #[arg(long, value_parser = parse_duration, default_value = "30m")]
    pub(crate) timeout: Duration,
}

impl WaitArgs {
    pub(crate) const fn options(&self) -> crate::exec::WaitOptions {
        crate::exec::WaitOptions {
            poll_interval: self.poll_interval,
            timeout: self.timeout,
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct WaitableInputArgs {
    #[command(flatten)]
    pub(crate) input: InputArgs,

    #[command(flatten)]
    pub(crate) wait: WaitArgs,
}

fn parse_poll_interval(raw: &str) -> Result<Duration, String> {
    let value = parse_duration(raw)?;
    if value < MIN_POLL_INTERVAL {
        return Err(format!(
            "poll interval `{raw}` is shorter than the {MIN_POLL_INTERVAL:?} minimum"
        ));
    }
    Ok(value)
}

/// Parses a human duration such as `90s`, `1m30s` or `1500ms`.
///
/// A bare number is now rejected rather than silently read as seconds: the
/// hand-rolled parser it replaces turned `--poll-interval 500` into an eight
/// minute wait instead of the half second the caller meant.
pub(crate) fn parse_duration(raw: &str) -> Result<Duration, String> {
    humantime::parse_duration(raw).map_err(|error| format!("invalid duration `{raw}`: {error}"))
}
