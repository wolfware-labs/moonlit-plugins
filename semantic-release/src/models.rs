//! Domain models shared across the three middlewares.

use moonlit_sdk::prelude::*; // Deserialize, Shared
use serde::Serialize;

/// A raw commit as produced by the `git` plugin's `commits.details` output.
/// Only `sha`, `date`, and `message` participate in the algorithm.
#[derive(Deserialize, Clone, Default, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct Commit {
    /// Commit SHA.
    pub sha: String,
    /// Commit author name.
    pub author: String,
    /// Commit author email.
    pub email: String,
    /// Author date (ISO 8601).
    pub date: String,
    /// Full commit message (subject and body).
    pub message: String,
}

/// A parsed conventional commit. Emitted by `analyze`, stored in `SrShared`, and
/// consumed by `calculate-version` / `generate-changelog`. `#[serde(default)]` lets
/// config-provided arrays omit fields and round-trips analyze's own output.
#[derive(Serialize, Deserialize, Clone, Default, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ConventionalCommit {
    /// Commit SHA.
    pub sha: String,
    /// One-line summary (the conventional-commit description).
    pub summary: String,
    /// Conventional-commit type (e.g. "feat", "fix").
    #[serde(rename = "type")]
    pub kind: String,
    /// Optional scope (the parenthesized part of the type).
    pub scope: Option<String>,
    /// Commit body (everything after the summary line).
    pub body: String,
    /// Whether the commit is marked as a breaking change.
    pub is_breaking_change: bool,
    /// The original, unparsed commit message.
    pub raw_message: String,
    /// Author date (ISO 8601).
    pub date: String,
}

/// Version bump magnitude. Declaration order == `Ord` order (None < Patch < Minor
/// < Major), which the algorithm relies on for "highest bump wins" and
/// "bump <= current-version-level".
#[derive(
    Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, schemars::JsonSchema,
)]
#[repr(u8)]
pub enum VersionBumpType {
    /// No version bump.
    #[default]
    None = 0,
    /// Patch bump (x.y.Z).
    Patch = 1,
    /// Minor bump (x.Y.0).
    Minor = 2,
    /// Major bump (X.0.0).
    Major = 3,
}

/// A version-bump rule: match by type/scope, produce a bump.
#[derive(Deserialize, Clone, Default, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", default)]
pub struct ReleaseRule {
    /// Conventional-commit type this rule matches (empty/omitted matches any type).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Scope this rule matches (empty/omitted matches any scope).
    pub scope: Option<String>,
    /// Version bump to apply when a commit matches this rule.
    pub release: VersionBumpType,
}

impl ReleaseRule {
    pub fn new(kind: &str, release: VersionBumpType) -> Self {
        Self {
            kind: Some(kind.to_string()),
            scope: None,
            release,
        }
    }

    pub fn matches(&self, c: &ConventionalCommit) -> bool {
        let type_ok = self
            .kind
            .as_deref()
            .is_none_or(|t| t.is_empty() || t.eq_ignore_ascii_case(&c.kind));
        let scope_ok = self.scope.as_deref().is_none_or(|s| {
            s.is_empty()
                || c.scope
                    .as_deref()
                    .is_some_and(|cs| cs.eq_ignore_ascii_case(s))
        });
        type_ok && scope_ok
    }
}

/// A changelog category rule. `matches` ports 1.x exactly: a breaking commit
/// satisfies the breaking rule's first clause and fails every non-breaking rule's
/// `is_breaking_change == false` clause, so it lands in "Breaking Changes" only.
#[derive(Deserialize, Clone, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangelogRule {
    /// Conventional-commit type this category collects (e.g. "feat").
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Match breaking-change commits instead of matching by type.
    #[serde(default)]
    pub is_breaking_change: bool,
    /// Emoji/icon shown next to the category heading.
    pub icon: String,
    /// Category heading (e.g. "Features").
    pub section: String,
    /// Short description shown under the category heading.
    pub summary: String,
}

impl ChangelogRule {
    pub fn matches(&self, c: &ConventionalCommit) -> bool {
        if self.is_breaking_change && c.is_breaking_change {
            return true;
        }
        self.kind
            .as_deref()
            .is_some_and(|t| t.eq_ignore_ascii_case(&c.kind))
            && c.is_breaking_change == self.is_breaking_change
    }
}

/// Plugin-wide shared state — one instance per pipeline run. `analyze` writes the
/// parsed commits; `calculate-version` / `generate-changelog` read them back.
#[derive(Default)]
pub struct SrShared {
    pub commits: Shared<Vec<ConventionalCommit>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(kind: &str, scope: Option<&str>, breaking: bool) -> ConventionalCommit {
        ConventionalCommit {
            kind: kind.to_string(),
            scope: scope.map(str::to_string),
            is_breaking_change: breaking,
            ..Default::default()
        }
    }

    #[test]
    fn bump_type_orders_none_lt_patch_lt_minor_lt_major() {
        assert!(VersionBumpType::None < VersionBumpType::Patch);
        assert!(VersionBumpType::Patch < VersionBumpType::Minor);
        assert!(VersionBumpType::Minor < VersionBumpType::Major);
    }

    #[test]
    fn release_rule_matches_type_case_insensitively_and_ignores_scope_when_unset() {
        let r = ReleaseRule::new("feat", VersionBumpType::Minor);
        assert!(r.matches(&commit("FEAT", Some("cli"), false)));
        assert!(!r.matches(&commit("fix", None, false)));
    }

    #[test]
    fn release_rule_with_scope_requires_scope_match() {
        let r = ReleaseRule {
            kind: Some("feat".into()),
            scope: Some("cli".into()),
            release: VersionBumpType::Minor,
        };
        assert!(r.matches(&commit("feat", Some("CLI"), false)));
        assert!(!r.matches(&commit("feat", None, false)));
        assert!(!r.matches(&commit("feat", Some("api"), false)));
    }

    #[test]
    fn changelog_rule_breaking_commit_matches_only_breaking_rule() {
        let feat = ChangelogRule {
            kind: Some("feat".into()),
            is_breaking_change: false,
            icon: ":sparkles:".into(),
            section: "Features".into(),
            summary: "New features".into(),
        };
        let breaking = ChangelogRule {
            kind: Some("breaking".into()),
            is_breaking_change: true,
            icon: ":boom:".into(),
            section: "Breaking Changes".into(),
            summary: "Breaking changes".into(),
        };
        let breaking_feat = commit("feat", None, true);
        assert!(
            !feat.matches(&breaking_feat),
            "breaking feat must NOT land in Features"
        );
        assert!(
            breaking.matches(&breaking_feat),
            "breaking feat lands in Breaking Changes"
        );
        let plain_feat = commit("feat", None, false);
        assert!(feat.matches(&plain_feat));
        assert!(!breaking.matches(&plain_feat));
    }
}
