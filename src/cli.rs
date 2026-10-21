use clap::{Parser, Subcommand};

use crate::chain::Chain;

#[derive(Parser)]
#[command(version, about = "Read-only RPC diagnostics for Ethereum and Solana")]
pub struct Cli {
    /// Chain protocol; use Ethereum for EVM-compatible endpoints
    #[arg(long, global = true, value_enum, default_value_t = Chain::Ethereum)]
    pub chain: Chain,
    /// Emit a JSON report instead of human-readable text
    #[arg(long, global = true)]
    pub json: bool,

    /// Timeout per HTTP request, in milliseconds
    #[arg(long, global = true, default_value_t = 5000, value_parser = clap::value_parser!(u64).range(1..=300_000))]
    pub timeout_ms: u64,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Check network identity, block height or slot, and request durations
    Check {
        #[arg(long, env = "RPC_URL", hide_env_values = true)]
        rpc_url: String,
        /// Expected decimal chain ID (Ethereum) or genesis hash (Solana)
        #[arg(long)]
        expected_network: Option<String>,
    },
    /// Compare two endpoints; block heights are sampled sequentially
    Compare {
        #[arg(long, env = "RPC_URL", hide_env_values = true)]
        rpc_url: String,
        #[arg(long, env = "RPC_OTHER_URL", hide_env_values = true)]
        other_url: String,
    },
    /// Inspect an Ethereum receipt or a Solana signature status
    #[command(visible_alias = "tx")]
    Transaction {
        /// Ethereum transaction hash or Solana base58 transaction signature
        transaction_id: String,
        #[arg(long, env = "RPC_URL", hide_env_values = true)]
        rpc_url: String,
    },
}
