#![cfg(feature = "cli-e2e")]

//! Black-box: `moonlit run` drives the docker plugin's `deploy` middleware via a file://
//! ref. Setting `service` hits the swarm-unsupported stub, which fails before any `docker`
//! spawn — deterministic and needs no docker binary — asserting the CLI surfaces the frozen
//! failure and exits non-zero.

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
        .join("tests/fixtures/docker.wasm")
        .canonicalize()
        .expect("docker.wasm fixture exists");
    format!("file://{}", p.display())
}

#[test]
fn moonlit_run_docker_deploy_swarm_unsupported_fails() {
    let dir = tempdir().unwrap();
    let yaml = format!(
        "name: demo\n\
         plugins:\n\
         \x20 - name: docker\n\
         \x20   url: {url}\n\
         \x20   permissions:\n\
         \x20     exec: [docker]\n\
         stages:\n\
         \x20 release:\n\
         \x20   - name: deploy\n\
         \x20     run: docker.deploy\n\
         \x20     config:\n\
         \x20       image: img\n\
         \x20       host: ssh://h\n\
         \x20       service: web\n",
        url = wasm_url()
    );
    fs::write(dir.path().join("release.yml"), &yaml).unwrap();

    moonlit()
        .args(["run", "--output", "plain", "-w"])
        .arg(dir.path())
        .assert()
        .failure()
        .stderr(contains("Swarm deploys are not supported yet."));
}
