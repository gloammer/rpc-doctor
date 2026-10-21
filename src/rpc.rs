use std::{
    io::Read,
    time::{Duration, Instant},
};

use alloy_primitives::B256;
use reqwest::blocking::Client;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::types::{Endpoint, Quantity};

const MAX_RESPONSE_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Serialize, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RpcError {
    #[error("request timed out")]
    Timeout,
    #[error("connection or TLS failure; check the endpoint, network and certificates")]
    Transport,
    #[error("HTTP {status}")]
    Http { status: u16 },
    // Provider messages and response bodies can echo secrets. Report the code
    // only; never forward arbitrary provider text into a shareable diagnostic.
    #[error("JSON-RPC error {code}")]
    Rpc { code: i64 },
    #[error("invalid JSON-RPC response: {reason}")]
    Protocol { reason: &'static str },
    #[error("response exceeds the 1 MiB limit")]
    ResponseTooLarge,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Probe<T> {
    Ok { value: T, duration_ms: u128 },
    Error { error: RpcError, duration_ms: u128 },
}

impl<T> Probe<T> {
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> Probe<U> {
        match self {
            Self::Ok { value, duration_ms } => Probe::Ok {
                value: map(value),
                duration_ms,
            },
            Self::Error { error, duration_ms } => Probe::Error { error, duration_ms },
        }
    }

    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Ok { value, .. } => Some(value),
            Self::Error { .. } => None,
        }
    }
}

pub struct RpcClient(Client);

impl RpcClient {
    pub fn new(timeout: Duration) -> Result<Self, reqwest::Error> {
        Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .user_agent(concat!("rpc-doctor/", env!("CARGO_PKG_VERSION")))
            .build()
            .map(Self)
    }

    pub fn chain_id(&self, endpoint: &Endpoint) -> Probe<Quantity> {
        self.probe(endpoint, "eth_chainId", json!([]), quantity)
    }

    pub fn block_number(&self, endpoint: &Endpoint) -> Probe<Quantity> {
        self.probe(endpoint, "eth_blockNumber", json!([]), |value| {
            quantity(value)
        })
    }

    pub fn receipt(&self, endpoint: &Endpoint, hash: B256) -> Probe<Receipt> {
        self.probe(
            endpoint,
            "eth_getTransactionReceipt",
            json!([hash.to_string()]),
            |value| Receipt::parse(value, hash),
        )
    }

    pub fn probe<T>(
        &self,
        endpoint: &Endpoint,
        method: &str,
        params: Value,
        parse: impl FnOnce(Value) -> Result<T, RpcError>,
    ) -> Probe<T> {
        let started = Instant::now();
        let result = self.call(endpoint, method, params).and_then(parse);
        let duration_ms = started.elapsed().as_millis();
        match result {
            Ok(value) => Probe::Ok { value, duration_ms },
            Err(error) => Probe::Error { error, duration_ms },
        }
    }

    fn call(&self, endpoint: &Endpoint, method: &str, params: Value) -> Result<Value, RpcError> {
        let response = self
            .0
            .post(endpoint.url().clone())
            .json(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .send()
            .map_err(request_error)?;
        if !response.status().is_success() {
            return Err(RpcError::Http {
                status: response.status().as_u16(),
            });
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::TimedOut
                    || error
                        .get_ref()
                        .and_then(|source| source.downcast_ref::<reqwest::Error>())
                        .is_some_and(reqwest::Error::is_timeout)
                {
                    RpcError::Timeout
                } else {
                    RpcError::Transport
                }
            })?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(RpcError::ResponseTooLarge);
        }
        let body: Value =
            serde_json::from_slice(&bytes).map_err(|_| protocol("body is not valid JSON"))?;
        response_result(body)
    }
}

fn request_error(error: reqwest::Error) -> RpcError {
    if error.is_timeout() {
        RpcError::Timeout
    } else {
        RpcError::Transport
    }
}

pub fn protocol(reason: &'static str) -> RpcError {
    RpcError::Protocol { reason }
}

fn response_result(mut body: Value) -> Result<Value, RpcError> {
    if body.get("jsonrpc") != Some(&json!("2.0")) || body.get("id") != Some(&json!(1)) {
        return Err(protocol("version or request ID does not match"));
    }
    match (body.get("result"), body.get("error")) {
        (Some(_), None) => Ok(body["result"].take()),
        (None, Some(error)) => {
            let code = error
                .get("code")
                .and_then(Value::as_i64)
                .ok_or_else(|| protocol("error code is missing or invalid"))?;
            if !error.get("message").is_some_and(Value::is_string) {
                return Err(protocol("error message is missing or invalid"));
            }
            Err(RpcError::Rpc { code })
        }
        _ => Err(protocol("expected exactly one of result or error")),
    }
}

fn quantity(value: Value) -> Result<Quantity, RpcError> {
    value
        .as_str()
        .and_then(|value| Quantity::parse(value).ok())
        .ok_or_else(|| protocol("expected a canonical 256-bit hex quantity"))
}

fn field<T: DeserializeOwned>(value: &Value, name: &str) -> Result<T, RpcError> {
    serde_json::from_value(value.get(name).cloned().unwrap_or(Value::Null))
        .map_err(|_| protocol("receipt has a missing or invalid field"))
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Receipt {
    NotFound,
    Included {
        transaction_hash: String,
        block_hash: String,
        block_number: Quantity,
        execution: Execution,
    },
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    Success,
    Reverted,
    UnknownLegacy,
}

impl Receipt {
    fn parse(value: Value, expected: B256) -> Result<Self, RpcError> {
        if value.is_null() {
            return Ok(Self::NotFound);
        }
        let hash = crate::types::transaction_hash(&field::<String>(&value, "transactionHash")?)
            .map_err(|_| protocol("invalid receipt transaction hash"))?;
        if hash != expected {
            return Err(protocol("receipt transaction hash does not match"));
        }
        let block_hash = crate::types::transaction_hash(&field::<String>(&value, "blockHash")?)
            .map_err(|_| protocol("invalid receipt block hash"))?;
        let block_number = quantity(value.get("blockNumber").cloned().unwrap_or(Value::Null))?;
        let execution = match value.get("status") {
            Some(status) if status == "0x1" => Execution::Success,
            Some(status) if status == "0x0" => Execution::Reverted,
            None => {
                crate::types::transaction_hash(&field::<String>(&value, "root")?)
                    .map_err(|_| protocol("receipt requires status or a legacy state root"))?;
                Execution::UnknownLegacy
            }
            _ => return Err(protocol("receipt execution status must be 0x0 or 0x1")),
        };
        Ok(Self::Included {
            transaction_hash: hash.to_string(),
            block_hash: block_hash.to_string(),
            block_number,
            execution,
        })
    }
}
