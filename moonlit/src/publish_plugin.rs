//! `moonlit publish-plugin` — stamp a version into a plugin crate, build it,
//! check the component reports that version, and push it to an OCI registry.

use std::path::PathBuf;

use moonlit_pdk::prelude::*;

use crate::manifest;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct PublishPluginInput {
    /// Crate directory holding the plugin's Cargo.toml, relative to the working directory. Required.
    pub manifest_path: String,
    /// Version to write into Cargo.toml and publish (MAJOR.MINOR.PATCH[-PRERELEASE]). Required.
    pub version: String,
    /// OCI reference without a tag, e.g. `oci://registry.moonlit.rs/wolfware/git`. Required.
    pub reference: String,
    /// Cargo target directory, relative to the working directory. Defaults to "target".
    pub target_dir: String,
    /// Succeed without pushing when `<reference>:<version>` already exists. Defaults to true.
    pub skip_if_published: bool,
}

impl Default for PublishPluginInput {
    fn default() -> Self {
        Self {
            manifest_path: String::new(),
            version: String::new(),
            reference: String::new(),
            target_dir: "target".to_string(),
            skip_if_published: true,
        }
    }
}

/// Output published by `publish-plugin`.
#[derive(Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublishPluginOutput {
    /// The full reference, `<reference>:<version>`.
    pub reference: String,
    /// Manifest digest reported by the registry. Absent when the push was skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    /// True when the version was already in the registry and nothing was pushed.
    pub already_published: bool,
}

#[derive(Default)]
pub struct PublishPlugin;

/// Where the working directory is visible to this component. Under the engine
/// it is preopened as `.`; native unit tests have no preopen and use the real path.
fn fs_root(ctx: &Context) -> PathBuf {
    if cfg!(target_arch = "wasm32") {
        PathBuf::from(".")
    } else {
        PathBuf::from(ctx.working_dir())
    }
}

fn moonlit<'a>(ctx: &Context<'a>) -> moonlit_pdk::process::Command<'a> {
    ctx.command("moonlit").cwd(ctx.working_dir())
}

/// Result of `moonlit plugin inspect`.
enum Inspected {
    /// The `version` the component reports.
    Version(String),
    /// The command exited non-zero (for a registry reference: absent or unreachable).
    Failed { exit_code: i32, stderr: String },
}

/// Runs `moonlit plugin inspect <target> --output json`. Only a run that exits 0
/// but yields unusable output is an `Err`.
fn inspect(ctx: &Context, target: &str) -> Result<Inspected, String> {
    let out = moonlit(ctx)
        .args(["plugin", "inspect", target, "--output", "json"])
        .run()?;
    if !out.success() {
        return Ok(Inspected::Failed {
            exit_code: out.exit_code,
            stderr: out.stderr().trim().to_string(),
        });
    }
    let v: serde_json::Value = serde_json::from_str(&out.stdout())
        .map_err(|e| format!("Unreadable inspect output for {target}: {e}."))?;
    v["version"]
        .as_str()
        .map(|s| Inspected::Version(s.to_string()))
        .ok_or_else(|| format!("Inspect output for {target} has no version."))
}

impl Middleware for PublishPlugin {
    const NAME: &'static str = "publish-plugin";
    const DESCRIPTION: &'static str =
        "version, build and publish a Moonlit plugin crate to an OCI registry";
    type Input = PublishPluginInput;
    type Output = PublishPluginOutput;

    fn execute(&self, ctx: &Context, input: Self::Input) -> MiddlewareResult<Self::Output> {
        let version = input.version.trim();
        if !manifest::is_semver(version) {
            return MiddlewareResult::failure(format!(
                "'{}' is not a valid version (expected MAJOR.MINOR.PATCH[-PRERELEASE]).",
                input.version
            ));
        }
        let reference = input.reference.trim().trim_end_matches('/');
        if reference.is_empty() {
            return MiddlewareResult::failure("A registry reference is required.");
        }
        let last = reference.rsplit('/').next().unwrap_or(reference);
        if last.contains(':') || last.contains('@') {
            return MiddlewareResult::failure(
                "The reference must not include a tag or digest; the version is appended to it.",
            );
        }
        let manifest_path = input.manifest_path.trim();
        let target_dir = input.target_dir.trim().trim_end_matches('/');
        for (field, p) in [("manifestPath", manifest_path), ("targetDir", target_dir)] {
            if let Err(e) = manifest::check_relative(p).map_err(|e| format!("{field}: {e}")) {
                return MiddlewareResult::failure(e);
            }
        }
        let full_ref = format!("{reference}:{version}");

        // Stamp the version first, and always build, even when the push is
        // skipped below: the pipeline's commit step needs both the bumped
        // Cargo.toml and a Cargo.lock that agrees with it.
        let manifest_file = fs_root(ctx).join(manifest_path).join("Cargo.toml");
        let toml = match std::fs::read_to_string(&manifest_file) {
            Ok(t) => t,
            Err(e) => {
                return MiddlewareResult::failure(format!(
                    "Cannot read {manifest_path}/Cargo.toml: {e}."
                ))
            }
        };
        let updated = match manifest::set_package_version(&toml, version) {
            Ok(u) => u,
            Err(e) => return MiddlewareResult::failure(e),
        };
        let Some(artifact) = manifest::artifact_name(&toml) else {
            return MiddlewareResult::failure(
                "Cannot determine the plugin's artifact name from Cargo.toml.",
            );
        };
        if let Err(e) = std::fs::write(&manifest_file, updated) {
            return MiddlewareResult::failure(format!(
                "Cannot write {manifest_path}/Cargo.toml: {e}."
            ));
        }

        match moonlit(ctx)
            .args([
                "plugin",
                "build",
                "--release",
                "--manifest-path",
                manifest_path,
            ])
            .stream(LineHandler::severity())
        {
            Ok(o) if o.success() => {}
            Ok(o) => {
                return MiddlewareResult::failure(format!(
                    "Plugin build failed with exit code {}.",
                    o.exit_code
                ))
            }
            Err(e) => return MiddlewareResult::failure(e),
        }

        let wasm = format!("{target_dir}/wasm32-wasip2/release/{artifact}.wasm");
        match inspect(ctx, &wasm) {
            Ok(Inspected::Version(built)) if built == version => {}
            Ok(Inspected::Version(built)) => {
                return MiddlewareResult::failure(format!(
                    "Built component reports version {built}, expected {version}."
                ))
            }
            Ok(Inspected::Failed { exit_code, stderr }) => {
                return MiddlewareResult::failure(format!(
                    "Inspecting {wasm} failed (exit code {exit_code}): {stderr}."
                ))
            }
            Err(e) => return MiddlewareResult::failure(e),
        }

        if input.skip_if_published {
            match inspect(ctx, &full_ref) {
                Ok(Inspected::Version(found)) if found == version => {
                    ctx.log_warn(&format!(
                        "{full_ref} is already published; skipping the push. The registry copy \
                         was built from an earlier commit: any commits merged since the failed \
                         release are NOT in it."
                    ));
                    return MiddlewareResult::ok(PublishPluginOutput {
                        reference: full_ref,
                        digest: None,
                        already_published: true,
                    });
                }
                Ok(Inspected::Version(found)) => {
                    return MiddlewareResult::failure(format!(
                        "{full_ref} already exists but reports version {found}."
                    ))
                }
                // A non-zero exit means absent (or unreachable); the push below
                // is the step that reports a real registry problem.
                Ok(Inspected::Failed { exit_code, stderr }) => ctx.log_info(&format!(
                    "{full_ref} not found in the registry (exit code {exit_code}): {stderr}."
                )),
                Err(e) => return MiddlewareResult::failure(e),
            }
        }

        let out = match moonlit(ctx)
            .args([
                "plugin",
                "publish",
                &full_ref,
                "--file",
                &wasm,
                "--manifest-path",
                manifest_path,
                "--output",
                "json",
            ])
            .run()
        {
            Ok(o) => o,
            Err(e) => return MiddlewareResult::failure(e),
        };
        if !out.success() {
            return MiddlewareResult::failure(format!(
                "Publishing {full_ref} failed (exit code {}): {}.",
                out.exit_code,
                out.stderr().trim()
            ));
        }
        let digest = serde_json::from_str::<serde_json::Value>(&out.stdout())
            .ok()
            .and_then(|v| v["digest"].as_str().map(str::to_string));
        if digest.is_none() {
            ctx.log_warn(&format!(
                "{full_ref} was published but its digest could not be read from the publish output."
            ));
        }
        MiddlewareResult::ok(PublishPluginOutput {
            reference: full_ref,
            digest,
            already_published: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonlit_pdk::process::{OutputChunk, StdioStream};
    use moonlit_pdk::testing::{run, MockHost};
    use std::collections::HashMap;

    const TOML: &str =
        "[package]\nname = \"moonlit-plugin-git\"\nversion = \"0.1.0\"\n\n[lib]\nname = \"git\"\n";
    const REF: &str = "oci://registry.moonlit.rs/wolfware/git:0.2.0";
    const WASM: &str = "target/wasm32-wasip2/release/git.wasm";

    fn out(text: &str) -> OutputChunk {
        OutputChunk {
            stream: StdioStream::Stdout,
            text: text.to_string(),
        }
    }
    fn err(text: &str) -> OutputChunk {
        OutputChunk {
            stream: StdioStream::Stderr,
            text: text.to_string(),
        }
    }
    fn workdir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("git")).unwrap();
        std::fs::write(d.path().join("git/Cargo.toml"), TOML).unwrap();
        d
    }
    fn input(version: &str) -> PublishPluginInput {
        PublishPluginInput {
            manifest_path: "git".into(),
            version: version.into(),
            reference: "oci://registry.moonlit.rs/wolfware/git".into(),
            ..Default::default()
        }
    }
    fn outputs(
        r: MiddlewareResult<PublishPluginOutput>,
    ) -> (bool, Option<String>, HashMap<String, String>) {
        let w = r.into_wit();
        (
            w.successful,
            w.error_message,
            w.output.into_iter().collect(),
        )
    }
    fn manifest(d: &tempfile::TempDir) -> String {
        std::fs::read_to_string(d.path().join("git/Cargo.toml")).unwrap()
    }
    fn wd(d: &tempfile::TempDir) -> String {
        d.path().to_str().unwrap().to_string()
    }
    /// Build succeeds and the built component reports 0.2.0.
    fn built() -> MockHost {
        MockHost::new()
            .with_process_result(0, vec![out("Compiling git")])
            .with_process_result(0, vec![out(r#"{"version":"0.2.0"}"#)])
    }

    #[test]
    fn invalid_version_fails_before_any_io() {
        let host = MockHost::new();
        let ctx = Context::new(&host, "/nowhere".into(), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("1.0")));
        assert!(!ok);
        assert!(msg.unwrap().contains("not a valid version"));
        assert!(host.recorded_commands().is_empty());
    }

    #[test]
    fn reference_with_tag_is_rejected() {
        let host = MockHost::new();
        let ctx = Context::new(&host, "/nowhere".into(), "publish".into());
        let mut i = input("0.2.0");
        i.reference = "oci://registry.moonlit.rs/wolfware/git:0.2.0".into();
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, i));
        assert!(!ok);
        assert!(msg.unwrap().contains("must not include a tag"));
    }

    #[test]
    fn reference_with_registry_port_is_allowed_past_validation() {
        let d = workdir();
        let host = built().with_process_result(0, vec![out(r#"{"version":"0.2.0"}"#)]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let mut i = input("0.2.0");
        i.reference = "oci://localhost:5000/wolfware/git".into();
        let (ok, _, m) = outputs(run(&PublishPlugin, &ctx, i));
        assert!(ok);
        assert_eq!(
            m["reference"],
            "\"oci://localhost:5000/wolfware/git:0.2.0\""
        );
    }

    #[test]
    fn escaping_manifest_path_is_rejected() {
        let host = MockHost::new();
        let ctx = Context::new(&host, "/nowhere".into(), "publish".into());
        let mut i = input("0.2.0");
        i.manifest_path = "../git".into();
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, i));
        assert!(!ok);
        let msg = msg.unwrap();
        assert!(
            msg.starts_with("manifestPath: ") && msg.contains("must not leave"),
            "{msg}"
        );
    }

    #[test]
    fn escaping_target_dir_is_rejected() {
        let host = MockHost::new();
        let ctx = Context::new(&host, "/nowhere".into(), "publish".into());
        let mut i = input("0.2.0");
        i.target_dir = "../t".into();
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, i));
        assert!(!ok);
        assert!(msg.unwrap().starts_with("targetDir: "));
    }

    #[test]
    fn workspace_version_fails_without_spawning() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("git")).unwrap();
        std::fs::write(
            d.path().join("git/Cargo.toml"),
            "[package]\nname = \"x\"\nversion.workspace = true\n",
        )
        .unwrap();
        let host = MockHost::new();
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(!ok);
        assert!(msg.unwrap().contains("workspace"));
        assert!(host.recorded_commands().is_empty());
    }

    #[test]
    fn already_published_skips_only_the_push() {
        let d = workdir();
        let host = built().with_process_result(0, vec![out(r#"{"name":"git","version":"0.2.0"}"#)]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, _, m) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(ok);
        assert_eq!(m["alreadyPublished"], "true");
        assert!(!m.contains_key("digest"));
        assert!(
            manifest(&d).contains("version = \"0.2.0\""),
            "bump still written for the commit step"
        );
        let cmds = host.recorded_commands();
        assert_eq!(
            cmds.len(),
            3,
            "build, local inspect, registry inspect; no publish"
        );
        assert_eq!(cmds[0].args[1], "build");
        assert_eq!(
            cmds[2].args,
            vec!["plugin", "inspect", REF, "--output", "json"]
        );
        let logs = host.logs();
        assert!(
            logs.iter()
                .any(|(level, l)| *level == moonlit_pdk::LogLevel::Warn
                    && l.contains("skipping the push")
                    && l.contains("earlier commit")
                    && l.contains("NOT in it")),
            "{logs:?}"
        );
    }

    #[test]
    fn published_version_mismatch_fails() {
        let d = workdir();
        let host = built().with_process_result(0, vec![out(r#"{"version":"0.1.9"}"#)]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(!ok);
        assert!(msg.unwrap().contains("reports version 0.1.9"));
        assert_eq!(host.recorded_commands().len(), 3, "publish never spawned");
    }

    #[test]
    fn builds_verifies_and_publishes() {
        let d = workdir();
        let host = built()
            .with_process_result(3, vec![err("error: manifest unknown")]) // registry inspect: absent
            .with_process_result(
                0,
                vec![out(r#"{"reference":"x","digest":"sha256:abc","size":1}"#)],
            ); // publish
        let wd = wd(&d);
        let ctx = Context::new(&host, wd.clone(), "publish".into());
        let (ok, msg, m) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(ok, "{msg:?}");
        assert_eq!(m["reference"], format!("\"{REF}\""));
        assert_eq!(m["digest"], "\"sha256:abc\"");
        assert_eq!(m["alreadyPublished"], "false");
        let cmds = host.recorded_commands();
        assert_eq!(cmds.len(), 4);
        assert!(cmds
            .iter()
            .all(|c| c.program == "moonlit" && c.cwd.as_deref() == Some(wd.as_str())));
        assert_eq!(
            cmds[0].args,
            vec!["plugin", "build", "--release", "--manifest-path", "git"]
        );
        assert_eq!(
            cmds[1].args,
            vec!["plugin", "inspect", WASM, "--output", "json"]
        );
        assert_eq!(
            cmds[2].args,
            vec!["plugin", "inspect", REF, "--output", "json"]
        );
        assert_eq!(
            cmds[3].args,
            vec![
                "plugin",
                "publish",
                REF,
                "--file",
                WASM,
                "--manifest-path",
                "git",
                "--output",
                "json",
            ]
        );
        let logs = host.logs();
        assert!(
            logs.iter()
                .any(|(_, l)| l.contains("exit code 3") && l.contains("manifest unknown")),
            "{logs:?}"
        );
    }

    #[test]
    fn skip_disabled_does_not_inspect_the_registry() {
        let d = workdir();
        let host = built().with_process_result(0, vec![out(r#"{"digest":"sha256:abc"}"#)]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let mut i = input("0.2.0");
        i.skip_if_published = false;
        let (ok, _, _) = outputs(run(&PublishPlugin, &ctx, i));
        assert!(ok);
        let cmds = host.recorded_commands();
        assert_eq!(cmds.len(), 3);
        assert_eq!(cmds[2].args[1], "publish");
    }

    #[test]
    fn built_version_mismatch_stops_before_registry_and_publish() {
        let d = workdir();
        let host = MockHost::new()
            .with_process_result(0, vec![])
            .with_process_result(0, vec![out(r#"{"version":"0.1.0"}"#)]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(!ok);
        assert!(msg
            .unwrap()
            .contains("Built component reports version 0.1.0"));
        assert_eq!(host.recorded_commands().len(), 2);
    }

    #[test]
    fn build_failure_is_reported() {
        let d = workdir();
        let host = MockHost::new().with_process_result(101, vec![]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(!ok);
        assert_eq!(
            msg.as_deref(),
            Some("Plugin build failed with exit code 101.")
        );
    }

    #[test]
    fn publish_failure_surfaces_stderr() {
        let d = workdir();
        let host = built()
            .with_process_result(3, vec![])
            .with_process_result(3, vec![err("error: unauthorized")]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(!ok);
        let msg = msg.unwrap();
        assert!(
            msg.contains("exit code 3") && msg.contains("unauthorized"),
            "{msg}"
        );
        assert!(msg.ends_with('.'), "{msg}");
    }

    #[test]
    fn registry_inspect_unreadable_output_fails_without_push() {
        let d = workdir();
        let host = built().with_process_result(0, vec![out("not json")]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(!ok);
        let msg = msg.unwrap();
        assert!(
            msg.contains("Unreadable inspect output") && msg.ends_with('.'),
            "{msg}"
        );
        assert_eq!(host.recorded_commands().len(), 3, "publish never spawned");
    }

    #[test]
    fn registry_inspect_without_version_fails_without_push() {
        let d = workdir();
        let host = built().with_process_result(0, vec![out("{}")]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(!ok);
        assert!(msg.unwrap().contains("has no version"));
        assert_eq!(host.recorded_commands().len(), 3, "publish never spawned");
    }

    #[test]
    fn local_inspect_failure_is_hard_and_ends_with_a_period() {
        let d = workdir();
        let host = MockHost::new()
            .with_process_result(0, vec![])
            .with_process_result(2, vec![err("error: bad component")]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, msg, _) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(!ok);
        let msg = msg.unwrap();
        assert!(msg.contains("bad component") && msg.ends_with('.'), "{msg}");
        assert_eq!(host.recorded_commands().len(), 2);
    }

    #[test]
    fn unparseable_publish_digest_is_warned_about() {
        let d = workdir();
        let host = built()
            .with_process_result(3, vec![])
            .with_process_result(0, vec![out("pushed")]);
        let ctx = Context::new(&host, wd(&d), "publish".into());
        let (ok, _, m) = outputs(run(&PublishPlugin, &ctx, input("0.2.0")));
        assert!(ok);
        assert!(!m.contains_key("digest"));
        assert!(host.logs().iter().any(|(_, l)| l.contains("digest")));
    }
}
