use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    // A completion request must stay silent: the shell shows its stderr.
    let filter = if cad_client::completion_requested() {
        tracing_subscriber::EnvFilter::new("off")
    } else {
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"))
    };
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_target(false)
        .init();
    cad_client::run(std::env::args_os()).await
}
