//! `npm run <command>` — optionally bump the version first, then run the build script.

use crate::npm::{exit_phrase, maybe_set_version, npm, require_package_json};
use moonlit_pdk::prelude::*;
use moonlit_pdk::process::LineHandler;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct BuildInput {
    /// Directory containing package.json. Defaults to ".".
    pub directory: String,
    /// package.json script to run for the build. Defaults to "build".
    pub command: String,
    /// Version to set in package.json before building. Omit to leave it unchanged.
    pub version: Option<String>,
}

impl Default for BuildInput {
    fn default() -> Self {
        Self {
            directory: ".".to_string(),
            command: "build".to_string(),
            version: None,
        }
    }
}

#[derive(Default)]
pub struct Build;

impl Middleware for Build {
    const NAME: &'static str = "build";
    const DESCRIPTION: &'static str = "run the npm build script (optional version bump first)";
    type Input = BuildInput;
    type Output = NoOutput;

    fn execute(&self, ctx: &Context, cfg: Self::Input) -> MiddlewareResult<Self::Output> {
        if let Err(msg) = require_package_json(ctx.working_dir(), &cfg.directory) {
            return MiddlewareResult::failure(msg);
        }
        if let Some(fail) = maybe_set_version(ctx, &cfg.directory, &cfg.version) {
            return fail;
        }
        let args = vec!["run".to_string(), cfg.command.clone()];
        match npm(ctx, &cfg.directory)
            .args(args)
            .stream(LineHandler::severity())
        {
            Ok(o) if o.success() => MiddlewareResult::ok(NoOutput {}),
            Ok(o) => MiddlewareResult::failure(format!(
                "Failed to build project: {}",
                exit_phrase(o.exit_code)
            )),
            Err(e) => MiddlewareResult::failure(format!("Failed to build project: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonlit_pdk::testing::{run, MockHost};

    fn proj_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("package.json"), b"{}").unwrap();
        d
    }
    fn ctx<'a>(host: &'a MockHost, dir: &std::path::Path) -> Context<'a> {
        Context::new(host, dir.to_str().unwrap().into(), "build".into())
    }

    #[test]
    fn runs_command_without_version_step() {
        let d = proj_dir();
        let host = MockHost::new().with_process_result(0, vec![]);
        let cfg = BuildInput {
            command: "compile".into(),
            ..Default::default()
        };
        assert!(run(&Build, &ctx(&host, d.path()), cfg).is_success());
        let cmds = host.recorded_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].args, vec!["run", "compile"]);
    }

    #[test]
    fn version_step_precedes_run() {
        let d = proj_dir();
        // Two npm calls: `npm version …` then `npm run build`. Enqueue TWO results.
        let host = MockHost::new()
            .with_process_result(0, vec![])
            .with_process_result(0, vec![]);
        let cfg = BuildInput {
            version: Some("2.0.0".into()),
            ..Default::default()
        };
        assert!(run(&Build, &ctx(&host, d.path()), cfg).is_success());
        let cmds = host.recorded_commands();
        assert_eq!(
            cmds[0].args,
            vec![
                "version",
                "2.0.0",
                "--no-git-tag-version",
                "--allow-same-version"
            ]
        );
        assert_eq!(cmds[1].args, vec!["run", "build"]);
    }

    #[test]
    fn version_step_failure_short_circuits() {
        let d = proj_dir();
        let host = MockHost::new().with_process_result(1, vec![]); // version step fails
        let cfg = BuildInput {
            version: Some("2.0.0".into()),
            ..Default::default()
        };
        let w = run(&Build, &ctx(&host, d.path()), cfg).into_wit();
        assert_eq!(
            w.error_message.as_deref(),
            Some("Failed to set version: Npm command failed with exit code 1")
        );
        // The run step never executed.
        assert_eq!(host.recorded_commands().len(), 1);
    }

    #[test]
    fn non_zero_run_maps_to_build_failure() {
        let d = proj_dir();
        let host = MockHost::new().with_process_result(3, vec![]);
        let w = run(&Build, &ctx(&host, d.path()), BuildInput::default()).into_wit();
        assert_eq!(
            w.error_message.as_deref(),
            Some("Failed to build project: Npm command failed with exit code 3")
        );
    }

    #[test]
    fn missing_package_json_fails_before_spawn() {
        let d = tempfile::tempdir().unwrap();
        let host = MockHost::new();
        let w = run(&Build, &ctx(&host, d.path()), BuildInput::default()).into_wit();
        assert!(w
            .error_message
            .unwrap()
            .starts_with("package.json not found in directory:"));
        assert!(host.recorded_commands().is_empty());
    }
}
