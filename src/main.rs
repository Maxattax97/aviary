//! Aviary service, command-line client, and operator terminal interface.

mod client;
mod error;
mod network;
mod protocol;
mod startup;
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
        #[arg(long, hide = true)]
        ready: bool,
    },
    /// Open the operator interface.
    #[command(name = "tui")]
    Terminal {
        address: Option<String>,
        /// Start or reuse a local service on an explicit loopback address.
        #[arg(long, conflicts_with = "address")]
        local_address: Option<std::net::SocketAddr>,
    },
    /// Send a JSON operation, or read it from standard input with -.
    Call { address: String, operation: String },
    /// Wait for a decision.
    Wait {
        address: String,
        id: i64,
        #[arg(long, default_value_t = 0)]
        after_event: i64,
    },
    /// Stream change hints and connection state for durable-state reconciliation.
    Watch { address: String },
    /// Wait for a message response.
    WaitMessage { address: String, id: i64 },
}

impl Command {
    async fn run(self) -> Result<()> {
        match self {
            Self::Serve {
                database,
                address,
                ready,
            } => network::serve(&database, &address, ready).await,
            Self::Terminal {
                address,
                local_address,
            } => {
                let address = match address {
                    Some(address) => address,
                    None => {
                        let address = local_address
                            .map_or_else(|| DEFAULT_ADDRESS.to_owned(), |value| value.to_string());
                        startup::ensure_local(&address).await?;
                        address
                    }
                };
                client::terminal(&address).await
            }
            Self::Call { address, operation } => {
                let response = network::call(&address, &parse_operation(&operation)?).await?;
                println!("{response}");
                Ok(())
            }
            Self::Wait {
                address,
                id,
                after_event,
            } => client::wait_for_answer(&address, id, after_event).await,
            Self::Watch { address } => client::watch(&address).await,
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
            matches!(cli.command, Command::Serve { database, address, ready: false } if database == "aviary.sqlite" && address == DEFAULT_ADDRESS)
        );
        let cli =
            Cli::try_parse_from(["aviary", "call", DEFAULT_ADDRESS, "-"]).expect("stdin operation");
        assert!(matches!(cli.command, Command::Call { operation, .. } if operation == "-"));
        assert!(Cli::try_parse_from(["aviary", "wait", DEFAULT_ADDRESS, "bad"]).is_err());
        assert!(Cli::try_parse_from(["aviary", "tui", DEFAULT_ADDRESS, "extra"]).is_err());
        let cli = Cli::try_parse_from(["aviary", "tui"]).expect("automatic local terminal");
        assert!(matches!(
            cli.command,
            Command::Terminal {
                address: None,
                local_address: None
            }
        ));
        assert!(Cli::try_parse_from([
            "aviary",
            "tui",
            DEFAULT_ADDRESS,
            "--local-address",
            DEFAULT_ADDRESS
        ])
        .is_err());
    }
}
