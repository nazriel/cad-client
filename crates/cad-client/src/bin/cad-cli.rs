use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    cad_client::run(std::env::args_os()).await
}
