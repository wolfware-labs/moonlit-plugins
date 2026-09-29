//! `github commit-files` — commit working-tree files to a branch through the
//! GraphQL `createCommitOnBranch` mutation. Commits made this way with an App
//! or bot token are signed by GitHub, so they satisfy "require signed commits"
//! without any signing key in CI.

use std::path::PathBuf;

use base64::Engine as _;
use moonlit_pdk::prelude::*;
use serde_json::json;

use crate::api;
use crate::config::GithubPluginConfig;
use crate::context::resolve_context;

const MUTATION: &str = "mutation($input: CreateCommitOnBranchInput!) { \
    createCommitOnBranch(input: $input) { commit { oid url } } }";

#[derive(Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct CommitFilesInput {
    /// Branch to commit to. Required.
    branch: String,
    /// Commit message. The first line is the headline, the rest the body. Required.
    message: String,
    /// Repository-relative paths whose working-tree contents are committed. Required.
    files: Vec<String>,
    /// Commit the branch must still point at. Defaults to the local `HEAD`.
    expected_head_sha: Option<String>,
}

/// Output published by `commit-files`: the new commit.
#[derive(Serialize, schemars::JsonSchema)]
pub struct CommitFilesOutput {
    pub sha: String,
    pub url: String,
}

#[derive(Default)]
pub struct CommitFiles;

/// Where the working directory is visible to this component. Under the engine
/// it is preopened as `.`; native unit tests have no preopen and use the real path.
fn fs_root(ctx: &Context) -> PathBuf {
    if cfg!(target_arch = "wasm32") {
        PathBuf::from(".")
    } else {
        PathBuf::from(ctx.working_dir())
    }
}

fn check_relative(path: &str) -> Result<(), String> {
    let p = path.trim();
    if p.is_empty() {
        return Err("File path must not be empty.".to_string());
    }
    if p.starts_with('/') || p.starts_with('\\') || p.contains(':') {
        return Err(format!(
            "File path '{path}' must be relative to the repository root."
        ));
    }
    if p.split(['/', '\\']).any(|c| c == "..") {
        return Err(format!("File path '{path}' must not leave the repository."));
    }
    Ok(())
}

/// The GraphQL error messages joined, or the raw `errors` JSON when none has a message.
fn describe(errors: &[serde_json::Value]) -> String {
    let msgs: Vec<&str> = errors
        .iter()
        .filter_map(|e| e["message"].as_str())
        .collect();
    if msgs.is_empty() {
        serde_json::Value::Array(errors.to_vec()).to_string()
    } else {
        msgs.join("; ")
    }
}

impl Middleware for CommitFiles {
    const NAME: &'static str = "commit-files";
    const DESCRIPTION: &'static str =
        "commit working-tree files to a branch via the GitHub API (signed by GitHub)";
    type Input = CommitFilesInput;
    type Output = CommitFilesOutput;

    fn execute(&self, ctx: &Context, cfg: Self::Input) -> MiddlewareResult<Self::Output> {
        let branch = cfg.branch.trim();
        if branch.is_empty() {
            return MiddlewareResult::failure("Branch is required.");
        }
        if cfg.message.trim().is_empty() {
            return MiddlewareResult::failure("Commit message is required.");
        }
        if cfg.files.is_empty() {
            return MiddlewareResult::failure("At least one file is required.");
        }
        let mut additions = Vec::with_capacity(cfg.files.len());
        for f in &cfg.files {
            if let Err(e) = check_relative(f) {
                return MiddlewareResult::failure(e);
            }
            let path = f.trim().replace('\\', "/");
            let bytes = match std::fs::read(fs_root(ctx).join(&path)) {
                Ok(b) => b,
                Err(e) => return MiddlewareResult::failure(format!("Cannot read {path}: {e}.")),
            };
            additions.push(json!({
                "path": path,
                "contents": base64::engine::general_purpose::STANDARD.encode(bytes),
            }));
        }

        let context = match resolve_context(ctx) {
            Ok(c) => c,
            Err(f) => return f,
        };
        let head = match cfg
            .expected_head_sha
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(s) => s.to_string(),
            None => match ctx
                .command("git")
                .cwd(ctx.working_dir())
                .args(["rev-parse", "HEAD"])
                .run()
            {
                Ok(o) if o.success() => o.stdout().trim().to_string(),
                _ => return MiddlewareResult::failure("Cannot resolve the local HEAD commit."),
            },
        };

        let (headline, body) = match cfg.message.trim().split_once('\n') {
            Some((h, b)) => (
                h.trim().to_string(),
                Some(b.trim().to_string()).filter(|b| !b.is_empty()),
            ),
            None => (cfg.message.trim().to_string(), None),
        };
        let mut message = json!({ "headline": headline });
        if let Some(b) = body {
            message["body"] = json!(b);
        }
        let payload = json!({
            "query": MUTATION,
            "variables": { "input": {
                "branch": {
                    "repositoryNameWithOwner": format!("{}/{}", context.owner, context.repo),
                    "branchName": branch,
                },
                "message": message,
                "expectedHeadOid": head,
                "fileChanges": { "additions": additions },
            }},
        });

        let token = ctx.plugin_config::<GithubPluginConfig>().token.clone();
        let resp = match api::post_json(ctx, &token, "/graphql", &payload) {
            Ok(r) => r,
            Err(e) => return MiddlewareResult::failure(e),
        };
        let v: serde_json::Value = match resp.json() {
            Ok(v) => v,
            Err(e) => return MiddlewareResult::failure(e),
        };
        let commit = &v["data"]["createCommitOnBranch"]["commit"];
        let errors = v["errors"].as_array().filter(|e| !e.is_empty());
        if let (Some(sha), Some(url)) = (commit["oid"].as_str(), commit["url"].as_str()) {
            // The commit landed; failing would make a rerun hit a stale head.
            if let Some(errors) = errors {
                ctx.log_warn(&format!(
                    "GitHub reported errors alongside the commit: {}",
                    describe(errors)
                ));
            }
            return MiddlewareResult::ok(CommitFilesOutput {
                sha: sha.to_string(),
                url: url.to_string(),
            });
        }
        // GraphQL reports failures with HTTP 200 and an `errors` array.
        match errors {
            Some(errors) => MiddlewareResult::failure(format!(
                "GitHub rejected the commit: {}.",
                describe(errors)
            )),
            None => MiddlewareResult::failure("GitHub returned no commit."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::GithubShared;
    use moonlit_pdk::process::{OutputChunk, StdioStream};
    use moonlit_pdk::testing::{run, MockHost};
    use std::collections::HashMap;

    fn line(text: &str) -> OutputChunk {
        OutputChunk {
            stream: StdioStream::Stdout,
            text: text.into(),
        }
    }
    fn base<'a>(
        host: &'a MockHost,
        sh: &'a GithubShared,
        pc: &'a GithubPluginConfig,
        wd: &str,
    ) -> Context<'a> {
        Context::new(host, wd.into(), "commit".into())
            .with_state(sh)
            .with_plugin_config(pc)
    }
    fn repo() -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir(d.path().join("git")).unwrap();
        std::fs::write(d.path().join("git/Cargo.toml"), "x").unwrap();
        std::fs::write(d.path().join("Cargo.lock"), "lock").unwrap();
        d
    }
    fn input() -> CommitFilesInput {
        CommitFilesInput {
            branch: "main".into(),
            message: "chore(release): git 0.2.0\n\nReleased by CI.".into(),
            files: vec!["git/Cargo.toml".into(), "Cargo.lock".into()],
            expected_head_sha: Some("abc".into()),
        }
    }
    const OK: &[u8] = br#"{"data":{"createCommitOnBranch":{"commit":{"oid":"def","url":"https://github.com/o/r/commit/def"}}}}"#;

    #[test]
    fn required_fields_fail_before_any_request() {
        let host = MockHost::new();
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(&host, &sh, &pc, "/nowhere");
        for (mut i, msg) in [
            (input(), "Branch is required."),
            (input(), "Commit message is required."),
            (input(), "At least one file is required."),
        ] {
            match msg {
                "Branch is required." => i.branch = " ".into(),
                "Commit message is required." => i.message = "".into(),
                _ => i.files.clear(),
            }
            let w = run(&CommitFiles, &ctx, i).into_wit();
            assert_eq!(w.error_message.as_deref(), Some(msg));
        }
        assert!(host.recorded_requests().is_empty());
    }

    #[test]
    fn escaping_file_path_fails_before_io() {
        let host = MockHost::new();
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(&host, &sh, &pc, "/nowhere");
        let mut i = input();
        i.files = vec!["../secrets".into()];
        let w = run(&CommitFiles, &ctx, i).into_wit();
        assert!(w.error_message.unwrap().contains("must not leave"));
        assert!(host.recorded_commands().is_empty());
    }

    #[test]
    fn unreadable_file_fails_before_any_request() {
        let d = repo();
        let host = MockHost::new();
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(&host, &sh, &pc, d.path().to_str().unwrap());
        let mut i = input();
        i.files = vec!["missing.toml".into()];
        let w = run(&CommitFiles, &ctx, i).into_wit();
        assert!(w
            .error_message
            .unwrap()
            .starts_with("Cannot read missing.toml"));
        assert!(host.recorded_requests().is_empty());
    }

    #[test]
    fn sends_create_commit_on_branch() {
        let d = repo();
        let host = MockHost::new()
            .with_process_result(0, vec![line("https://github.com/o/r.git")])
            .with_http_response(200, OK);
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(&host, &sh, &pc, d.path().to_str().unwrap());
        let w = run(&CommitFiles, &ctx, input()).into_wit();
        assert!(w.successful, "{:?}", w.error_message);
        let m: HashMap<_, _> = w.output.into_iter().collect();
        assert_eq!(m["sha"], "\"def\"");
        assert_eq!(m["url"], "\"https://github.com/o/r/commit/def\"");

        let req = &host.recorded_requests()[0];
        assert_eq!(req.authority, "api.github.com");
        assert_eq!(req.path_with_query, "/graphql");
        let body: serde_json::Value = serde_json::from_slice(req.body.as_deref().unwrap()).unwrap();
        assert!(body["query"]
            .as_str()
            .unwrap()
            .contains("createCommitOnBranch"));
        let i = &body["variables"]["input"];
        assert_eq!(i["branch"]["repositoryNameWithOwner"], "o/r");
        assert_eq!(i["branch"]["branchName"], "main");
        assert_eq!(i["expectedHeadOid"], "abc");
        assert_eq!(i["message"]["headline"], "chore(release): git 0.2.0");
        assert_eq!(i["message"]["body"], "Released by CI.");
        assert_eq!(i["fileChanges"]["additions"][0]["path"], "git/Cargo.toml");
        assert_eq!(i["fileChanges"]["additions"][0]["contents"], "eA==");
        assert_eq!(i["fileChanges"]["additions"][1]["contents"], "bG9jaw==");
    }

    #[test]
    fn expected_head_defaults_to_local_head() {
        let d = repo();
        let host = MockHost::new()
            .with_process_result(0, vec![line("https://github.com/o/r.git")])
            .with_process_result(0, vec![line("1234567")])
            .with_http_response(200, OK);
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(&host, &sh, &pc, d.path().to_str().unwrap());
        let mut i = input();
        i.expected_head_sha = None;
        i.message = "chore(release): git 0.2.0".into();
        assert!(run(&CommitFiles, &ctx, i).is_success());
        assert_eq!(host.recorded_commands()[1].args, vec!["rev-parse", "HEAD"]);
        let body: serde_json::Value =
            serde_json::from_slice(host.recorded_requests()[0].body.as_deref().unwrap()).unwrap();
        assert_eq!(body["variables"]["input"]["expectedHeadOid"], "1234567");
        assert!(body["variables"]["input"]["message"].get("body").is_none());
    }

    #[test]
    fn graphql_errors_fail_the_step() {
        let d = repo();
        let host = MockHost::new()
            .with_process_result(0, vec![line("https://github.com/o/r.git")])
            .with_http_response(
                200,
                br#"{"data":{"createCommitOnBranch":null},"errors":[{"message":"Expected branch to point to \"abc\" but it did not."}]}"#,
            );
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(&host, &sh, &pc, d.path().to_str().unwrap());
        let w = run(&CommitFiles, &ctx, input()).into_wit();
        assert!(!w.successful);
        assert!(w
            .error_message
            .unwrap()
            .contains("Expected branch to point to"));
    }

    fn remote_host(responses: &[(u16, &[u8])]) -> MockHost {
        let mut h =
            MockHost::new().with_process_result(0, vec![line("https://github.com/o/r.git")]);
        for (st, b) in responses {
            h = h.with_http_response(*st, b);
        }
        h
    }

    fn fail_msg(host: &MockHost, i: CommitFilesInput) -> String {
        let d = repo();
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(host, &sh, &pc, d.path().to_str().unwrap());
        let w = run(&CommitFiles, &ctx, i).into_wit();
        assert!(!w.successful);
        w.error_message.unwrap()
    }

    #[test]
    fn errors_without_messages_include_raw_json_and_end_with_period() {
        let host = remote_host(&[(200, br#"{"errors":[{"type":"X"}]}"#)]);
        let msg = fail_msg(&host, input());
        assert!(msg.contains("type"), "{msg}");
        assert!(msg.ends_with('.'), "{msg}");
    }

    #[test]
    fn graphql_error_message_ends_with_period() {
        let host = remote_host(&[(200, br#"{"errors":[{"message":"nope"}]}"#)]);
        assert_eq!(
            fail_msg(&host, input()),
            "GitHub rejected the commit: nope."
        );
    }

    #[test]
    fn commit_present_alongside_errors_is_success() {
        let d = repo();
        let host = remote_host(&[(
            200,
            br#"{"data":{"createCommitOnBranch":{"commit":{"oid":"def","url":"u"}}},"errors":[{"message":"partial"}]}"#,
        )]);
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(&host, &sh, &pc, d.path().to_str().unwrap());
        let w = run(&CommitFiles, &ctx, input()).into_wit();
        assert!(w.successful, "{:?}", w.error_message);
        let m: HashMap<_, _> = w.output.into_iter().collect();
        assert_eq!(m["sha"], "\"def\"");
    }

    #[test]
    fn check_relative_table() {
        for bad in ["/abs", "\\x", "C:\\x", "C:/x", "a\\..\\b", "", "  "] {
            assert!(check_relative(bad).is_err(), "{bad}");
        }
        for ok in ["a/b", "Cargo.lock"] {
            assert!(check_relative(ok).is_ok(), "{ok}");
        }
    }

    #[test]
    fn failing_rev_parse_is_reported() {
        let host = MockHost::new()
            .with_process_result(0, vec![line("https://github.com/o/r.git")])
            .with_process_result(1, vec![]);
        let mut i = input();
        i.expected_head_sha = None;
        assert_eq!(fail_msg(&host, i), "Cannot resolve the local HEAD commit.");
        assert!(host.recorded_requests().is_empty());
    }

    #[test]
    fn null_commit_without_errors_is_reported() {
        let host = remote_host(&[(200, br#"{"data":{"createCommitOnBranch":null}}"#)]);
        assert_eq!(fail_msg(&host, input()), "GitHub returned no commit.");
    }

    #[test]
    fn whitespace_expected_head_falls_back_to_rev_parse() {
        let d = repo();
        let host = MockHost::new()
            .with_process_result(0, vec![line("https://github.com/o/r.git")])
            .with_process_result(0, vec![line("1234567")])
            .with_http_response(200, OK);
        let (sh, pc) = (
            GithubShared::default(),
            GithubPluginConfig { token: "t".into() },
        );
        let ctx = base(&host, &sh, &pc, d.path().to_str().unwrap());
        let mut i = input();
        i.expected_head_sha = Some("   ".into());
        assert!(run(&CommitFiles, &ctx, i).is_success());
        assert_eq!(host.recorded_commands()[1].args, vec!["rev-parse", "HEAD"]);
    }
}
