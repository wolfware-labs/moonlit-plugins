#!/usr/bin/env python3
"""PR guards for moonlit-plugins.

PRs are squash-merged, so the PR title becomes the commit on main and its scope
decides which plugin the release pipeline versions. Three checks keep that true:
the title is a scoped Conventional Commit, the scope names the plugin the PR
changes, and every commit carries a DCO sign-off.
"""
import argparse
import re
import subprocess
import sys

PLUGINS = ["docker", "dotnet", "git", "github", "gitlab", "moonlit", "nodejs", "semantic-release", "slack"]
OTHER_SCOPES = ["ci", "deps", "docs", "repo", "test-support"]
TYPES = ["feat", "fix", "perf", "refactor", "docs", "test", "build", "ci", "chore", "revert"]
BOTS = {"dependabot[bot]", "github-actions[bot]"}

TITLE_RE = re.compile(r"^(?P<type>[a-z]+)\((?P<scope>[a-z0-9-]+)\)(?P<bang>!)?: (?P<subject>\S.*)$")
# Linear regex pattern to avoid catastrophic backtracking: no greedy wildcards.
SIGNOFF_RE = re.compile(r"^Signed-off-by:[^<\n]*<(?P<email>[^<>\n]+)>[ \t]*$", re.MULTILINE)


def escape(msg):
    """Escape workflow command special characters to prevent injection."""
    msg = msg.replace("%", "%25")
    msg = msg.replace("\r", "%0D")
    msg = msg.replace("\n", "%0A")
    return msg


def parse_paths(z_output):
    """Parse git diff -z output into a list of paths."""
    if not z_output:
        return []
    return [p for p in z_output.split("\0") if p]


def parse_log(raw):
    """Parse git log output into a list of commit dicts.

    Format: sha\x1fname\x1femail\x1fmessage\x1e (repeated)
    Returns list of {"sha", "author_name", "author_email", "message"} dicts.
    On malformed records, returns {"error": ...} dict.
    """
    if not raw:
        return []
    commits = []
    for rec in filter(None, (r.strip("\n") for r in raw.split("\x1e"))):
        parts = rec.split("\x1f", 3)
        if len(parts) != 4:
            commits.append({"error": f"Malformed commit record: {rec[:50]}"})
            continue
        sha, name, email, msg = parts
        commits.append({"sha": sha, "author_name": name, "author_email": email, "message": msg})
    return commits


def check_title(title):
    m = TITLE_RE.match(title.strip())
    if not m:
        return [f"PR title '{title}' must look like 'type(scope): subject', e.g. 'fix(git): handle detached HEAD'."]
    errors = []
    if m["type"] not in TYPES:
        errors.append(f"Type '{m['type']}' is not one of: {', '.join(TYPES)}.")
    if m["scope"] not in PLUGINS + OTHER_SCOPES:
        errors.append(f"Scope '{m['scope']}' is not one of: {', '.join(PLUGINS + OTHER_SCOPES)}.")
    return errors


def touched_plugins(paths):
    return sorted({p.split("/", 1)[0] for p in paths if "/" in p and p.split("/", 1)[0] in PLUGINS})


def is_deps_path(path):
    """True for dependency manifests and CI config: Cargo.lock, the root or a plugin Cargo.toml, .github/."""
    if path in ("Cargo.lock", "Cargo.toml") or path.startswith(".github/"):
        return True
    parts = path.split("/")
    return len(parts) == 2 and parts[0] in PLUGINS and parts[1] == "Cargo.toml"


def check_scope(title, paths):
    m = TITLE_RE.match(title.strip())
    if not m:
        return []
    if m["scope"] == "deps":
        # deps may span plugins, but only while it touches nothing but dependency manifests.
        offending = [p for p in paths if not is_deps_path(p)]
        if offending:
            return [f"Scope 'deps' is for dependency manifests only; this PR changes {offending[0]}. "
                    "Use the plugin's scope."]
        return []
    scope = m["scope"]
    plugins = touched_plugins(paths)

    if len(plugins) > 1:
        return [f"This PR changes {len(plugins)} plugins ({', '.join(plugins)}). Each plugin is released "
                "from its own commit scope, so split it into one PR per plugin."]

    # If scope is a plugin name, it must touch that plugin's directory.
    if scope in PLUGINS:
        if scope not in plugins:
            # The scope names a plugin, but no files under that plugin's directory.
            if plugins:
                # Files touched in a different plugin.
                p = plugins[0]
                return [f"This PR changes '{p}/' but its scope is '{scope}'. Use '{p}' as the scope so the change releases {p}."]
            else:
                # No plugin files touched at all.
                return [f"Scope '{scope}' releases {scope}, but this PR changes nothing under '{scope}/'. "
                        "Use a non-plugin scope (ci, docs, repo, test-support)."]
        # scope is in plugins, so we're good.
        return []

    # If we touched a plugin, scope must match.
    if plugins and scope != plugins[0]:
        p = plugins[0]
        return [f"This PR changes '{p}/' but its scope is '{scope}'. Use '{p}' as the scope so the change releases {p}."]

    return []


def check_dco(commits, pr_author=None):
    errors = []
    bot_pr = pr_author in BOTS
    for c in commits:
        if "error" in c:
            errors.append(c["error"])
            continue
        # Exempt bot commits only in a PR opened by a bot, and only if the author name is in BOTS
        # AND the email ends with [bot]@users.noreply.github.com. Author name and email are
        # self-asserted, so a human PR never gets the exemption.
        author_name = c["author_name"]
        author_email = c["author_email"]
        if bot_pr and author_name in BOTS and author_email.endswith("[bot]@users.noreply.github.com"):
            continue
        signed = {m["email"].lower() for m in SIGNOFF_RE.finditer(c["message"])}
        if author_email.lower() not in signed:
            errors.append(f"Commit {c['sha'][:7]} has no 'Signed-off-by: {author_name} <{author_email}>' "
                          "line. Add it with 'git commit --amend -s' or 'git rebase --signoff <base>', then force-push.")
    return errors


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True, errors="replace").stdout


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--title", required=True)
    ap.add_argument("--base", required=True)
    ap.add_argument("--head", required=True)
    ap.add_argument("--pr-author", default=None, help="Login of the PR author; bot DCO exemption needs a bot PR.")
    a = ap.parse_args()

    # Use -z to handle special characters and renames.
    z_output = git("diff", "-z", "--name-only", "--no-renames", f"{a.base}...{a.head}")
    paths = parse_paths(z_output)

    # Use --no-merges to exclude "Update branch" merge commits.
    raw = git("log", "--no-merges", "--format=%H%x1f%an%x1f%ae%x1f%B%x1e", f"{a.base}..{a.head}")
    commits = parse_log(raw)

    errors = check_title(a.title) + check_scope(a.title, paths) + check_dco(commits, a.pr_author)
    for e in errors:
        print(f"::error::{escape(e)}")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
