#![cfg(feature = "cli-e2e")]

//! Black-box: run the semantic-release trio through `moonlit run` via a file:// ref,
//! asserting `moonlit run` drives the analyze -> calculate-version trio end-to-end
//! and exits successfully.

use std::fs;

use assert_cmd::Command;
use tempfile::tempdir;

/// Resolve the `moonlit` CLI binary: `$MOONLIT_BIN` if set, else `moonlit` on PATH.
/// These end-to-end tests drive the published CLI and are gated behind the
/// `cli-e2e` feature so a plain `cargo test` needs no external binary.
fn moonlit() -> Command {
    match std::env::var_os("MOONLIT_BIN") {
        Some(p) => Command::new(p),
        None => Command::new("moonlit"),
    }
}

fn wasm_url() -> String {
    moonlit_plugin_test_support::component_url("semantic-release")
}

#[test]
fn moonlit_run_drives_semantic_release_trio() {
    let dir = tempdir().unwrap();
    let yaml = format!(
        "name: demo\n\
         plugins:\n\
         \x20 - name: semantic-release\n\
         \x20   url: {url}\n\
         stages:\n\
         \x20 release:\n\
         \x20   - name: analyze\n\
         \x20     run: semantic-release.analyze\n\
         \x20     config:\n\
         \x20       commits:\n\
         \x20         - sha: abc1234def\n\
         \x20           date: 2026-01-01T00:00:00Z\n\
         \x20           message: 'feat: add thing'\n\
         \x20   - name: version\n\
         \x20     run: semantic-release.calculate-version\n\
         \x20     config:\n\
         \x20       baseVersion: 1.2.3\n",
        url = wasm_url()
    );
    fs::write(dir.path().join("release.yml"), &yaml).unwrap();

    moonlit()
        .args(["run", "--output", "plain", "-w"])
        .arg(dir.path())
        .assert()
        .success();
}
