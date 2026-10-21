# Small future tasks

Choose one item at a time. Sizes estimate focused implementation time after becoming familiar with this code: S = 25–45 minutes; M = 60–90 minutes and may span several sessions. Investigation and Rust learning can take longer.

## Current baseline

- [x] Ethereum/EVM identity and latest-block checks.
- [x] Solana genesis-hash identity and confirmed-slot checks.
- [x] Compare two endpoints of the selected protocol and check network identity before comparing heights.
- [x] Ethereum receipt and Solana signature-status diagnostics.
- [x] Timeouts, structured errors, exact quantities, text/JSON reports and local mock tests.
- [x] Formatting, Clippy and test workflow for GitHub Actions (runs after pushing).

## Next sessions

- [ ] S — Run the tool with a provider you use. Record one useful finding and one confusing part of the output, with credentials removed.
- [ ] S — Follow the README in a fresh checkout. Fix one verified setup problem, or record that it worked without manufacturing a change.
- [ ] S — Improve human-readable height labels while keeping machine-readable JSON stable. Verify both chains' output.
- [ ] S — Show the full Ethereum receipt's block hash in the human transaction report; it already exists in JSON.
- [ ] S — Add an explanatory hint for HTTP 401/403 versus 429 without printing a provider response body.
- [ ] S — Explain JSON-RPC method-not-found (`-32601`) in human output; retain the numeric error in JSON.
- [ ] S — Add the matching-network Solana comparison example to README with synthetic output.
- [ ] S — Add a captured, redacted JSON report under `examples/` and verify it reflects current output.
- [ ] S — Read one relevant Reqwest or Alloy issue. Reproduce it only if relevant to actual use; submit findings only when useful.
- [ ] M — Add optional `--expected-network` to transaction diagnostics, with mismatch behaviour defined before implementation.
- [ ] M — Define and add Solana `--commitment` selection for height checks. Compare both endpoints at the same commitment and report the choice.
- [ ] M — Add configurable repeated samples. Define sample count, spacing, time budget, partial failures and rate-limit handling first.
- [ ] M — Add latency summaries for repeated samples and document exactly what is timed. Do not equate speed with correctness.
- [ ] M — Add an optional block-hash comparison at a common Ethereum height, with explicit reorg limitations.
- [x] M — Add network identity labels for known Ethereum networks and Solana clusters, retaining raw identities in JSON and verifying mappings in primary sources.
- [ ] M — Add Windows/macOS CI only after checking local-server test portability and HTTPS certificate behaviour.
- [ ] M — Add a release workflow producing installable binaries once platform checks pass and a license is selected.

## Later candidates

- [ ] A small React report viewer, driven by real JSON output rather than a second RPC implementation.
- [ ] WebSocket subscription diagnostics with bounded observation times and cancellation.
- [ ] Opt-in monitoring with clear retry/backoff and request-budget policies.
- [ ] Another protocol adapter, chosen from actual user demand.

## Completion evidence

Each issue should explain the user problem, acceptance criteria, relevant test/demo output and what you learned. Use a weekly three-minute English walkthrough to practise explaining tradeoffs. A daily learning session does not require a daily commit.
