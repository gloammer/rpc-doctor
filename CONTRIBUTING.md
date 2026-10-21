# Contributing

Start with an actual diagnostic problem, a small reproduction, or a task from [BACKLOG.md](BACKLOG.md). Discuss changes to CLI syntax or report semantics before expanding the interface.

## Code map

| File | Responsibility |
|---|---|
| `src/main.rs` | Validate inputs, run commands, write output, select exit status |
| `src/cli.rs` | Clap commands, global options and environment-variable bindings |
| `src/types.rs` | Validated endpoints, exact quantities and Ethereum hash validation |
| `src/rpc.rs` | Shared HTTP JSON-RPC transport, response validation, probes and Ethereum receipts |
| `src/chain.rs` | Ethereum/Solana method selection, network identity and Solana transaction status |
| `src/report.rs` | Structured reports, comparisons and human-readable output |
| `tests/cli.rs` | CLI tests against local HTTP fixtures, including expected request methods/params |

The transport is blocking because this first version samples a few endpoints sequentially. Async fan-out is a future design decision, not a prerequisite for a small CLI. Alloy supplies Ethereum primitive types; the full SDK is unnecessary for the limited methods currently supported. Solana identifiers use bounded base58 decoding, without adding a wallet SDK.

## Working on a change

1. Write the observed problem and expected behaviour.
2. Make the smallest change that addresses it.
3. Add a meaningful test for changed behaviour, especially protocol edge cases.
4. Update examples when CLI syntax or JSON semantics change.
5. Run the formatting, Clippy and test commands from the README.

Keep domain validation at input boundaries. Preserve exact blockchain integers, use typed errors for expected failures, and avoid panics or unnecessary cloning in application code. Credentials and arbitrary provider text must not leak into shareable reports. New methods must remain read-only.

With AI assistance, understand the changed code and verify the protocol documentation and tests yourself. Describe the problem, decisions and validation in your own words. Record material AI assistance in your PR description.

Never commit private RPC URLs, `.env` files, keys, or employer/client data. Use synthetic fixtures when reporting a problem.
