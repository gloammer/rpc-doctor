use std::fmt;

use serde::Serialize;

use crate::{
    chain::{
        Chain, Confirmation, HeightKind, Network, SolanaExecution, SolanaStatus, TransactionId,
        TransactionStatus,
    },
    rpc::{Execution, Probe, Receipt, RpcClient, RpcError},
    types::{Endpoint, Quantity},
};

#[derive(Serialize)]
pub struct Check {
    endpoint: String,
    network: Probe<Network>,
    height: Probe<Quantity>,
    height_kind: HeightKind,
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

    fn details(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "Endpoint: {} (path/query/credentials omitted)",
            self.endpoint
        )?;
        probe_text(f, "Network identity", &self.network)?;
        probe_text(f, self.height_kind.label(), &self.height)?;
        if let Some(expected) = &self.expected_network {
            match self.chain_matches {
                Some(true) => writeln!(f, "PASS Expected network: {expected}; matches")?,
                Some(false) => writeln!(
                    f,
                    "FAIL Expected network: {expected}; MISMATCH. Check your RPC configuration."
                )?,
                None => writeln!(f, "UNKNOWN Expected network: {expected}; could not verify")?,
            }
        }
        Ok(())
    }

    fn request_summary(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let passed = usize::from(self.network.value().is_some())
            + usize::from(self.height.value().is_some());
        if passed == 2 {
            writeln!(f, "Both RPC requests succeeded.")
        } else {
            writeln!(
                f,
                "{passed} of 2 RPC requests succeeded. Review the failed requests above."
            )
        }
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
        Probe::Ok { value, duration_ms } => writeln!(f, "PASS {label}: {value} — {duration_ms} ms"),
        Probe::Error { error, duration_ms } => {
            writeln!(f, "FAIL {label}: {error} — {duration_ms} ms")
        }
    }
}

impl fmt::Display for Check {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.details(f)?;
        writeln!(f)?;
        self.request_summary(f)?;
        writeln!(
            f,
            "Freshness has not been checked against another provider."
        )?;
        writeln!(
            f,
            "A successful request does not establish consistent provider reliability."
        )?;
        Ok(())
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "First endpoint")?;
        self.first.details(f)?;
        writeln!(f, "\nSecond endpoint")?;
        self.second.details(f)?;
        writeln!(f)?;
        match self.same_network {
            Some(false) => writeln!(
                f,
                "FAIL Network mismatch: heights are not compared. Check your RPC configuration."
            )?,
            Some(true) => {
                writeln!(f, "PASS Both endpoints report the same network identity.")?;
                if let Some(difference) = self.height_difference {
                    writeln!(
                        f,
                        "Absolute height difference: {difference} {}.",
                        self.first.height_kind.unit()
                    )?;
                    if difference == Quantity::from(0) {
                        writeln!(
                            f,
                            "Both endpoints returned the same height in these samples."
                        )?;
                    } else {
                        let (lower, higher) =
                            if self.first.height.value() < self.second.height.value() {
                                ("First", "second")
                            } else {
                                ("Second", "first")
                            };
                        writeln!(
                            f,
                            "{lower} endpoint returned a lower height than the {higher}. Repeat the comparison to see whether this persists."
                        )?;
                    }
                } else {
                    writeln!(
                        f,
                        "Height comparison is unavailable because a height request failed."
                    )?;
                }
            }
            None => writeln!(
                f,
                "Could not verify that both endpoints use the same network."
            )?,
        }
        writeln!(f, "{}", self.note)?;
        writeln!(
            f,
            "Freshness relative to the network has not been independently verified."
        )
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
        probe_text(f, "Network identity", &self.network)?;
        probe_text(f, "Transaction lookup", &self.transaction)?;
        writeln!(f)?;
        if self.network.value().is_none() {
            writeln!(
                f,
                "Network identity could not be verified. Check your RPC configuration before relying on this transaction observation."
            )?;
        }
        match &self.transaction {
            Probe::Ok { value, .. } => {
                writeln!(
                    f,
                    "The lookup succeeded; PASS describes the request, not transaction execution."
                )?;
                writeln!(f, "Next step: {}", transaction_guidance(value))?;
            }
            Probe::Error { error, .. } => {
                writeln!(f, "The lookup failed; transaction status is unknown.")?;
                writeln!(f, "Next step: {}", lookup_error_guidance(error))?;
            }
        }
        writeln!(f, "{}", self.note)
    }
}

fn transaction_guidance(status: &TransactionStatus) -> &'static str {
    match status {
        TransactionStatus::Ethereum(receipt) => ethereum_guidance(receipt),
        TransactionStatus::Solana(status) => solana_guidance(status),
    }
}

fn ethereum_guidance(receipt: &Receipt) -> &'static str {
    match receipt {
        Receipt::NotFound => {
            "Verify the network and transaction hash. This provider has no receipt; it cannot tell us whether the transaction is pending, unknown, dropped or outside its retained history."
        }
        Receipt::Included {
            execution: Execution::Success,
            ..
        } => {
            "The provider reports successful execution. If your UI still says pending, check its receipt polling/subscription and the confirmation or finality policy required by your application."
        }
        Receipt::Included {
            execution: Execution::Reverted,
            ..
        } => {
            "Execution reverted. Investigate the transaction's failure reason before retrying; your UI should handle the failed execution rather than wait for success."
        }
        Receipt::Included {
            execution: Execution::UnknownLegacy,
            ..
        } => {
            "This legacy receipt does not report execution success or failure. Investigate the transaction with a tool that supports historical execution; do not infer success from inclusion alone."
        }
    }
}

fn solana_guidance(status: &SolanaStatus) -> &'static str {
    match status {
        SolanaStatus::NotFound => {
            "Verify the cluster and signature. The history lookup returned no status; check another provider with the required history before treating this transaction as pending or failed."
        }
        SolanaStatus::Observed {
            execution: SolanaExecution::Failed,
            ..
        } => {
            "The provider reports failed execution. Inspect the transaction error and your application's failure handling before deciding whether to retry."
        }
        SolanaStatus::Observed {
            confirmation: Confirmation::Processed,
            ..
        } => {
            "The provider has only processed this transaction. Wait for the confirmation level required by your application before marking a payment complete."
        }
        SolanaStatus::Observed {
            confirmation: Confirmation::Confirmed,
            ..
        } => {
            "The provider reports confirmed execution. If your UI still says pending, check its signature polling/subscription and whether your application requires finalized confirmation."
        }
        SolanaStatus::Observed {
            confirmation: Confirmation::Finalized,
            ..
        } => {
            "The provider reports finalized execution. If your UI still says pending, inspect its status updates and payment reconciliation; this observation does not verify the recipient, amount or your business rules."
        }
        SolanaStatus::Observed {
            confirmation: Confirmation::Unknown,
            ..
        } => {
            "Execution was observed, but confirmation status is unavailable. Verify the confirmation level required by your application with a provider that reports it."
        }
    }
}

fn lookup_error_guidance(error: &RpcError) -> &'static str {
    match error {
        RpcError::Timeout => {
            "Check provider responsiveness or increase --timeout-ms, then repeat the lookup. A timeout does not establish transaction status."
        }
        RpcError::Transport => {
            "Check the endpoint, network connection and TLS certificates, then repeat the lookup."
        }
        RpcError::Http { status: 401 | 403 } => {
            "Check your provider credentials and endpoint permissions without sharing the private URL."
        }
        RpcError::Http { status: 429 } => {
            "The provider rate-limited this request. Reduce request frequency or check your provider quota, then retry later."
        }
        RpcError::Rpc { code: -32601 } => {
            "Check --chain and whether this provider supports the requested transaction method."
        }
        RpcError::Http { .. } | RpcError::Rpc { .. } => {
            "Check the reported error code against your provider's documentation, then repeat the lookup when the problem is resolved."
        }
        RpcError::Protocol { .. } => {
            "Check --chain and the provider's response format; a malformed response does not establish transaction status."
        }
        RpcError::ResponseTooLarge => {
            "The response exceeded the diagnostic limit. Use another suitable endpoint or investigate the response size locally."
        }
    }
}
