//! moonlit plugin driven through the real engine host — offline: an empty
//! `modulePaths` hits the pinned guard and fails before any subprocess spawn,
//! and `publish-plugin` is checked for its sandboxed Cargo.toml version stamp.

use std::sync::Arc;

use moonlit_engine::config::model::{FilesystemAccess, Permissions};
use moonlit_engine::host::{
    HostEventSink, InstanceConfig, LogLevel, PluginInstance, ReleaseContext,
};

fn fixture() -> Vec<u8> {
    moonlit_plugin_test_support::component_bytes("moonlit")
}

struct NullSink;
impl HostEventSink for NullSink {
    fn log(&self, _s: &str, _l: LogLevel, _m: &str) {}
    fn progress(&self, _s: &str, _m: &str) {}
}

fn cfg(workdir: &std::path::Path) -> InstanceConfig {
    InstanceConfig {
        working_directory: workdir.to_path_buf(),
        permissions: Permissions {
            network: vec![],
            exec: vec!["moonlit".to_string()],
            env: vec![],
            filesystem: FilesystemAccess::ReadWrite,
        },
        config_view: serde_json::json!({}),
        env_snapshot: vec![],
    }
}

fn ctx(workdir: &std::path::Path, step: &str) -> ReleaseContext {
    ReleaseContext {
        working_directory: workdir.to_str().unwrap().to_string(),
        step_name: step.to_string(),
    }
}

#[tokio::test]
async fn empty_module_paths_fails_before_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let eng = moonlit_engine::host::test_engine();
    let mut p = PluginInstance::instantiate(&eng, &fixture(), cfg(dir.path()), Arc::new(NullSink))
        .await
        .expect("instantiates");
    p.init(&serde_json::json!({})).await.expect("init ok");
    let r = p
        .execute(
            "run-modules",
            ctx(dir.path(), "modules"),
            &serde_json::json!({ "modulePaths": [] }),
        )
        .await
        .unwrap();
    assert!(!r.successful);
    assert_eq!(
        r.error_message.as_deref(),
        Some("No module paths provided for run-modules.")
    );
}

fn cfg_fs(workdir: &std::path::Path, filesystem: FilesystemAccess) -> InstanceConfig {
    let mut c = cfg(workdir);
    c.permissions.filesystem = filesystem;
    c
}

/// Real WASI filesystem write through the engine preopen. Where `moonlit` is
/// installed the build really runs in the empty temp crate and fails; where it
/// is not, the spawn fails. Either way the step fails after the stamp, and the
/// bump is the assertion.
#[tokio::test]
async fn publish_plugin_stamps_version_through_the_sandbox() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("p")).unwrap();
    std::fs::write(
        dir.path().join("p/Cargo.toml"),
        "[package]\nname = \"p\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let eng = moonlit_engine::host::test_engine();
    let mut p = PluginInstance::instantiate(&eng, &fixture(), cfg(dir.path()), Arc::new(NullSink))
        .await
        .expect("instantiates");
    p.init(&serde_json::json!({})).await.expect("init ok");
    let r = p
        .execute(
            "publish-plugin",
            ctx(dir.path(), "publish"),
            &serde_json::json!({
                "manifestPath": "p",
                "version": "0.2.0",
                "reference": "oci://localhost:1/none/p",
                "skipIfPublished": false
            }),
        )
        .await
        .unwrap();
    assert!(!r.successful, "build must fail in an empty crate");
    let toml = std::fs::read_to_string(dir.path().join("p/Cargo.toml")).unwrap();
    assert!(toml.contains("version = \"0.2.0\""), "{toml}");
}

#[tokio::test]
async fn publish_plugin_without_filesystem_grant_cannot_read_manifest() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("p")).unwrap();
    std::fs::write(
        dir.path().join("p/Cargo.toml"),
        "[package]\nname = \"p\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let eng = moonlit_engine::host::test_engine();
    let mut p = PluginInstance::instantiate(
        &eng,
        &fixture(),
        cfg_fs(dir.path(), FilesystemAccess::None),
        Arc::new(NullSink),
    )
    .await
    .expect("instantiates");
    p.init(&serde_json::json!({})).await.expect("init ok");
    let r = p
        .execute(
            "publish-plugin",
            ctx(dir.path(), "publish"),
            &serde_json::json!({ "manifestPath": "p", "version": "0.2.0", "reference": "oci://h/n/p" }),
        )
        .await
        .unwrap();
    assert!(!r.successful);
    assert!(r
        .error_message
        .unwrap()
        .starts_with("Cannot read p/Cargo.toml"));
}
