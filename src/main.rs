mod client;
mod error;
mod network;
mod protocol;
mod storage;

use std::env;

use error::{AppError, Result};
use protocol::Operation;
use tracing_subscriber::EnvFilter;

const DEFAULT_ADDRESS: &str = "127.0.0.1:7432";
const USAGE: &str =
    "usage: aviary serve [db] [bind] | tui [address] | call <address> <json> | wait <address> <id>";

enum Command {
    Serve {
        database: String,
        address: String,
    },
    Terminal {
        address: String,
    },
    Call {
        address: String,
        operation: Operation,
    },
    Wait {
        address: String,
        id: i64,
    },
}

impl Command {
    fn parse() -> Result<Self> {
        let arguments: Vec<String> = env::args().skip(1).collect();
        let required = |index: usize, message: &str| {
            arguments
                .get(index)
                .cloned()
                .ok_or_else(|| AppError::Usage(message.to_owned()))
        };
        match arguments.first().map(String::as_str) {
            Some("serve") => Ok(Self::Serve {
                database: arguments
                    .get(1)
                    .map_or_else(|| "aviary.sqlite".into(), Clone::clone),
                address: arguments
                    .get(2)
                    .map_or_else(|| DEFAULT_ADDRESS.into(), Clone::clone),
            }),
            Some("tui") => Ok(Self::Terminal {
                address: arguments
                    .get(1)
                    .map_or_else(|| DEFAULT_ADDRESS.into(), Clone::clone),
            }),
            Some("call") => Ok(Self::Call {
                address: required(1, "call requires address and JSON")?,
                operation: serde_json::from_str(&required(2, "call requires address and JSON")?)?,
            }),
            Some("wait") => Ok(Self::Wait {
                address: required(1, "wait requires address and id")?,
                id: required(2, "wait requires address and id")?.parse()?,
            }),
            _ => Err(AppError::Usage(USAGE.into())),
        }
    }

    async fn run(self) -> Result<()> {
        match self {
            Self::Serve { database, address } => network::serve(&database, &address).await,
            Self::Terminal { address } => client::terminal(&address).await,
            Self::Call { address, operation } => {
                let response = network::call(&address, &operation).await?;
                println!("{response}");
                Ok(())
            }
            Self::Wait { address, id } => client::wait_for_answer(&address, id).await,
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

    if let Err(error) = async { Command::parse()?.run().await }.await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
