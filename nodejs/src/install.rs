//! `npm ci` / `npm install` — install dependencies, `ci` auto-selected by lockfile presence.

use crate::npm::{exit_phrase, has_lockfile, npm, require_package_json};
use moonlit_pdk::prelude::*;
use moonlit_pdk::process::LineHandler;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct InstallInput {
    /// Directory containing package.json. Defaults to ".".
    pub directory: String,
    /// Install only production dependencies (omit devDependencies). Defaults to false.
    pub production: bool,
    /// Force `npm ci` (true) or `npm install` (false). Omit to auto-select by lockfile presence.
    pub ci: Option<bool>,
}

impl Default for InstallInput {
    fn default() -> Self {
        Self {
            directory: ".".to_string(),
            production: false,
            ci: None,
        }
    }
}

#[derive(Default)]
pub struct Install;

impl Middleware for Install {
    const NAME: &'static str = "install";
    const DESCRIPTION: &'static str = "install npm dependencies (ci or install)";
    type Input = InstallInput;
    type Output = NoOutput;

    fn execute(&self, ctx: &Context, cfg: Self::Input) -> MiddlewareResult<Self::Output> {
        if let Err(msg) = require_package_json(ctx.working_dir(), &cfg.directory) {
            return MiddlewareResult::failure(msg);
        }
        let ci = cfg
            .ci
            .unwrap_or_else(|| has_lockfile(ctx.working_dir(), &cfg.directory));
        let mut args = vec![if ci {
            "ci".to_string()
        } else {
            "install".to_string()
        }];
        if cfg.production {
            args.push("--omit=dev".to_string());
        }
        match npm(ctx, &cfg.directory)
            .args(args)
            .stream(LineHandler::severity())
        {
            Ok(o) if o.success() => MiddlewareResult::ok(NoOutput {}),
            Ok(o) => MiddlewareResult::failure(format!(
                "Failed to install dependencies: {}",
                exit_phrase(o.exit_code)
            )),
            Err(e) => MiddlewareResult::failure(format!("Failed to install dependencies: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonlit_pdk::testing::{run, MockHost};

    fn proj_dir(lockfile: bool) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("package.json"), b"{}").unwrap();
        if lockfile {
            std::fs::write(d.path().join("package-lock.json"), b"{}").unwrap();
        }
        d
    }
    fn ctx<'a>(host: &'a MockHost, dir: &std::path::Path) -> Context<'a> {
        Context::new(host, dir.to_str().unwrap().into(), "install".into())
    }

    #[test]
    fn lockfile_present_selects_ci() {
        let d = proj_dir(true);
        let host = MockHost::new().with_process_result(0, vec![]);
        assert!(run(&Install, &ctx(&host, d.path()), InstallInput::default()).is_success());
        assert_eq!(host.recorded_commands()[0].args, vec!["ci".to_string()]);
    }

    #[test]
    fn no_lockfile_selects_install() {
        let d = proj_dir(false);
        let host = MockHost::new().with_process_result(0, vec![]);
        let _ = run(&Install, &ctx(&host, d.path()), InstallInput::default());
        assert_eq!(
            host.recorded_commands()[0].args,
            vec!["install".to_string()]
        );
    }

    #[test]
    fn explicit_ci_false_overrides_lockfile() {
        let d = proj_dir(true);
        let host = MockHost::new().with_process_result(0, vec![]);
        let cfg = InstallInput {
            ci: Some(false),
            ..Default::default()
        };
        let _ = run(&Install, &ctx(&host, d.path()), cfg);
        assert_eq!(
            host.recorded_commands()[0].args,
            vec!["install".to_string()]
        );
    }

    #[test]
    fn production_appends_omit_dev() {
        let d = proj_dir(true);
        let host = MockHost::new().with_process_result(0, vec![]);
        let cfg = InstallInput {
            production: true,
            ..Default::default()
        };
        let _ = run(&Install, &ctx(&host, d.path()), cfg);
        assert_eq!(
            host.recorded_commands()[0].args,
            vec!["ci".to_string(), "--omit=dev".to_string()]
        );
    }

    #[test]
    fn missing_package_json_fails_before_spawn() {
        let d = tempfile::tempdir().unwrap();
        let host = MockHost::new();
        let w = run(&Install, &ctx(&host, d.path()), InstallInput::default()).into_wit();
        assert!(!w.successful);
        assert!(w
            .error_message
            .unwrap()
            .starts_with("package.json not found in directory:"));
        assert!(host.recorded_commands().is_empty());
    }

    #[test]
    fn non_zero_exit_maps_to_install_failure() {
        let d = proj_dir(true);
        let host = MockHost::new().with_process_result(1, vec![]);
        let w = run(&Install, &ctx(&host, d.path()), InstallInput::default()).into_wit();
        assert_eq!(
            w.error_message.as_deref(),
            Some("Failed to install dependencies: Npm command failed with exit code 1")
        );
    }
}
