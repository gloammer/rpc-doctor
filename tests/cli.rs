use std::{
    process::{Command, Output},
    thread::{self, JoinHandle},
    time::Duration,
};

use serde_json::{Value, json};
use tiny_http::{Header, Response, Server};

const TX: &str = "0x1111111111111111111111111111111111111111111111111111111111111111";
const BLOCK: &str = "0x2222222222222222222222222222222222222222222222222222222222222222";

struct Reply {
    method: &'static str,
    params: Value,
    body: String,
    status: u16,
    delay: Duration,
}

impl Reply {
    fn result(method: &'static str, params: Value, result: Value) -> Self {
        Self {
            method,
            params,
            body: json!({"jsonrpc":"2.0", "id":1, "result":result}).to_string(),
            status: 200,
            delay: Duration::ZERO,
        }
    }
}

struct Fixture {
    url: String,
    handle: JoinHandle<()>,
}

impl Fixture {
    fn new(replies: Vec<Reply>) -> Self {
        let server = Server::http("127.0.0.1:0").unwrap();
        let url = format!("http://{}", server.server_addr());
        let handle = thread::spawn(move || {
            for reply in replies {
                let mut request = server
                    .recv_timeout(Duration::from_secs(5))
                    .unwrap()
                    .expect("CLI did not make the expected request");
                assert_eq!(request.method().as_str(), "POST");
                let mut body = String::new();
                request.as_reader().read_to_string(&mut body).unwrap();
                let body: Value = serde_json::from_str(&body).unwrap();
                assert_eq!(body["jsonrpc"], "2.0");
                assert_eq!(body["id"], 1);
                assert_eq!(body["method"], reply.method);
                assert_eq!(body["params"], reply.params);
                thread::sleep(reply.delay);
                let response = Response::from_string(reply.body)
                    .with_status_code(reply.status)
                    .with_header(Header::from_bytes("Content-Type", "application/json").unwrap());
                // A timeout may close the client connection before we respond.
                let _ = request.respond(response);
            }
        });
        Self { url, handle }
    }

    fn finish(self) {
        self.handle.join().unwrap();
    }
}

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rpc-doctor"));
    for name in [
        "RPC_URL",
        "RPC_OTHER_URL",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        command.env_remove(name);
    }
    command
}

fn json_output(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
        panic!(
            "stdout: {}; stderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn eth_check(chain: &str, block: &str) -> Vec<Reply> {
    vec![
        Reply::result("eth_chainId", json!([]), json!(chain)),
        Reply::result("eth_blockNumber", json!([]), json!(block)),
    ]
}

fn genesis(byte: u8) -> String {
    bs58::encode([byte; 32]).into_string()
}
fn signature() -> String {
    bs58::encode([3_u8; 64]).into_string()
}

fn sol_check(hash: &str, slot: u64) -> Vec<Reply> {
    vec![
        Reply::result("getGenesisHash", json!([]), json!(hash)),
        Reply::result("getSlot", json!([{"commitment":"confirmed"}]), json!(slot)),
    ]
}

fn eth_transaction(receipt: Value) -> (Output, Value) {
    let fixture = Fixture::new(vec![
        Reply::result("eth_chainId", json!([]), json!("0x1")),
        Reply::result("eth_getTransactionReceipt", json!([TX]), receipt),
    ]);
    let output = cli()
        .args(["--json", "transaction", TX, "--rpc-url", &fixture.url])
        .output()
        .unwrap();
    fixture.finish();
    let report = json_output(&output);
    (output, report)
}

fn sol_transaction(status: Value) -> (Output, Value) {
    let signature = signature();
    let fixture = Fixture::new(vec![
        Reply::result("getGenesisHash", json!([]), json!(genesis(1))),
        Reply::result(
            "getSignatureStatuses",
            json!([[signature], {"searchTransactionHistory":true}]),
            json!({"context":{"slot":123},"value":[status]}),
        ),
    ]);
    let output = cli()
        .args([
            "--chain",
            "solana",
            "--json",
            "transaction",
            &signature,
            "--rpc-url",
            &fixture.url,
        ])
        .output()
        .unwrap();
    fixture.finish();
    let report = json_output(&output);
    (output, report)
}

#[test]
fn ethereum_check_preserves_integer_precision_and_supports_env() {
    let fixture = Fixture::new(eth_check("0x1", "0x20000000000001"));
    let output = cli()
        .env("RPC_URL", &fixture.url)
        .args(["check", "--expected-network", "1", "--json"])
        .output()
        .unwrap();
    fixture.finish();
    assert!(output.status.success());
    let report = json_output(&output);
    assert_eq!(report["network"]["value"]["chain_id"], "1");
    assert_eq!(report["height"]["value"], "9007199254740993");
    assert_eq!(report["chain_matches"], true);
    assert!(report["height"]["duration_ms"].is_number());
}

#[test]
fn expected_network_mismatch_fails_without_hiding_report() {
    let fixture = Fixture::new(eth_check("0x89", "0x10"));
    let output = cli()
        .args([
            "--json",
            "check",
            "--rpc-url",
            &fixture.url,
            "--expected-network",
            "1",
        ])
        .output()
        .unwrap();
    fixture.finish();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(json_output(&output)["chain_matches"], false);
}

#[test]
fn compare_same_network_reports_absolute_difference() {
    let first = Fixture::new(eth_check("0x1", "0x10"));
    let second = Fixture::new(eth_check("0x1", "0x14"));
    let output = cli()
        .args([
            "compare",
            "--rpc-url",
            &first.url,
            "--other-url",
            &second.url,
            "--json",
        ])
        .output()
        .unwrap();
    first.finish();
    second.finish();
    assert!(output.status.success());
    let report = json_output(&output);
    assert_eq!(report["same_network"], true);
    assert_eq!(report["height_difference"], "4");
}

#[test]
fn compare_different_networks_never_compares_heights() {
    let first = Fixture::new(eth_check("0x1", "0x10"));
    let second = Fixture::new(eth_check("0x89", "0x14"));
    let output = cli()
        .args([
            "--json",
            "compare",
            "--rpc-url",
            &first.url,
            "--other-url",
            &second.url,
        ])
        .output()
        .unwrap();
    first.finish();
    second.finish();
    assert!(!output.status.success());
    let report = json_output(&output);
    assert_eq!(report["same_network"], false);
    assert!(report["height_difference"].is_null());
}

#[test]
fn solana_check_uses_genesis_identity_and_confirmed_slots() {
    let hash = genesis(1);
    let fixture = Fixture::new(sol_check(&hash, 400_000_001));
    let output = cli()
        .args([
            "--chain",
            "solana",
            "--json",
            "check",
            "--rpc-url",
            &fixture.url,
            "--expected-network",
            &hash,
        ])
        .output()
        .unwrap();
    fixture.finish();
    assert!(output.status.success());
    let report = json_output(&output);
    assert_eq!(report["network"]["value"]["genesis_hash"], hash);
    assert_eq!(report["height"]["value"], "400000001");
    assert_eq!(report["height_kind"], "confirmed_slot");
    assert_eq!(report["chain_matches"], true);
}

#[test]
fn solana_compare_checks_genesis_hashes() {
    let first = Fixture::new(sol_check(&genesis(1), 100));
    let second = Fixture::new(sol_check(&genesis(2), 110));
    let output = cli()
        .args([
            "--chain",
            "solana",
            "--json",
            "compare",
            "--rpc-url",
            &first.url,
            "--other-url",
            &second.url,
        ])
        .output()
        .unwrap();
    first.finish();
    second.finish();
    assert!(!output.status.success());
    assert_eq!(json_output(&output)["same_network"], false);
    assert!(json_output(&output)["height_difference"].is_null());
}

#[test]
fn missing_ethereum_receipt_is_not_reported_as_pending() {
    let (output, report) = eth_transaction(Value::Null);
    assert!(output.status.success());
    assert_eq!(
        report["transaction"]["value"]["status"]["state"],
        "not_found"
    );
}

#[test]
fn ethereum_success_revert_and_legacy_receipts_are_distinct() {
    for (status, expected) in [
        (Some("0x1"), "success"),
        (Some("0x0"), "reverted"),
        (None, "unknown_legacy"),
    ] {
        let mut receipt = json!({"transactionHash":TX,"blockHash":BLOCK,"blockNumber":"0x20"});
        match status {
            Some(status) => receipt["status"] = json!(status),
            None => receipt["root"] = json!(BLOCK),
        }
        let (output, report) = eth_transaction(receipt);
        assert!(output.status.success());
        assert_eq!(
            report["transaction"]["value"]["status"]["execution"],
            expected
        );
    }
}

#[test]
fn invalid_receipts_are_rejected() {
    for receipt in [
        json!({}),
        json!({"transactionHash":BLOCK,"blockHash":BLOCK,"blockNumber":"0x1","status":"0x1"}),
        json!({"transactionHash":TX,"blockHash":BLOCK,"blockNumber":"0x1","status":"0x2"}),
    ] {
        let (output, report) = eth_transaction(receipt);
        assert!(!output.status.success());
        assert_eq!(report["transaction"]["error"]["kind"], "protocol");
    }
}

#[test]
fn solana_status_distinguishes_execution_from_confirmation() {
    for (confirmation, err, expected) in [
        ("processed", Value::Null, "succeeded"),
        (
            "confirmed",
            json!({"InstructionError":[0,"InvalidArgument"]}),
            "failed",
        ),
        ("finalized", Value::Null, "succeeded"),
    ] {
        let (output, report) =
            sol_transaction(json!({"slot":100,"err":err,"confirmationStatus":confirmation}));
        assert!(output.status.success());
        let status = &report["transaction"]["value"]["status"];
        assert_eq!(status["state"], "observed");
        assert_eq!(status["execution"], expected);
        assert_eq!(status["confirmation"], confirmation);
    }
}

#[test]
fn missing_and_legacy_solana_status_do_not_claim_confirmation() {
    let (output, report) = sol_transaction(Value::Null);
    assert!(output.status.success());
    assert_eq!(
        report["transaction"]["value"]["status"]["state"],
        "not_found"
    );
    let (output, report) =
        sol_transaction(json!({"slot":100,"err":null,"confirmationStatus":null}));
    assert!(output.status.success());
    assert_eq!(
        report["transaction"]["value"]["status"]["confirmation"],
        "unknown"
    );
}

#[test]
fn malformed_solana_status_is_an_error() {
    let (output, report) = sol_transaction(json!({"slot":100,"confirmationStatus":"finalized"}));
    assert!(!output.status.success());
    assert_eq!(report["transaction"]["error"]["kind"], "protocol");
}

#[test]
fn rpc_errors_preserve_code_but_do_not_echo_provider_text() {
    let mut replies = eth_check("0x1", "0x10");
    replies[0].body =
        json!({"jsonrpc":"2.0","id":1,"error":{"code":-32005,"message":"secret-api-key"}})
            .to_string();
    let fixture = Fixture::new(replies);
    let url = format!("{}/secret-api-key?token=secret-api-key", fixture.url);
    let output = cli()
        .args(["check", "--rpc-url", &url, "--json"])
        .output()
        .unwrap();
    fixture.finish();
    assert!(!output.status.success());
    let report = json_output(&output);
    assert_eq!(report["network"]["error"]["code"], -32005);
    assert_eq!(report["height"]["status"], "ok");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret-api-key"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("secret-api-key"));
}

#[test]
fn malformed_envelopes_and_quantities_are_not_healthy() {
    for body in [
        "not json".to_owned(),
        json!({"jsonrpc":"2.0","id":2,"result":"0x1"}).to_string(),
        json!({"jsonrpc":"2.0","id":1,"result":"0x1","error":{"code":1,"message":"bad"}})
            .to_string(),
        json!({"jsonrpc":"2.0","id":1}).to_string(),
        json!({"jsonrpc":"2.0","id":1,"result":"0x01"}).to_string(),
    ] {
        let mut replies = eth_check("0x1", "0x10");
        replies[0].body = body;
        let fixture = Fixture::new(replies);
        let output = cli()
            .args(["--json", "check", "--rpc-url", &fixture.url])
            .output()
            .unwrap();
        fixture.finish();
        assert!(!output.status.success());
        assert_eq!(json_output(&output)["network"]["error"]["kind"], "protocol");
    }
}

#[test]
fn http_failures_and_oversized_bodies_are_classified() {
    for (status, body, kind) in [
        (429, "rate limited".to_owned(), "http"),
        (200, " ".repeat(1024 * 1024 + 1), "response_too_large"),
    ] {
        let mut replies = eth_check("0x1", "0x10");
        replies[0].status = status;
        replies[0].body = body;
        let fixture = Fixture::new(replies);
        let output = cli()
            .args(["--json", "check", "--rpc-url", &fixture.url])
            .output()
            .unwrap();
        fixture.finish();
        assert!(!output.status.success());
        assert_eq!(json_output(&output)["network"]["error"]["kind"], kind);
    }
}

#[test]
fn timeouts_are_bounded_and_reported() {
    let mut replies = eth_check("0x1", "0x10");
    for reply in &mut replies {
        reply.delay = Duration::from_millis(150);
    }
    let fixture = Fixture::new(replies);
    let output = cli()
        .args([
            "--json",
            "--timeout-ms",
            "50",
            "check",
            "--rpc-url",
            &fixture.url,
        ])
        .output()
        .unwrap();
    fixture.finish();
    assert!(!output.status.success());
    let report = json_output(&output);
    assert_eq!(report["network"]["error"]["kind"], "timeout");
    assert_eq!(report["height"]["error"]["kind"], "timeout");
}

#[test]
fn invalid_input_fails_before_network_and_does_not_leak_urls() {
    for args in [
        vec!["check", "--rpc-url", "ws://host/secret-api-key"],
        vec![
            "transaction",
            "not-a-hash",
            "--rpc-url",
            "http://127.0.0.1:1",
        ],
        vec![
            "--chain",
            "solana",
            "transaction",
            TX,
            "--rpc-url",
            "http://127.0.0.1:1",
        ],
        vec![
            "--chain",
            "solana",
            "check",
            "--rpc-url",
            "http://127.0.0.1:1",
            "--expected-network",
            "1",
        ],
    ] {
        let output = cli().args(args).output().unwrap();
        assert!(!output.status.success());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!error.contains("secret-api-key"));
        assert!(!error.contains("connection or TLS failure"));
    }
}

#[test]
fn human_reports_and_command_help_are_usable() {
    let fixture = Fixture::new(sol_check(&genesis(1), 123));
    let output = cli()
        .args(["--chain", "solana", "check", "--rpc-url", &fixture.url])
        .output()
        .unwrap();
    fixture.finish();
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Solana genesis hash"));
    assert!(text.contains("confirmed_slot: 123"));
    let output = cli().args(["--help"]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("solana"));
}
