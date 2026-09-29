import time
import unittest

import pr_guard as g


def commit(msg, name="Ada", email="ada@example.com", sha="0123456789"):
    return {"sha": sha, "author_name": name, "author_email": email, "message": msg}


class Escape(unittest.TestCase):
    def test_escape_newline(self):
        result = g.escape("line1\nline2")
        self.assertNotIn("\n", result)
        self.assertIn("%0A", result)

    def test_escape_percent(self):
        self.assertEqual(g.escape("%"), "%25")

    def test_escape_roundtrip(self):
        original = "msg with % sign"
        escaped = g.escape(original)
        self.assertIn("%25", escaped)

    def test_escape_carriage_return(self):
        self.assertIn("%0D", g.escape("\r"))


class ParsePaths(unittest.TestCase):
    def test_parse_simple_paths(self):
        z_output = "git/a.rs\0slack/b.rs\0"
        paths = g.parse_paths(z_output)
        self.assertEqual(paths, ["git/a.rs", "slack/b.rs"])

    def test_parse_non_ascii_path(self):
        z_output = "slack/café.rs\0git/a.rs\0"
        paths = g.parse_paths(z_output)
        plugins = g.touched_plugins(paths)
        self.assertIn("slack", plugins)
        self.assertIn("git", plugins)

    def test_parse_empty(self):
        self.assertEqual(g.parse_paths(""), [])


class ParseLog(unittest.TestCase):
    def test_parse_single_commit(self):
        raw = "abc123\x1fJohn Doe\x1fjohn@example.com\x1ffix: msg\x1e"
        commits = g.parse_log(raw)
        self.assertEqual(len(commits), 1)
        self.assertEqual(commits[0]["sha"], "abc123")

    def test_parse_malformed_record(self):
        raw = "incomplete\x1frecord\x1e"
        commits = g.parse_log(raw)
        self.assertEqual(len(commits), 1)
        self.assertIn("error", commits[0])

    def test_parse_empty(self):
        self.assertEqual(g.parse_log(""), [])


class Title(unittest.TestCase):
    def test_valid_titles(self):
        for t in ["feat(git): add push tags", "fix(semantic-release)!: drop node 18",
                  "ci(ci): pin actions", "build(deps): bump serde"]:
            self.assertEqual(g.check_title(t), [], t)

    def test_missing_scope(self):
        self.assertTrue(g.check_title("feat: add thing"))

    def test_unknown_type_and_scope(self):
        errs = g.check_title("feature(gitt): x")
        self.assertEqual(len(errs), 2)

    def test_release_scope_is_not_allowed(self):
        self.assertTrue(g.check_title("chore(release): git 0.2.0"))


class Scope(unittest.TestCase):
    def test_matching_scope(self):
        self.assertEqual(g.check_scope("fix(git): x", ["git/src/tag.rs", "README.md"]), [])

    def test_wrong_scope(self):
        errs = g.check_scope("fix(github): x", ["git/src/tag.rs"])
        self.assertEqual(len(errs), 1)
        self.assertIn("'git'", errs[0])

    def test_two_plugins_fail(self):
        errs = g.check_scope("fix(git): x", ["git/a.rs", "gitlab/b.rs"])
        self.assertEqual(len(errs), 1)
        self.assertIn("split", errs[0])

    def test_deps_scope_may_touch_many_plugins(self):
        self.assertEqual(g.check_scope("build(deps): bump", ["git/Cargo.toml", "slack/Cargo.toml", "Cargo.lock"]), [])

    def test_deps_scope_with_root_manifest_and_lock_passes(self):
        self.assertEqual(g.check_scope("build(deps): bump",
                                       ["Cargo.toml", "Cargo.lock", "git/Cargo.toml", "moonlit/Cargo.toml"]), [])

    def test_deps_scope_with_github_path_passes(self):
        self.assertEqual(g.check_scope("build(deps): bump checkout", [".github/workflows/x.yml"]), [])

    def test_deps_scope_with_plugin_source_fails(self):
        errs = g.check_scope("build(deps): bump", ["git/Cargo.toml", "git/src/lib.rs", "Cargo.lock"])
        self.assertEqual(errs, ["Scope 'deps' is for dependency manifests only; this PR changes "
                                "git/src/lib.rs. Use the plugin's scope."])

    def test_deps_scope_with_nested_manifest_fails(self):
        errs = g.check_scope("build(deps): bump", ["git/tests/fixture/Cargo.toml"])
        self.assertEqual(len(errs), 1)
        self.assertIn("git/tests/fixture/Cargo.toml", errs[0])

    def test_root_only_change_with_repo_scope(self):
        self.assertEqual(g.check_scope("docs(repo): x", ["README.md", ".github/x.yml"]), [])

    def test_plugin_name_prefix_is_not_a_match(self):
        # "git" must not match files under "github/"
        errs = g.check_scope("fix(git): x", ["github/src/api.rs"])
        self.assertEqual(len(errs), 1)
        self.assertIn("'github'", errs[0])

    def test_plugin_scope_without_plugin_changes(self):
        errs = g.check_scope("fix(git): x", ["README.md"])
        self.assertEqual(len(errs), 1)
        self.assertIn("nothing under", errs[0])


class Dco(unittest.TestCase):
    def test_signed(self):
        self.assertEqual(g.check_dco([commit("fix: x\n\nSigned-off-by: Ada <ADA@example.com>")]), [])

    def test_unsigned(self):
        errs = g.check_dco([commit("fix: x")])
        self.assertIn("0123456", errs[0])

    def test_signed_by_someone_else(self):
        self.assertTrue(g.check_dco([commit("fix: x\n\nSigned-off-by: Bob <bob@example.com>")]))

    def test_dependabot_commit_is_exempt(self):
        self.assertEqual(g.check_dco([commit("build(deps): bump", name="dependabot[bot]",
                                             email="49699333+dependabot[bot]@users.noreply.github.com")],
                                     pr_author="dependabot[bot]"), [])

    def test_spoofed_bot_name_with_normal_email_not_exempt(self):
        errs = g.check_dco([commit("fix: x", name="dependabot[bot]", email="normal@example.com")],
                           pr_author="dependabot[bot]")
        self.assertTrue(errs)

    def test_spoofed_bot_commit_in_human_pr_not_exempt(self):
        errs = g.check_dco([commit("fix: x", name="dependabot[bot]",
                                   email="49699333+dependabot[bot]@users.noreply.github.com")],
                           pr_author="mallory")
        self.assertEqual(len(errs), 1)

    def test_bot_commit_without_pr_author_not_exempt(self):
        errs = g.check_dco([commit("fix: x", name="dependabot[bot]",
                                   email="49699333+dependabot[bot]@users.noreply.github.com")])
        self.assertEqual(len(errs), 1)

    def test_regex_no_catastrophic_backtrack(self):
        # Message with "Signed-off-by: " followed by many "<" should not hang
        msg = "fix: x\n\nSigned-off-by: " + (" <" * 20000)
        start = time.monotonic()
        result = g.check_dco([commit(msg)])
        elapsed = time.monotonic() - start
        self.assertLess(elapsed, 1.0)


if __name__ == "__main__":
    unittest.main()
