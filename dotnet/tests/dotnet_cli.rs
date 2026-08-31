#![cfg(feature = "cli-e2e")]

//! Black-box: `moonlit run` drives the dotnet plugin's `build` middleware via a file://
//! ref. A missing project fails before any `dotnet` spawn, so the run is deterministic
//! and needs no .NET SDK — asserting the CLI surfaces the frozen failure and exits non-zero.

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
    moonlit_plugin_test_support::component_url("dotnet")
}

#[test]
fn moonlit_run_dotnet_build_missing_project_fails() {
    let dir = tempdir().unwrap();
    let yaml = format!(
        "name: demo\n\
         plugins:\n\
         \x20 - name: dotnet\n\
         \x20   url: {url}\n\
         \x20   permissions:\n\
         \x20     exec: [dotnet]\n\
         \x20     filesystem: read-write\n\
         stages:\n\
         \x20 release:\n\
         \x20   - name: build\n\
         \x20     run: dotnet.build\n\
         \x20     config:\n\
         \x20       project: missing.csproj\n\
         \x20       version: 1.0.0\n",
        url = wasm_url()
    );
    fs::write(dir.path().join("release.yml"), &yaml).unwrap();

    moonlit()
        .args(["run", "--output", "plain", "-w"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("Project file not found at path:"));
}
