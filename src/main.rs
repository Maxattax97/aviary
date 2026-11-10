mod client;
mod error;
mod network;
mod protocol;
mod storage;

use std::io::{self, Read};

use clap::{Parser, Subcommand};
use error::{AppError, Result};
use protocol::Operation;
use tracing_subscriber::EnvFilter;

const DEFAULT_ADDRESS: &str = "127.0.0.1:7432";
fn parse_operation(argument: &str) -> Result<Operation> {
    if argument != "-" {
        return Ok(serde_json::from_str(argument)?);
    }
    let mut input = Vec::new();
    io::stdin()
        .lock()
        .take(protocol::MAX_MESSAGE_BYTES + 1)
        .read_to_end(&mut input)?;
    if input.len() > protocol::MAX_MESSAGE_BYTES as usize {
        return Err(AppError::InvalidFrame);
    }
    Ok(serde_json::from_slice(&input)?)
}

#[derive(Parser)]
#[command(version, about = "Communication for operators and agents")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Start the communication service.
    Serve {
        #[arg(default_value = "aviary.sqlite")]
        database: String,
        #[arg(default_value = DEFAULT_ADDRESS)]
        address: String,
    },
    /// Open the operator interface.
    #[command(name = "tui")]
    Terminal {
        #[arg(default_value = DEFAULT_ADDRESS)]
        address: String,
    },
    /// Send a JSON operation, or read it from standard input with -.
    Call { address: String, operation: String },
    /// Wait for a decision.
    Wait { address: String, id: i64 },
    /// Wait for a message response.
    WaitMessage { address: String, id: i64 },
}

impl Command {
    async fn run(self) -> Result<()> {
        match self {
            Self::Serve { database, address } => network::serve(&database, &address).await,
            Self::Terminal { address } => client::terminal(&address).await,
            Self::Call { address, operation } => {
                let response = network::call(&address, &parse_operation(&operation)?).await?;
                println!("{response}");
                Ok(())
            }
            Self::Wait { address, id } => client::wait_for_answer(&address, id).await,
            Self::WaitMessage { address, id } => {
                client::wait_for_message_response(&address, id).await
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let log_filter = match EnvFilter::try_from_default_env() {
        Ok(filter) => filter,
        Err(_) => EnvFilter::new("info"),
    };
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .with_writer(std::io::stderr)
        .init();

    if let Err(error) = Cli::parse().command.run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_preserves_positional_commands_and_checks_arguments() {
        let cli = Cli::try_parse_from(["aviary", "serve"]).expect("default service arguments");
        assert!(
            matches!(cli.command, Command::Serve { database, address } if database == "aviary.sqlite" && address == DEFAULT_ADDRESS)
        );
        let cli =
            Cli::try_parse_from(["aviary", "call", DEFAULT_ADDRESS, "-"]).expect("stdin operation");
        assert!(matches!(cli.command, Command::Call { operation, .. } if operation == "-"));
        assert!(Cli::try_parse_from(["aviary", "wait", DEFAULT_ADDRESS, "bad"]).is_err());
        assert!(Cli::try_parse_from(["aviary", "tui", DEFAULT_ADDRESS, "extra"]).is_err());
    }
}
