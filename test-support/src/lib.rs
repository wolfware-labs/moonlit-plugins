//! Shared helpers for the plugins' CLI end-to-end tests.
//!
//! These tests need a built `.wasm` component to hand the moonlit CLI. The
//! component is a build artifact, so it is not committed - it is read from the
//! workspace's own `target/wasm32-wasip2/release/`, which is what `cargo build
//! --target wasm32-wasip2 --release` produces and what CI already runs before
//! the test step.

use std::path::PathBuf;
use std::process::Command;

/// A `file://` URL to a plugin's built component, building it if absent.
///
/// `lib` is the crate's `[lib] name`, which is what the artifact is named -
/// `slack`, not the package name `moonlit-plugin-slack`.
///
/// Building on demand keeps a bare `cargo test` working for someone who has
/// not run the wasm build yet. When the artifact is already there - CI, and any
/// second run locally - this costs one `stat`.
pub fn component_url(lib: &str) -> String {
    let path = component_path(lib);
    if !path.exists() {
        build(lib);
    }
    let path = path.canonicalize().unwrap_or_else(|e| {
        panic!(
            "{} was not produced at {}: {e}\n\
             build it with: cargo build --target wasm32-wasip2 --release",
            lib,
            component_path(lib).display()
        )
    });
    format!("file://{}", path.display())
}

/// A plugin's built component as bytes, building it if absent.
///
/// The plugin tests used `include_bytes!`, which resolves at compile time and
/// so required the artifact to be committed. Reading at runtime is what lets
/// the component stay a build artifact.
pub fn component_bytes(lib: &str) -> Vec<u8> {
    let path = component_path(lib);
    if !path.exists() {
        build(lib);
    }
    std::fs::read(&path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()))
}

fn component_path(lib: &str) -> PathBuf {
    // CARGO_TARGET_DIR is respected so this still works under a shared target
    // directory; the workspace root is two levels up from a member's manifest.
    let target = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace_root().join("target"));
    // Cargo replaces dashes with underscores in artifact names.
    target
        .join("wasm32-wasip2/release")
        .join(format!("{}.wasm", lib.replace('-', "_")))
}

fn build(lib: &str) {
    // The whole workspace, not `-p`: the artifact is named after the crate's
    // [lib] name while `-p` takes the package name, and the two differ here
    // (slack.wasm comes from moonlit-plugin-slack). Building everything is
    // also what CI does, so this path stays consistent with it.
    let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["build", "--target", "wasm32-wasip2", "--release"])
        .current_dir(workspace_root())
        .status()
        .unwrap_or_else(|e| panic!("could not run cargo to build {lib}: {e}"));
    assert!(
        status.success(),
        "building the wasm components failed while looking for {lib}; \
         is the target installed? rustup target add wasm32-wasip2"
    );
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("test-support has a parent")
        .to_path_buf()
}
