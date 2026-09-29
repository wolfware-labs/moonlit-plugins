# Contributing to Moonlit Plugins

Thanks for your interest in improving the Moonlit plugins.

## Ground rules

- Everyone taking part is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
- Report security issues privately, following [SECURITY.md](SECURITY.md). Never
  open a public issue for a vulnerability.
- Problems with the Moonlit CLI, the engine or the registry belong in the
  [core repository](https://github.com/wolfware-labs/moonlit/issues).

## Setup

Install [`rustup`](https://rustup.rs). The toolchain and the `wasm32-wasip2`
target are selected automatically by `rust-toolchain.toml`.

```sh
# Build every plugin as a WebAssembly component
cargo build --workspace --target wasm32-wasip2 --release

# Run the tests
cargo test --workspace

# Optional: end-to-end tests that drive the real Moonlit CLI
MOONLIT_BIN=/path/to/moonlit cargo test --workspace --features cli-e2e

# Self-tests of the PR guard
python3 -m unittest discover -s .github/scripts -p 'test_*.py'
```

Tests that create throwaway git repositories are isolated from your git
configuration by `.cargo/config.toml`. If you have commit or tag signing
enabled, you do not need to change anything.

## Tests expected in a PR

- Unit tests next to the code they cover.
- For every new middleware, an engine-level test in
  `<plugin>/tests/<plugin>_plugin.rs`.
- A `cli-e2e` test when behaviour visible through `moonlit run` changes.

## PR titles

PRs are squash-merged, so the PR title becomes the commit on `main`. The
`pr-guard` check enforces this format:

```
type(scope): subject
type(scope)!: subject      # breaking change
```

- **type**: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore`, `revert`
- **scope** (required), one of:
  - a plugin: `docker`, `dotnet`, `git`, `github`, `gitlab`, `moonlit`, `nodejs`, `semantic-release`, `slack`
  - or: `ci`, `deps`, `docs`, `repo`, `test-support`

**One plugin per PR.** A PR may change files under only one plugin directory,
and when the scope is a plugin it must be that plugin and the PR must change
something under its directory. The scope decides which plugin gets released.
The one exception is the `deps` scope, which may span plugins but is for
dependency manifests only: every changed file must be `Cargo.lock`, the root
`Cargo.toml`, a `<plugin>/Cargo.toml`, or under `.github/`. A PR that also
changes anything else (for example a plugin's `src/`) must use that plugin's
scope.

Mark breaking changes with `!` in the title and explain them in the PR
description; a `BREAKING CHANGE:` footer in the commit body works too. A
breaking change always bumps the major version, so on a plugin still at 0.x it
releases 1.0.0 (not 0.x+1).

Examples:

- `feat(github): add create-tag middleware`
- `fix(git): handle detached HEAD in latest-tag`
- `refactor(slack)!: rename the channel input to channel-id`
- `ci(ci): pin actions to commit SHAs`
- `docs(docs): clarify the release runbook`

## Developer Certificate of Origin (DCO)

All commits must be signed off under the
[Developer Certificate of Origin 1.1](https://developercertificate.org/). The
`Signed-off-by` trailer certifies that you wrote the work, or otherwise have the
right to submit it under the project's licenses:

    Signed-off-by: Your Name <you@example.com>

`git commit -s` adds it automatically. The email must match the commit author.
To fix a branch where commits are missing it:

```sh
git rebase --signoff main
git push --force-with-lease
```

Commits authored by `dependabot[bot]` or `github-actions[bot]` (with their
GitHub noreply email) are exempt, but only in a PR opened by that bot. A bot
commit inside a PR opened by a person still needs a sign-off.

## Merging

Only squash merging is allowed. The PR title is the commit message, and
maintainers may edit titles to make them conform before merging.

A PR must be up to date with `main` before it merges. Every release pushes a
`chore(release)` commit to `main`, so open PRs often need "Update branch" (and
a fresh CI run) before they can be merged.

## Releases

Releases are automatic: every merge to `main` releases the plugin named by the
commit scope, when the commit warrants a new version. Contributors never bump
versions or create or edit tags. See [RELEASING.md](RELEASING.md) for how it
works.

Dependabot `build(deps)` commits never release a plugin; to ship a dependency
fix, merge a follow-up `fix(<plugin>): …` PR that touches that plugin.

## Licensing

Moonlit Plugins is dual-licensed under the MIT license and the Apache License,
Version 2.0 (see [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE)).
Unless stated otherwise, contributions you submit are provided under the same
terms.

"Moonlit" is a trademark of Wolfware LLC. See [TRADEMARKS.md](TRADEMARKS.md)
and [NOTICE](NOTICE).
