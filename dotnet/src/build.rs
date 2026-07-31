//! `dotnet build` — build a project with SemVer-derived assembly metadata.

use crate::dotnet::{dotnet, exit_phrase, resolve};
use crate::version::{assembly_or_file_version, informational_version};
use moonlit_sdk::prelude::*;
use moonlit_sdk::process::LineHandler;

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct BuildInput {
    /// Project or solution file to build. Defaults to the one in the working directory.
    pub project: String,
    /// SemVer used to derive assembly metadata when the specific versions are unset.
    pub version: Option<String>,
    /// Explicit assembly version. Overrides the value derived from `version`.
    pub assembly_version: Option<String>,
    /// Explicit file version. Overrides the value derived from `version`.
    pub file_version: Option<String>,
    /// Explicit informational version. Overrides the value derived from `version`.
    pub informational_version: Option<String>,
    /// Build configuration. Defaults to "Release".
    pub configuration: String,
    /// Skip the implicit restore before building. Defaults to false.
    pub no_restore: bool,
}

impl Default for BuildInput {
    fn default() -> Self {
        Self {
            project: String::new(),
            version: None,
            assembly_version: None,
            file_version: None,
            informational_version: None,
            configuration: "Release".to_string(),
            no_restore: false,
        }
    }
}

#[derive(Default)]
pub struct Build;

impl Middleware for Build {
    const NAME: &'static str = "build";
    const DESCRIPTION: &'static str = "build a .NET project with derived assembly versions";
    type Input = BuildInput;
    type Output = NoOutput;

    fn execute(&self, ctx: &Context, cfg: Self::Input) -> MiddlewareResult<Self::Output> {
        let proj_path = resolve(ctx.working_dir(), &cfg.project);
        if !proj_path.is_file() {
            return MiddlewareResult::failure(format!(
                "Project file not found at path: {}",
                proj_path.display()
            ));
        }

        let assembly_version = match assembly_or_file_version(&cfg.assembly_version, &cfg.version) {
            Some(v) => v,
            None => return MiddlewareResult::failure(
                "AssemblyVersion could not be determined. Please specify it in the configuration or provide a valid Version.",
            ),
        };
        let file_version = match assembly_or_file_version(&cfg.file_version, &cfg.version) {
            Some(v) => v,
            None => return MiddlewareResult::failure(
                "FileVersion could not be determined. Please specify it in the configuration or provide a valid Version.",
            ),
        };
        let info_version = match informational_version(&cfg.informational_version, &cfg.version) {
            Some(v) => v,
            None => return MiddlewareResult::failure(
                "InformationalVersion could not be determined. Please specify it in the configuration or provide a valid Version.",
            ),
        };

        let mut args: Vec<String> = vec![
            "build".to_string(),
            cfg.project.clone(),
            format!("-p:AssemblyVersion={assembly_version}"),
            format!("-p:FileVersion={file_version}"),
            format!("-p:InformationalVersion={info_version}"),
            "--configuration".to_string(),
            cfg.configuration.clone(),
        ];
        if cfg.no_restore {
            args.push("--no-restore".to_string());
        }

        match dotnet(ctx).args(args).stream(LineHandler::severity()) {
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
    use moonlit_sdk::testing::{run, MockHost};

    fn proj_dir() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("app.csproj"), b"<Project/>").unwrap();
        d
    }

    #[test]
    fn builds_with_derived_versions_and_no_restore() {
        let d = proj_dir();
        let host = MockHost::new().with_process_result(0, vec![]);
        let ctx = Context::new(&host, d.path().to_str().unwrap().into(), "build".into());
        let cfg = BuildInput {
            project: "app.csproj".into(),
            version: Some("1.2.3-rc.1".into()),
            no_restore: true,
            ..Default::default()
        };
        assert!(run(&Build, &ctx, cfg).is_success());
        let cmds = host.recorded_commands();
        assert_eq!(cmds[0].program, "dotnet");
        assert_eq!(
            cmds[0].args,
            vec![
                "build",
                "app.csproj",
                "-p:AssemblyVersion=1.2.3",
                "-p:FileVersion=1.2.3",
                "-p:InformationalVersion=1.2.3-rc.1",
                "--configuration",
                "Release",
                "--no-restore",
            ]
        );
    }

    #[test]
    fn missing_project_fails() {
        let d = tempfile::tempdir().unwrap();
        let host = MockHost::new();
        let ctx = Context::new(&host, d.path().to_str().unwrap().into(), "build".into());
        let cfg = BuildInput {
            project: "nope.csproj".into(),
            version: Some("1.0.0".into()),
            ..Default::default()
        };
        let w = run(&Build, &ctx, cfg).into_wit();
        assert!(!w.successful);
        let m = w.error_message.unwrap();
        assert!(m.starts_with("Project file not found at path:"), "got: {m}");
        assert!(m.ends_with("nope.csproj"), "got: {m}");
    }

    #[test]
    fn unresolved_assembly_version_fails_with_exact_message() {
        let d = proj_dir();
        let host = MockHost::new();
        let ctx = Context::new(&host, d.path().to_str().unwrap().into(), "build".into());
        let cfg = BuildInput {
            project: "app.csproj".into(),
            ..Default::default()
        };
        let w = run(&Build, &ctx, cfg).into_wit();
        assert_eq!(
            w.error_message.as_deref(),
            Some("AssemblyVersion could not be determined. Please specify it in the configuration or provide a valid Version.")
        );
    }

    #[test]
    fn non_zero_exit_maps_to_build_failure() {
        let d = proj_dir();
        let host = MockHost::new().with_process_result(1, vec![]);
        let ctx = Context::new(&host, d.path().to_str().unwrap().into(), "build".into());
        let cfg = BuildInput {
            project: "app.csproj".into(),
            version: Some("1.0.0".into()),
            ..Default::default()
        };
        let w = run(&Build, &ctx, cfg).into_wit();
        assert!(!w.successful);
        assert_eq!(
            w.error_message.as_deref(),
            Some("Failed to build project: Dotnet command failed with exit code 1")
        );
    }
}
