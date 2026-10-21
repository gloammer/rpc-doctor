use std::fmt;

use alloy_primitives::B256;
use clap::ValueEnum;
use serde::Serialize;
use serde_json::{Value, json};

use crate::{
    rpc::{Probe, Receipt, RpcClient, RpcError, protocol},
    types::{Endpoint, Quantity, transaction_hash},
};

#[derive(Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Chain {
    Ethereum,
    Solana,
}

#[derive(PartialEq, Eq, Serialize)]
#[serde(tag = "chain", rename_all = "snake_case")]
pub enum Network {
    Ethereum { chain_id: Quantity },
    Solana { genesis_hash: String },
}

impl Network {
    /// Recognise returned identifiers, never provider hostnames. Unlisted IDs
    /// remain valid networks and keep their raw identity in the display.
    pub fn name(&self) -> Option<&'static str> {
        match self {
            Self::Ethereum { chain_id } => {
                // https://ethereum.org/en/developers/docs/networks/
                [
                    (1, "Ethereum Mainnet"),
                    (11_155_111, "Ethereum Sepolia"),
                    (560_048, "Ethereum Hoodi"),
                ]
                .into_iter()
                .find_map(|(id, name)| (*chain_id == Quantity::from(id)).then_some(name))
            }
            Self::Solana { genesis_hash } => match genesis_hash.as_str() {
                // Known cluster identities from Solana's genesis configuration:
                // https://github.com/solana-labs/solana/blob/master/sdk/src/genesis_config.rs
                "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d" => Some("Solana Mainnet Beta"),
                "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG" => Some("Solana Devnet"),
                "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY" => Some("Solana Testnet"),
                _ => None,
            },
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ethereum { chain_id } => match self.name() {
                Some(name) => write!(f, "{name} (chain ID {chain_id})"),
                None => write!(f, "EVM network (chain ID {chain_id}; name unknown)"),
            },
            Self::Solana { genesis_hash } => match self.name() {
                Some(name) => write!(f, "{name}"),
                None => write!(
                    f,
                    "Solana cluster (genesis hash {genesis_hash}; name unknown)"
                ),
            },
        }
    }
}

pub enum TransactionId {
    Ethereum(B256),
    Solana(String),
}

impl fmt::Display for TransactionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ethereum(hash) => write!(f, "{hash}"),
            Self::Solana(signature) => write!(f, "{signature}"),
        }
    }
}

impl Chain {
    pub fn expected_network(self, input: &str) -> Result<Network, &'static str> {
        match self {
            Self::Ethereum => Ok(Network::Ethereum {
                chain_id: Quantity::decimal(input)?,
            }),
            Self::Solana => {
                validate_base58::<32>(input)
                    .map_err(|_| "expected a base58 Solana genesis hash (32 bytes)")?;
                Ok(Network::Solana {
                    genesis_hash: input.to_owned(),
                })
            }
        }
    }

    pub fn transaction_id(self, input: &str) -> Result<TransactionId, &'static str> {
        match self {
            Self::Ethereum => transaction_hash(input).map(TransactionId::Ethereum),
            Self::Solana => {
                validate_base58::<64>(input)
                    .map_err(|_| "expected a base58 Solana transaction signature (64 bytes)")?;
                Ok(TransactionId::Solana(input.to_owned()))
            }
        }
    }

    pub fn network(self, client: &RpcClient, endpoint: &Endpoint) -> Probe<Network> {
        match self {
            Self::Ethereum => client
                .chain_id(endpoint)
                .map(|chain_id| Network::Ethereum { chain_id }),
            Self::Solana => client.probe(endpoint, "getGenesisHash", json!([]), |value| {
                let hash = value
                    .as_str()
                    .ok_or_else(|| protocol("expected a genesis hash string"))?;
                validate_base58::<32>(hash).map_err(|_| protocol("invalid Solana genesis hash"))?;
                Ok(Network::Solana {
                    genesis_hash: hash.to_owned(),
                })
            }),
        }
    }

    pub fn height(self, client: &RpcClient, endpoint: &Endpoint) -> Probe<Quantity> {
        match self {
            Self::Ethereum => client.block_number(endpoint),
            Self::Solana => client.probe(
                endpoint,
                "getSlot",
                json!([{"commitment": "confirmed"}]),
                |value| {
                    value
                        .as_u64()
                        .map(Quantity::from)
                        .ok_or_else(|| protocol("expected an unsigned Solana slot"))
                },
            ),
        }
    }

    pub fn height_kind(self) -> &'static str {
        match self {
            Self::Ethereum => "latest_block",
            Self::Solana => "confirmed_slot",
        }
    }
}

impl TransactionId {
    pub fn status(&self, client: &RpcClient, endpoint: &Endpoint) -> Probe<TransactionStatus> {
        match self {
            Self::Ethereum(hash) => client
                .receipt(endpoint, *hash)
                .map(TransactionStatus::Ethereum),
            Self::Solana(signature) => client
                .probe(
                    endpoint,
                    "getSignatureStatuses",
                    json!([[signature], {"searchTransactionHistory": true}]),
                    SolanaStatus::parse,
                )
                .map(TransactionStatus::Solana),
        }
    }
}

fn validate_base58<const N: usize>(input: &str) -> Result<(), ()> {
    // Bound untrusted CLI/provider input before decoding it.
    if input.is_empty() || input.len() > N * 2 {
        return Err(());
    }
    let mut bytes = [0_u8; N];
    match bs58::decode(input).onto(&mut bytes) {
        Ok(size) if size == N => Ok(()),
        _ => Err(()),
    }
}

#[derive(Serialize)]
#[serde(tag = "chain", content = "status", rename_all = "snake_case")]
pub enum TransactionStatus {
    Ethereum(Receipt),
    Solana(SolanaStatus),
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SolanaStatus {
    NotFound,
    Observed {
        slot: Quantity,
        execution: SolanaExecution,
        confirmation: Confirmation,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolanaExecution {
    Succeeded,
    Failed,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confirmation {
    Processed,
    Confirmed,
    Finalized,
    Unknown,
}

impl SolanaStatus {
    fn parse(value: Value) -> Result<Self, RpcError> {
        let statuses = value
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| protocol("expected a Solana signature status array"))?;
        let [status] = statuses.as_slice() else {
            return Err(protocol("expected exactly one signature status"));
        };
        if status.is_null() {
            return Ok(Self::NotFound);
        }
        let slot = status
            .get("slot")
            .and_then(Value::as_u64)
            .map(Quantity::from)
            .ok_or_else(|| protocol("signature status requires an unsigned slot"))?;
        let execution = match status.get("err") {
            Some(Value::Null) => SolanaExecution::Succeeded,
            Some(Value::Object(_) | Value::String(_)) => SolanaExecution::Failed,
            _ => return Err(protocol("signature status requires a valid err field")),
        };
        let confirmation = match status.get("confirmationStatus") {
            Some(Value::Null) | None => Confirmation::Unknown,
            Some(value) if value == "processed" => Confirmation::Processed,
            Some(value) if value == "confirmed" => Confirmation::Confirmed,
            Some(value) if value == "finalized" => Confirmation::Finalized,
            _ => return Err(protocol("invalid Solana confirmation status")),
        };
        Ok(Self::Observed {
            slot,
            execution,
            confirmation,
        })
    }
}

impl fmt::Display for TransactionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ethereum(receipt) => write!(f, "{receipt}"),
            Self::Solana(status) => write!(f, "{status}"),
        }
    }
}

impl fmt::Display for SolanaStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "not found at this endpoint"),
            Self::Observed {
                slot,
                execution,
                confirmation,
            } => write!(
                f,
                "observed in slot {slot}; execution {}; confirmation {}",
                match execution {
                    SolanaExecution::Succeeded => "succeeded",
                    SolanaExecution::Failed => "failed",
                },
                match confirmation {
                    Confirmation::Processed => "processed",
                    Confirmation::Confirmed => "confirmed",
                    Confirmation::Finalized => "finalized (provider-reported)",
                    Confirmation::Unknown => "unknown",
                }
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_network_names_are_derived_from_identity() {
        for (id, name) in [
            (1, "Ethereum Mainnet"),
            (11_155_111, "Ethereum Sepolia"),
            (560_048, "Ethereum Hoodi"),
        ] {
            let network = Network::Ethereum {
                chain_id: Quantity::from(id),
            };
            assert_eq!(network.name(), Some(name));
            assert!(network.to_string().contains(&format!("chain ID {id}")));
        }
        for (hash, name) in [
            (
                "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d",
                "Solana Mainnet Beta",
            ),
            (
                "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG",
                "Solana Devnet",
            ),
            (
                "4uhcVJyU9pJkvQyS88uRDiswHXSCkY3zQawwpjk2NsNY",
                "Solana Testnet",
            ),
        ] {
            let network = Chain::Solana.expected_network(hash).unwrap();
            assert_eq!(network.name(), Some(name));
            assert_eq!(network.to_string(), name);
            assert_eq!(
                serde_json::to_value(&network).unwrap()["genesis_hash"],
                hash
            );
        }
    }

    #[test]
    fn unknown_networks_keep_identifiers_without_guessing_names() {
        let network = Network::Ethereum {
            chain_id: Quantity::from(31337),
        };
        assert!(network.name().is_none());
        assert_eq!(
            network.to_string(),
            "EVM network (chain ID 31337; name unknown)"
        );
        let hash = bs58::encode([7_u8; 32]).into_string();
        let network = Chain::Solana.expected_network(&hash).unwrap();
        assert!(network.name().is_none());
        assert!(network.to_string().contains(&hash));
        assert!(network.to_string().contains("name unknown"));
    }
}
