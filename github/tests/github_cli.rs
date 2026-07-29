#![cfg(feature = "cli-e2e")]

//! Black-box: run `github write-variables` through `moonlit run` via a file:// ref.

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

fn github_wasm_url() -> String {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/github.wasm")
        .canonicalize()
        .expect("github.wasm fixture exists");
    format!("file://{}", p.display())
}

#[test]
fn moonlit_run_drives_github_write_variables() {
    let dir = tempdir().unwrap();
    let out_file = dir.path().join("gh_output");
    fs::write(&out_file, "").unwrap();

    let yaml = format!(
        "name: demo\n\
         plugins:\n\
         \x20 - name: github\n\
         \x20   url: {url}\n\
         \x20   config:\n\
         \x20     token: dummy\n\
         \x20   permissions:\n\
         \x20     exec: [sh]\n\
         \x20     env: [\"*\"]\n\
         \x20     filesystem: read-write\n\
         stages:\n\
         \x20 release:\n\
         \x20   - name: vars\n\
         \x20     run: github.write-variables\n\
         \x20     config:\n\
         \x20       output:\n\
         \x20         version: 1.2.3\n",
        url = github_wasm_url()
    );
    fs::write(dir.path().join("release.yml"), &yaml).unwrap();

    moonlit()
        .args(["run", "--output", "plain", "-w"])
        .arg(dir.path())
        .env("GITHUB_OUTPUT", &out_file)
        .assert()
        .success();

    let written = fs::read_to_string(&out_file).unwrap();
    assert!(written.contains("version=1.2.3\n"), "got: {written}");
}

/// Regression: `plugin inspect` must describe a plugin whose config validation
/// rejects the empty config. It used to `init({})`, so `PluginConfig::validate`
/// failed with a token error. github requires a token, so it is the guard here.
const REQUIRED_CONFIG_FIXTURE: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/github.wasm");

#[test]
fn inspect_succeeds_for_required_config_plugin() {
    moonlit()
        .args(["plugin", "inspect", REQUIRED_CONFIG_FIXTURE])
        .assert()
        .success()
        .stdout(predicates::str::contains("github"))
        .stdout(predicates::str::contains("related-items"))
        .stdout(predicates::str::contains("create-release"))
        .stdout(predicates::str::contains("write-variables"));
}
