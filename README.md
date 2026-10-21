# rpc-doctor

A read-only RPC diagnostic CLI for **Ethereum and Solana**, built in Rust.

Check which network an endpoint serves, compare two providers, and investigate a transaction without connecting a wallet.

## Quick start

Install a current stable [Rust toolchain](https://rustup.rs/), then run from this repository:

```sh
cargo run -- --help

# Solana devnet: identify the cluster and read its confirmed slot
cargo run -- --chain solana check --rpc-url https://api.devnet.solana.com

# Ethereum / EVM: use your configured HTTP(S) endpoint
cargo run -- check --rpc-url "$RPC_URL" --expected-network 1
```

The default protocol is `ethereum`. An EVM-compatible chain uses the same protocol; its chain ID distinguishes the network. Solana uses its genesis hash as the network identifier. Public endpoints may enforce rate limits.

Human reports recognise Ethereum Mainnet, Sepolia and Hoodi, plus Solana Mainnet Beta, Devnet and Testnet. Names come from returned chain IDs or known genesis hashes, not endpoint hostnames. Unknown identities remain usable and display their raw identifier with `name unknown`. JSON always retains the raw chain ID or genesis hash. A familiar name is not independent verification of a provider's claims.

The Solana mappings follow the [Solana genesis configuration](https://github.com/solana-labs/solana/blob/master/sdk/src/genesis_config.rs); Ethereum names follow the [Ethereum network documentation](https://ethereum.org/en/developers/docs/networks/). A reset or new network identity needs an explicit mapping update.

Install the binary locally:

```sh
cargo install --path . --locked
rpc-doctor --help
```

## Commands

### Check an endpoint

```sh
rpc-doctor check --rpc-url "$RPC_URL"
rpc-doctor check --rpc-url "$RPC_URL" --expected-network 1
rpc-doctor --chain solana check --rpc-url "$SOLANA_RPC_URL"
rpc-doctor --chain solana check --rpc-url "$SOLANA_RPC_URL" \
  --expected-network "$SOLANA_GENESIS_HASH"
```

Checks network identity and height independently, so a failed method does not hide a working method. `--expected-network` accepts a decimal Ethereum chain ID or a base58 Solana genesis hash.

Human output explains which requests passed and what remains unknown. For example (illustrative measurements):

```text
Endpoint: https://api.devnet.solana.com (path/query/credentials omitted)
PASS Network identity: Solana Devnet — 234 ms
PASS Latest confirmed slot: 513469360 — 82 ms

Both RPC requests succeeded.
Freshness has not been checked against another provider.
A successful request does not establish consistent provider reliability.
```

`PASS` refers to a particular request or identity comparison. It is not a provider-health score. A successful pair of requests can still report an expected-network mismatch. JSON field names such as `confirmed_slot` remain stable even though the human labels use plain language.

| Protocol | Network identity | Height |
|---|---|---|
| Ethereum / EVM | `eth_chainId` | `eth_blockNumber`: latest block |
| Solana | `getGenesisHash` | `getSlot` at `confirmed` commitment: slot |

### Compare providers

```sh
rpc-doctor compare --rpc-url "$RPC_URL" --other-url "$RPC_OTHER_URL"
rpc-doctor --chain solana compare \
  --rpc-url "$SOLANA_RPC_A" --other-url "$SOLANA_RPC_B"
```

The selected protocol applies to both endpoints. Heights are compared only when both network identities match. Reports retain both endpoint results if one fails.

Sampling is sequential. A height difference is an observation, not proof of a faulty or lagging provider. Slots and blocks are different measurements. The tool does not compare Ethereum heights with Solana slots.

Human comparisons identify which endpoint returned the lower height and suggest repeating the check. They do not reuse a single-endpoint freshness message or claim that either provider is independently verified.

### Inspect a transaction

```sh
rpc-doctor transaction "$ETH_TRANSACTION_HASH" --rpc-url "$ETH_RPC_URL"
rpc-doctor --chain solana transaction "$SOLANA_SIGNATURE" --rpc-url "$SOLANA_RPC_URL"
```

`tx` is an alias for `transaction`.

- Ethereum calls `eth_getTransactionReceipt`. It distinguishes successful execution, reverted execution, legacy receipts without an execution status, and no receipt at this endpoint.
- Solana calls `getSignatureStatuses` with `searchTransactionHistory: true`. It reports observed execution and the provider's `processed`, `confirmed` or `finalized` status separately. Older responses without a confirmation status are reported as unknown.
- No result does not establish whether a transaction is pending, unknown, dropped, on another network, or outside the provider's retained history.
- Ethereum receipt inclusion alone does not establish finality. No receipt/status is independently verified against consensus.

### JSON and timeouts

```sh
rpc-doctor --json check --rpc-url "$RPC_URL"
rpc-doctor --chain solana --timeout-ms 3000 --json check --rpc-url "$SOLANA_RPC_URL"
```

Global options work before or after the command. The default timeout is 5,000 milliseconds **per HTTP request**, not per command. A check or transaction inspection makes two requests; compare makes four. There are no automatic application retries. Durations include the request, reading the response and validating it; they are individual samples, not benchmark percentiles.

JSON reports go to stdout, with per-method `ok` or `error` results. Startup/input errors go to stderr; `--json` formats those as JSON. Clap's usage errors and help retain their normal text format.

Blockchain quantities are decimal **strings** in JSON to preserve precision in JavaScript. Durations are integer milliseconds. The v0.1 JSON layout may change as the tool evolves.

### Environment variables

`RPC_URL` supplies `--rpc-url`; `RPC_OTHER_URL` supplies `--other-url`. Explicit flags take precedence. The protocol always comes from `--chain`, never from an environment variable or hostname. `.env` files are not loaded automatically.

Environment variables are preferable for provider URLs containing keys. URLs supplied directly on the command line may be recorded by the shell or visible in process listings.

Reports show only endpoint origins (scheme, host, port), omitting URL credentials, path, query and fragment. Origins still reveal the hostname. Raw provider error messages and response bodies are omitted because they can echo credentials; JSON-RPC error codes are retained. Redirects are not followed. HTTP(S) endpoints are supported; WebSockets are future work.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Requested diagnostics completed, including a valid not-found, reverted or failed transaction observation |
| `1` | An RPC/HTTP/transport/protocol error, invalid domain input, output failure, expected-network mismatch, or comparison across different networks |
| `2` | Invalid CLI syntax or missing required arguments |

Exit status describes the diagnostic operation, not whether a transaction succeeded or whether the provider is healthy. Inspect the report for transaction execution state.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Tests use local mock HTTP servers and synthetic transaction identifiers; they do not require RPC credentials, a wallet or live chains. A 1 MiB response-body limit bounds the amount parsed from a provider.

Read [CONTRIBUTING.md](CONTRIBUTING.md) for the code map and workflow, and [BACKLOG.md](BACKLOG.md) for small future tasks.

## Protocol references

- [Ethereum JSON-RPC](https://ethereum.org/en/developers/docs/apis/json-rpc/)
- Solana: [getGenesisHash](https://solana.com/docs/rpc/http/getgenesishash), [getSlot](https://solana.com/docs/rpc/http/getslot), [getSignatureStatuses](https://solana.com/docs/rpc/http/getsignaturestatuses)

All network operations are limited to the read-only methods listed above. The application has no wallet, signing or transaction-submission functionality.
