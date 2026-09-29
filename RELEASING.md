# Releasing

This is the maintainer runbook. Contributors never release by hand; see
[CONTRIBUTING.md](CONTRIBUTING.md).

## 1. How releases work

Every plugin is versioned and released independently, from `main`, by
`.github/workflows/release.yml`. It runs a matrix over the 9 plugins with
`max-parallel: 1` and is gated on the repository variable
`RELEASE_ENABLED == 'true'`. Each job runs the pipeline in
`.moonlit/plugin-release.yml` for one plugin `<plugin>`:

1. `git.repo-context` reads the repository context.
2. `git.latest-tag` finds the latest tag with prefix `<plugin>-v`.
3. `git.commits` lists the commits since that tag.
4. `sr.analyze` classifies them, with `includeScopes: [<plugin>]`.
5. `sr.calculate-version` computes the next version. It uses
   `breaking_changes_always_major` (the default), so a breaking change (`!` or
   `BREAKING CHANGE:`) always bumps the major version: a plugin at 0.x jumps to
   1.0.0, not to 0.x+1.
6. `sr.generate-changelog` builds the release notes.
7. `moonlit.publish-plugin` stamps `Cargo.toml`, builds, verifies the local
   version, checks the registry (skipping the push if the version is already
   there, with a warning) and pushes. Reruns therefore always rebuild and keep
   `Cargo.lock` in sync. A skipped push means the registry copy was built from
   an earlier commit; see §3.
8. `gh.commit-files` commits `<plugin>/Cargo.toml` and `Cargo.lock` as
   `chore(release): <plugin> <version>`.
9. `gh.create-release` creates the tag `<plugin>-v<version>` at that commit's SHA.

The commit-back and the release are made through the release GitHub App
(`vars.RELEASE_APP_ID`, `secrets.RELEASE_APP_PRIVATE_KEY`); the registry push
uses `secrets.MOONLIT_TOKEN`. All three live in the `release` environment, which
only `main` may deploy to, and the job also runs only when `github.ref` is
`refs/heads/main`. The App token is minted with Contents: write only, right
before the release step. The App token reaches the pipeline as
`MOONLIT_GITHUB_TOKEN`: Moonlit exposes only `MOONLIT_*` environment variables,
with the prefix stripped, so the pipeline reads it as `$(GITHUB_TOKEN)`. Commits
made through the App's API are signed by GitHub, so `main-integrity` (which
requires signatures and has no bypass actors) holds. The App bypasses only
`main-protection` (review and checks).

Release commits are not skipped: each one triggers a run that finds nothing to
do. This is deliberate, because the `release-main` concurrency group keeps only
the newest pending run, and every run rescans all plugins from the latest `main`,
so whichever run survives still covers every earlier push.

`main-protection` requires status checks to pass on an up-to-date branch
(strict), so every release commit makes open PRs out of date: they need
"Update branch" and a fresh CI run before merging. This is accepted.

### Security notes

- `moonlit plugin build` runs dependency build scripts and proc-macros in a
  process that inherits the App token. Dependency changes (Dependabot PRs
  included) therefore deserve careful review.
- The workflow pins the Moonlit CLI to an exact version and disables the plugin
  cache. Bump the CLI pin deliberately, after validating the pipeline against
  the new version.

### Dependency-only fixes

Dependabot `build(deps)` commits never trigger a plugin release, because they
carry no plugin scope. To ship a dependency fix, merge a follow-up
`fix(<plugin>): …` PR that touches that plugin (for example a comment, or the
fix itself).

## 2. One-time bootstrap

### Before the first publish

Two open items to settle before publishing 0.1.0:

- (a) Confirm that registry.moonlit.rs rejects a re-push of an existing version
  with a different digest. The Moonlit CLI does not check this client-side.
- (b) Known issue: published artifacts carry an empty `sdkVersion` annotation,
  because `moonlit plugin publish --manifest-path <crate>` reads
  `<crate>/Cargo.lock` while this workspace keeps its lock at the root. This is
  a core CLI issue.

The workflows pin Moonlit CLI 2.0.0. 3.0.0 removes `-w` and runs pipelines from
the manifest's directory; migrating needs a decision (move the pipeline to the
repo root or a core working-directory option).

### Steps

1. Repository settings: allow squash merging only, and set the default squash
   message to "Pull request title and commit details" (this keeps sign-offs).
2. From a clean, up-to-date `main`:

   ```sh
   cargo build --workspace --target wasm32-wasip2 --release
   moonlit login registry.moonlit.rs
   ```

3. For each plugin `p` (docker, dotnet, git, github, gitlab, moonlit, nodejs,
   semantic-release, slack), with `<lib>` the library name, which equals `p`
   except `semantic-release` → `semantic_release`:

   ```sh
   moonlit plugin inspect target/wasm32-wasip2/release/<lib>.wasm   # version must be 0.1.0
   moonlit plugin publish oci://registry.moonlit.rs/wolfware/<p>:0.1.0 \
     --file target/wasm32-wasip2/release/<lib>.wasm --manifest-path <p>
   ```

4. Tag the same commit with signed tags and push them:

   ```sh
   plugins="docker dotnet git github gitlab moonlit nodejs semantic-release slack"
   for p in $plugins; do
     git tag -s "$p-v0.1.0" -m "$p 0.1.0"
   done
   for p in $plugins; do
     git push origin "refs/tags/$p-v0.1.0"
   done
   ```

   Push only these 9 tag names, never `--tags`, so no stray local tag reaches
   the repository. Push them before importing `release-tags.json` (step 7);
   once that ruleset is active, only the release App may create or change
   `*-v*` tags, and a maintainer would need a temporary bypass.

   The pipeline needs these tags. Without a `<plugin>-v0.1.0` tag,
   `$(output:tag:name)` resolves to the literal string `name` (a core expression
   quirk) and the version step fails.

5. Optional: create GitHub releases for the 0.1.0 tags.
6. Create the GitHub App with Contents: read & write and Metadata: read, and
   install it on this repository. Create the environment `release` (Settings →
   Environments) with deployment branches restricted to `main` only. Do not
   enable required reviewers on it: every matrix job, no-op runs included,
   would wait for approval (9 approvals per run) while holding the
   `release-main` concurrency group. In it, add the environment secrets
   `RELEASE_APP_PRIVATE_KEY` and `MOONLIT_TOKEN` (a registry token scoped to
   `wolfware`), and the environment variable `RELEASE_APP_ID`. Keeping them in
   the environment means a workflow run from any other branch, including a
   dispatch or an edited workflow, cannot read them.
7. Import the three rulesets (Settings → Rules → Rulesets → Import) from
   `.github/rulesets/`. **Before importing `main-protection.json` and
   `release-tags.json`, replace `"actor_id": 0` in the `Integration` bypass
   actor with the release App's ID.** `actor_id: 5` is the built-in Admin role,
   which may merge its own PRs through a pull request only.
   `main-integrity.json` has no bypass actors and must stay that way.
   `release-tags.json` blocks deleting or moving `*-v*` tags for everyone but
   the release App; import it only after the step 4 tags are pushed (or give
   the maintainer a temporary bypass). The required checks in
   `main-protection.json` are pinned to the GitHub Actions app
   (`integration_id` 15368), so no other app can satisfy them.
8. Enable private vulnerability reporting (Settings → Code security). SECURITY.md
   and the issue template link to it.
9. Set the repository variable (not an environment variable, since the job's
   `if:` is evaluated before the environment applies) `RELEASE_ENABLED=true`.

## 3. Recovering a failed release

Re-run the failed matrix job. `publish-plugin` rebuilds and skips the push if the
version is already in the registry, so a rerun never overwrites a published
version. If `main` moved in the meantime, the re-run picks up the new head.

Whenever the push is skipped, `publish-plugin` logs a warning: the registry copy
was built from an earlier commit, and any commits merged since the failed
release are not in it. Always read that warning (see below).

### The release commit landed, but the release failed

If the `chore(release)` commit landed but `create-release` failed, the follow-up
run (the one the release commit itself triggered) recovers automatically. It
recomputes the same version, skips the push, commits the identical
`Cargo.toml`/`Cargo.lock` content as an empty `chore(release)` commit, and
creates the tag and release at that commit. The empty commit is expected. A
manual re-run of the failed job does the same and adds another empty commit.

If the follow-up run did not create the tag either, create the tag and release
by hand at the existing `chore(release)` commit instead of re-running:

```sh
gh release create <plugin>-v<version> --target <sha> --title "<plugin> <version>" --generate-notes
```

### A job failed at `commit-files` because `main` moved

If `main` moves between a job's checkout and its commit step, that job fails
after publishing. The next run recomputes the version from the commits since
the last tag:

- If a scoped commit for the same plugin landed in between without raising the
  bump level, the next run reuses the same version, skips the push, and tags a
  commit whose code the registry artifact lacks. The published artifact is the
  one built from the older commit; the new scoped commits are released under a
  tag but not in the registry.
- If the intervening commits raise the bump level, the next run publishes a new
  version, and the first published version stays in the registry untagged.

Remedy: check the skip warning in the recovery run's `publish-plugin` step and
compare the commits since the previous tag with the commit the failed job
built. If new scoped commits were included, merge a follow-up
`fix(<plugin>): …` PR so the next patch release publishes them.

## 4. Bumping the pipeline's own plugin pins

`.moonlit/plugin-release.yml` pins the plugin versions it uses. After a plugin
release, edit the pin in a `ci(ci): ...` PR.
