use std::fmt;

use serde::Serialize;

use crate::{
    chain::{Chain, Network, TransactionId, TransactionStatus},
    rpc::{Execution, Probe, Receipt, RpcClient},
    types::{Endpoint, Quantity},
};

#[derive(Serialize)]
pub struct Check {
    endpoint: String,
    network: Probe<Network>,
    height: Probe<Quantity>,
    height_kind: &'static str,
    expected_network: Option<Network>,
    chain_matches: Option<bool>,
}

impl Check {
    pub fn run(
        client: &RpcClient,
        endpoint: &Endpoint,
        chain: Chain,
        expected_network: Option<Network>,
    ) -> Self {
        let network = chain.network(client, endpoint);
        let height = chain.height(client, endpoint);
        let chain_matches = network
            .value()
            .zip(expected_network.as_ref())
            .map(|(actual, expected)| actual == expected);
        Self {
            endpoint: endpoint.to_string(),
            network,
            height,
            height_kind: chain.height_kind(),
            expected_network,
            chain_matches,
        }
    }

    pub fn successful(&self) -> bool {
        self.network.value().is_some()
            && self.height.value().is_some()
            && self.chain_matches != Some(false)
    }
}

#[derive(Serialize)]
pub struct Comparison {
    first: Check,
    second: Check,
    same_network: Option<bool>,
    height_difference: Option<Quantity>,
    note: &'static str,
}

impl Comparison {
    pub fn new(first: Check, second: Check) -> Self {
        let same_network = first
            .network
            .value()
            .zip(second.network.value())
            .map(|(first, second)| first == second);
        let height_difference = (same_network == Some(true))
            .then(|| {
                first
                    .height
                    .value()
                    .zip(second.height.value())
                    .map(|(first, second)| first.difference(*second))
            })
            .flatten();
        Self {
            first,
            second,
            same_network,
            height_difference,
            note: "Sequential, single samples: height differences do not prove lag, correctness or provider health. Solana heights are confirmed slots, not blocks.",
        }
    }

    pub fn successful(&self) -> bool {
        self.first.successful() && self.second.successful() && self.same_network == Some(true)
    }
}

#[derive(Serialize)]
pub struct TransactionReport {
    endpoint: String,
    transaction_id: String,
    network: Probe<Network>,
    transaction: Probe<TransactionStatus>,
    note: &'static str,
}

impl TransactionReport {
    pub fn run(client: &RpcClient, endpoint: &Endpoint, chain: Chain, id: &TransactionId) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            transaction_id: id.to_string(),
            network: chain.network(client, endpoint),
            transaction: id.status(client, endpoint),
            note: "Not found does not distinguish pending, unknown, dropped or unavailable history. Ethereum inclusion is not finality; Solana confirmation is provider-reported.",
        }
    }

    pub fn successful(&self) -> bool {
        self.network.value().is_some() && self.transaction.value().is_some()
    }
}

fn probe_text<T: fmt::Display>(
    f: &mut fmt::Formatter<'_>,
    label: &str,
    probe: &Probe<T>,
) -> fmt::Result {
    match probe {
        Probe::Ok { value, duration_ms } => writeln!(f, "  {label}: {value} ({duration_ms} ms)"),
        Probe::Error { error, duration_ms } => {
            writeln!(f, "  {label}: ERROR — {error} ({duration_ms} ms)")
        }
    }
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Endpoint: {} (path/query/credentials omitted)",
            self.endpoint
        )?;
        probe_text(f, "Network", &self.network)?;
        probe_text(f, self.height_kind, &self.height)?;
        if let Some(expected) = &self.expected_network {
            writeln!(
                f,
                "  Expected network: {expected}; {}",
                match self.chain_matches {
                    Some(true) => "matches",
                    Some(false) => "MISMATCH",
                    None => "could not verify",
                }
            )?;
        }
        Ok(())
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "First\n{}\nSecond\n{}", self.first, self.second)?;
        match self.same_network {
            Some(false) => writeln!(f, "Network mismatch: heights are not compared.")?,
            Some(true) => {
                if let Some(difference) = self.height_difference {
                    writeln!(f, "Absolute height difference: {difference}")?;
                }
            }
            None => writeln!(
                f,
                "Could not verify that both endpoints use the same network."
            )?,
        }
        writeln!(f, "{}", self.note)
    }
}

impl fmt::Display for Receipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "not found at this endpoint"),
            Self::Included {
                block_number,
                execution,
                ..
            } => write!(
                f,
                "included in block {block_number}; {}",
                match execution {
                    Execution::Success => "execution succeeded",
                    Execution::Reverted => "execution reverted",
                    Execution::UnknownLegacy => "legacy receipt: execution status unavailable",
                }
            ),
        }
    }
}

impl fmt::Display for TransactionReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Endpoint: {} (path/query/credentials omitted)",
            self.endpoint
        )?;
        writeln!(f, "Transaction: {}", self.transaction_id)?;
        probe_text(f, "Network", &self.network)?;
        probe_text(f, "Transaction", &self.transaction)?;
        writeln!(f, "{}", self.note)
    }
}
