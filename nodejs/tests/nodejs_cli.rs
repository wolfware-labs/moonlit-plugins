#![cfg(feature = "cli-e2e")]

//! Black-box: `moonlit run` drives the nodejs plugin's `build` middleware via a file://
//! ref. A directory with no package.json fails before any `npm` spawn, so the run is
//! deterministic and needs no npm/Node — asserting the CLI surfaces the frozen failure
//! and exits non-zero.

use std::fs;
use std::path::Path;

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
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/nodejs.wasm")
        .canonicalize()
        .expect("nodejs.wasm fixture exists");
    format!("file://{}", p.display())
}

#[test]
fn moonlit_run_nodejs_build_missing_package_json_fails() {
    let dir = tempdir().unwrap();
    let yaml = format!(
        "name: demo\n\
         plugins:\n\
         \x20 - name: nodejs\n\
         \x20   url: {url}\n\
         \x20   permissions:\n\
         \x20     exec: [npm, node]\n\
         \x20     filesystem: read-write\n\
         stages:\n\
         \x20 release:\n\
         \x20   - name: build\n\
         \x20     run: nodejs.build\n\
         \x20     config:\n\
         \x20       command: build\n",
        url = wasm_url()
    );
    fs::write(dir.path().join("release.yml"), &yaml).unwrap();

    moonlit()
        .args(["run", "--output", "plain", "-w"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("package.json not found in directory:"));
}
