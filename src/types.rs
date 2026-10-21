use std::{fmt, str::FromStr};

use alloy_primitives::{B256, U256};
use reqwest::Url;
use serde::{Serialize, Serializer};

/// Own the URL at the boundary. Its Display deliberately omits credentials,
/// paths, query parameters and fragments, which may contain provider API keys.
pub struct Endpoint(Url);

impl FromStr for Endpoint {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let url = Url::parse(value).map_err(|_| "RPC endpoint must be a valid HTTP(S) URL")?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err("RPC endpoint must be a valid HTTP(S) URL");
        }
        Ok(Self(url))
    }
}

impl Endpoint {
    pub fn url(&self) -> &Url {
        &self.0
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.origin().ascii_serialization())
    }
}

/// Ethereum quantities stay exact; JSON output uses decimal strings so JS
/// consumers do not lose precision above Number.MAX_SAFE_INTEGER.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Quantity(U256);

impl Quantity {
    pub fn decimal(value: &str) -> Result<Self, &'static str> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("expected a decimal Ethereum chain ID");
        }
        U256::from_str_radix(value, 10)
            .map(Self)
            .map_err(|_| "expected a decimal Ethereum chain ID within 256 bits")
    }

    pub fn parse(value: &str) -> Result<Self, &'static str> {
        let hex = value
            .strip_prefix("0x")
            .ok_or("quantity requires 0x prefix")?;
        if hex.is_empty()
            || hex.len() > 64
            || (hex.len() > 1 && hex.starts_with('0'))
            || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err("quantity must be a canonical unsigned 256-bit hex value");
        }
        U256::from_str_radix(hex, 16)
            .map(Self)
            .map_err(|_| "invalid quantity")
    }

    pub fn difference(self, other: Self) -> Self {
        Self(if self >= other {
            self.0 - other.0
        } else {
            other.0 - self.0
        })
    }
}

impl From<u64> for Quantity {
    fn from(value: u64) -> Self {
        Self(U256::from(value))
    }
}

impl fmt::Display for Quantity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for Quantity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub fn transaction_hash(value: &str) -> Result<B256, &'static str> {
    if value.len() != 66 || !value.starts_with("0x") {
        return Err("transaction hash must be 0x followed by 64 hex digits");
    }
    value
        .parse()
        .map_err(|_| "transaction hash must be 0x followed by 64 hex digits")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_display_hides_provider_credentials() {
        let endpoint: Endpoint =
            "https://user:password@example.org/v2/api-key?token=secret#fragment"
                .parse()
                .unwrap();
        assert_eq!(endpoint.to_string(), "https://example.org");
    }

    #[test]
    fn quantities_are_canonical_and_preserve_large_integers() {
        for invalid in ["0x", "0x00", "0x01", "1", "0x-1", "0xzz"] {
            assert!(Quantity::parse(invalid).is_err(), "accepted {invalid}");
        }
        let quantity = Quantity::parse("0x20000000000001").unwrap();
        assert_eq!(quantity.to_string(), "9007199254740993");
        assert_eq!(
            serde_json::to_string(&quantity).unwrap(),
            "\"9007199254740993\""
        );
    }
}
