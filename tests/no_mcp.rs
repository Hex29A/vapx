//! A build without the `mcp` feature (`cargo build --no-default-features`) still knows the
//! subcommand and says why it is missing, instead of failing with a parse error.
#![cfg(not(feature = "mcp"))]

use std::process::Command;

#[test]
fn mcp_subcommand_explains_the_missing_feature() {
    let out = Command::new(env!("CARGO_BIN_EXE_vapx"))
        .args(["mcp", "--transport", "http", "--bind", "127.0.0.1:0"])
        .output()
        .expect("run vapx");
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("without MCP support"), "stderr was: {err}");
}
