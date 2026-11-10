use std::{
    io::{self, Write},
    time::Duration,
};

use tokio::{
    io::{AsyncBufReadExt, BufReader},
    time,
};

use crate::{
    error::Result,
    network,
    protocol::{Operation, PendingRequest, RequestList},
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

pub async fn wait_for_answer(address: &str, id: i64) -> Result<()> {
    loop {
        let response = network::call(address, &Operation::Get { id }).await?;
        if !response["answer"].is_null() {
            println!("{response}");
            return Ok(());
        }
        time::sleep(POLL_INTERVAL).await;
    }
}

pub async fn terminal(address: &str) -> Result<()> {
    let mut input = BufReader::new(tokio::io::stdin()).lines();
    let mut interval = time::interval(POLL_INTERVAL);
    let mut displayed: Option<Vec<PendingRequest>> = None;

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let response = network::call(address, &Operation::List).await?;
                let requests: RequestList = serde_json::from_value(response)?;
                if displayed.as_ref() != Some(&requests.requests) {
                    display_requests(&requests.requests)?;
                    displayed = Some(requests.requests);
                }
            }
            line = input.next_line() => {
                match line? {
                    None => return Ok(()),
                    Some(line) if line == "q" => return Ok(()),
                    Some(line) if line.is_empty() => {
                        if let Some((request, index)) = displayed.as_ref().and_then(|requests| requests.first()).and_then(|request| request.request.recommendation.map(|index| (request, index))) {
                            network::call(address, &Operation::Answer { id: request.id, index }).await?;
                        }
                    }
                    Some(_) => {}
                }
            }
        }
    }
}

fn display_requests(requests: &[PendingRequest]) -> Result<()> {
    print!("\x1b[2J\x1b[H");
    if let Some(pending) = requests.first() {
        let request = &pending.request;
        // Treat persisted and remote data as untrusted input to the terminal client.
        request.validate()?;
        println!(
            "Request {} from {}\n{}\n",
            pending.id, request.agent, request.question
        );
        if let Some(recommended) = request.recommendation {
            if let Some(option) = request.options.get(recommended) {
                println!(
                    "1. {} (recommended: {})",
                    option,
                    request.reason.as_deref().unwrap_or("")
                );
            }
            for (index, option) in request.options.iter().enumerate() {
                if index != recommended {
                    println!("   {option} (original index {index})");
                }
            }
            println!("\nPress Enter to confirm recommendation; q then Enter to quit.");
        } else {
            for (index, option) in request.options.iter().enumerate() {
                println!("   {option} (original index {index})");
            }
            println!("\nNo recommendation. This prototype cannot answer this request yet; q then Enter to quit.");
        }
    } else {
        println!("No pending requests. q then Enter to quit.");
    }
    io::stdout().flush()?;
    Ok(())
}
