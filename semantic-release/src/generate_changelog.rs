//! generate-changelog — structured changelog categories, with optional AI filter/refine.

use moonlit_sdk::changelog::Category;
use moonlit_sdk::prelude::*;

use crate::changelog::ChangelogGeneratorConfig;
use crate::models::{ConventionalCommit, SrShared};

/// Output published by `generate-changelog`: the structured categories. Absent
/// when there are no commits to summarize (that path returns an empty output).
#[derive(Serialize, Default, schemars::JsonSchema)]
pub struct GenerateChangelogOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    categories: Option<Vec<Category>>,
}

#[derive(Deserialize, Default, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct GenerateChangelogInput {
    /// Commits to include. Omit to use the commits stored by the `analyze` step.
    commits: Option<Vec<ConventionalCommit>>,
    /// Use AI to drop commits that are not user-facing. Requires the plugin `ai` config. Defaults to false.
    filter_non_user_facing_commits: bool,
    /// Use AI to rewrite each commit summary for the changelog. Requires the plugin `ai` config. Defaults to false.
    refine_commits_summary: bool,
    /// Rules mapping commits to changelog categories.
    changelog_rules: ChangelogGeneratorConfig,
}

#[derive(Default)]
pub struct GenerateChangelog;

impl Middleware for GenerateChangelog {
    const NAME: &'static str = "generate-changelog";
    const DESCRIPTION: &'static str = "generate structured changelog categories from commits";
    type Input = GenerateChangelogInput;
    type Output = GenerateChangelogOutput;

    fn execute(&self, ctx: &Context, cfg: Self::Input) -> MiddlewareResult<Self::Output> {
        let want_ai = cfg.filter_non_user_facing_commits || cfg.refine_commits_summary;

        let mut commits = cfg
            .commits
            .clone()
            .unwrap_or_else(|| ctx.state::<SrShared>().commits.get());
        if commits.is_empty() {
            ctx.log_warn("No commits provided for changelog generation.");
            return MiddlewareResult::ok(GenerateChangelogOutput::default());
        }

        if want_ai {
            let ai = match &ctx.plugin_config::<crate::config::SrPluginConfig>().ai {
                Some(a) => a,
                None => {
                    return MiddlewareResult::failure(
                        "AI refinement requires an 'ai' config block with an apiKey.",
                    )
                }
            };
            let client = crate::ai::build_client(ai);
            if cfg.filter_non_user_facing_commits {
                match crate::refine::filter_commits(ctx, &*client, commits) {
                    Ok(c) => commits = c,
                    Err(e) => return MiddlewareResult::failure(&e),
                }
            }
            if cfg.refine_commits_summary {
                match crate::refine::refine_summaries(ctx, &*client, commits) {
                    Ok(c) => commits = c,
                    Err(e) => return MiddlewareResult::failure(&e),
                }
            }
        }

        let categories = cfg.changelog_rules.generate(&commits);
        MiddlewareResult::ok(GenerateChangelogOutput {
            categories: Some(categories),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moonlit_sdk::testing::{run, MockHost};
    use moonlit_sdk::LogLevel;
    use serde_json::Value;

    fn cfg(json: Value) -> GenerateChangelogInput {
        moonlit_sdk::config::from_json_value(&json.to_string()).unwrap()
    }
    fn sr_cfg(json: Value) -> crate::config::SrPluginConfig {
        moonlit_sdk::config::from_json_value(&json.to_string()).unwrap()
    }
    fn run_with_config(
        shared: &SrShared,
        host: &MockHost,
        pc: &crate::config::SrPluginConfig,
        c: GenerateChangelogInput,
    ) -> moonlit_sdk::bindings::MiddlewareResult {
        let ctx = Context::new(host, "/w".into(), "s".into())
            .with_state(shared)
            .with_plugin_config(pc);
        run(&GenerateChangelog, &ctx, c).into_wit()
    }

    #[test]
    fn ai_flag_without_config_fails() {
        let shared = SrShared::default();
        shared.commits.set(vec![crate::models::ConventionalCommit {
            kind: "feat".into(),
            summary: "add flag".into(),
            sha: "abc1234".into(),
            ..Default::default()
        }]);
        let host = MockHost::new();
        let pc = sr_cfg(serde_json::json!({})); // no ai block
        let c = cfg(serde_json::json!({ "filterNonUserFacingCommits": true }));
        let w = run_with_config(&shared, &host, &pc, c);
        assert!(!w.successful);
        assert_eq!(
            w.error_message.as_deref(),
            Some("AI refinement requires an 'ai' config block with an apiKey.")
        );
    }

    #[test]
    fn filter_flag_drops_non_user_facing_then_builds_categories() {
        let shared = SrShared::default();
        shared.commits.set(vec![
            crate::models::ConventionalCommit {
                kind: "feat".into(),
                summary: "add flag".into(),
                sha: "a".into(),
                ..Default::default()
            },
            crate::models::ConventionalCommit {
                kind: "chore".into(),
                summary: "bump".into(),
                sha: "b".into(),
                ..Default::default()
            },
        ]);
        // OpenAI 200: drop index 1 (the chore).
        let host = MockHost::new().with_http_response(
            200,
            br#"{"choices":[{"message":{"content":"{\"drop\":[1]}"}}]}"#,
        );
        let pc = sr_cfg(serde_json::json!({ "ai": { "apiKey": "sk-x" } }));
        let c = cfg(serde_json::json!({ "filterNonUserFacingCommits": true }));
        let w = run_with_config(&shared, &host, &pc, c);
        assert!(w.successful);
        let out: std::collections::HashMap<String, Value> = w
            .output
            .into_iter()
            .map(|(k, v)| (k, serde_json::from_str(&v).unwrap()))
            .collect();
        let cats = out["categories"].as_array().unwrap();
        // Only the feat survived -> single "Features" category.
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0]["name"], "Features");
    }

    #[test]
    fn refine_flag_rewrites_summaries() {
        let shared = SrShared::default();
        shared.commits.set(vec![crate::models::ConventionalCommit {
            kind: "feat".into(),
            summary: "add flag".into(),
            sha: "a".into(),
            ..Default::default()
        }]);
        let host = MockHost::new().with_http_response(
            200, br#"{"choices":[{"message":{"content":"{\"summaries\":[{\"index\":0,\"summary\":\"Add the flag\"}]}"}}]}"#);
        let pc = sr_cfg(serde_json::json!({ "ai": { "apiKey": "sk-x" } }));
        let c = cfg(serde_json::json!({ "refineCommitsSummary": true }));
        let w = run_with_config(&shared, &host, &pc, c);
        assert!(w.successful);
        let out: std::collections::HashMap<String, Value> = w
            .output
            .into_iter()
            .map(|(k, v)| (k, serde_json::from_str(&v).unwrap()))
            .collect();
        assert_eq!(
            out["categories"][0]["entries"][0]["description"],
            "Add the flag"
        );
    }

    #[test]
    fn empty_commits_succeed_without_output_and_warn() {
        let shared = SrShared::default();
        let host = MockHost::new();
        let pc = sr_cfg(serde_json::json!({}));
        let w = run_with_config(&shared, &host, &pc, GenerateChangelogInput::default());
        assert!(w.successful);
        assert!(w.output.is_empty());
        assert!(host
            .logs()
            .iter()
            .any(|(l, m)| *l == LogLevel::Warn
                && m == "No commits provided for changelog generation."));
    }

    #[test]
    fn emits_categories_from_shared_commits() {
        let shared = SrShared::default();
        shared.commits.set(vec![crate::models::ConventionalCommit {
            kind: "feat".into(),
            summary: "add flag".into(),
            sha: "abc1234".into(),
            ..Default::default()
        }]);
        let host = MockHost::new();
        let pc = sr_cfg(serde_json::json!({}));
        let w = run_with_config(&shared, &host, &pc, GenerateChangelogInput::default());
        assert!(w.successful);
        let out: std::collections::HashMap<String, Value> = w
            .output
            .into_iter()
            .map(|(k, v)| (k, serde_json::from_str(&v).unwrap()))
            .collect();
        let cats = out["categories"].as_array().unwrap();
        assert_eq!(cats[0]["name"], "Features");
        assert_eq!(cats[0]["entries"][0]["description"], "add flag");
    }
}
