mod chain;
mod cli;
mod report;
mod rpc;
mod types;

use std::{
    io::{self, Write},
    process::ExitCode,
    time::Duration,
};

use clap::Parser;
use serde::Serialize;

use crate::{
    cli::{Cli, Command},
    report::{Check, Comparison, TransactionReport},
    rpc::RpcClient,
    types::Endpoint,
};

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            if cli.json {
                eprintln!("{}", serde_json::json!({"error": error}));
            } else {
                eprintln!("Error: {error}");
            }
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli) -> Result<bool, &'static str> {
    let client = RpcClient::new(Duration::from_millis(cli.timeout_ms))
        .map_err(|_| "could not initialise the HTTP client")?;
    match &cli.command {
        Command::Check {
            rpc_url,
            expected_network,
        } => {
            let endpoint: Endpoint = rpc_url.parse()?;
            let expected = expected_network
                .as_deref()
                .map(|value| cli.chain.expected_network(value))
                .transpose()?;
            let report = Check::run(&client, &endpoint, cli.chain, expected);
            emit(&report, cli.json)?;
            Ok(report.successful())
        }
        Command::Compare { rpc_url, other_url } => {
            // Validate both URLs before making any requests.
            let first: Endpoint = rpc_url.parse()?;
            let second: Endpoint = other_url.parse()?;
            let report = Comparison::new(
                Check::run(&client, &first, cli.chain, None),
                Check::run(&client, &second, cli.chain, None),
            );
            emit(&report, cli.json)?;
            Ok(report.successful())
        }
        Command::Transaction {
            rpc_url,
            transaction_id,
        } => {
            let endpoint: Endpoint = rpc_url.parse()?;
            let id = cli.chain.transaction_id(transaction_id)?;
            let report = TransactionReport::run(&client, &endpoint, cli.chain, &id);
            emit(&report, cli.json)?;
            Ok(report.successful())
        }
    }
}

fn emit(report: &(impl Serialize + std::fmt::Display), json: bool) -> Result<(), &'static str> {
    let mut stdout = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut stdout, report)
            .map_err(|_| "could not write JSON report")?;
        writeln!(stdout).map_err(|_| "could not write report")
    } else {
        write!(stdout, "{report}").map_err(|_| "could not write report")
    }
}
