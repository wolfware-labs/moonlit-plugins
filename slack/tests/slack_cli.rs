#![cfg(feature = "cli-e2e")]

//! Black-box: `moonlit run` drives the slack plugin's `send-notification` via a file://
//! ref. A blank channel hits the pinned guard, which fails before any HTTP call —
//! deterministic and network-free — asserting the CLI surfaces the frozen failure and
//! exits non-zero.

use std::fs;

use assert_cmd::Command;
use predicates::str::contains;
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
    moonlit_plugin_test_support::component_url("slack")
}

#[test]
fn moonlit_run_slack_blank_channel_fails() {
    let dir = tempdir().unwrap();
    let yaml = format!(
        "name: demo\n\
         plugins:\n\
         \x20 - name: slack\n\
         \x20   url: {url}\n\
         \x20   config:\n\
         \x20     token: xoxb-dummy\n\
         \x20   permissions:\n\
         \x20     network: [slack.com]\n\
         stages:\n\
         \x20 release:\n\
         \x20   - name: notify\n\
         \x20     run: slack.send-notification\n\
         \x20     config:\n\
         \x20       message: hello\n",
        url = wasm_url()
    );
    fs::write(dir.path().join("release.yml"), &yaml).unwrap();

    moonlit()
        .args(["run", "--output", "plain", "-w"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("No Slack channel provided for notification."));
}
