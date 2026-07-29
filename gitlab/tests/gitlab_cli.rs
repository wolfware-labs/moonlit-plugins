#![cfg(feature = "cli-e2e")]

//! Black-box: run `gitlab write-variables` through `moonlit run` via a file:// ref.

use std::fs;
use std::path::Path;

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

fn gitlab_wasm_url() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gitlab.wasm")
        .canonicalize()
        .expect("gitlab.wasm fixture exists");
    format!("file://{}", p.display())
}

#[test]
fn moonlit_run_drives_gitlab_write_variables() {
    let dir = tempdir().unwrap();
    let yaml = format!(
        "name: demo\n\
         plugins:\n\
         \x20 - name: gitlab\n\
         \x20   url: {url}\n\
         \x20   config:\n\
         \x20     token: dummy\n\
         \x20   permissions:\n\
         \x20     filesystem: read-write\n\
         stages:\n\
         \x20 release:\n\
         \x20   - name: vars\n\
         \x20     run: gitlab.write-variables\n\
         \x20     config:\n\
         \x20       output:\n\
         \x20         version: 1.2.3\n",
        url = gitlab_wasm_url()
    );
    fs::write(dir.path().join("release.yml"), &yaml).unwrap();

    moonlit()
        .args(["run", "--output", "plain", "-w"])
        .arg(dir.path())
        .assert()
        .success();

    // write-variables wrote moonlit.env into the working dir (no env var, no exec needed).
    let written = fs::read_to_string(dir.path().join("moonlit.env")).unwrap();
    assert!(written.contains("version=1.2.3\n"), "got: {written}");
}
