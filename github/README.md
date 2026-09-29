# moonlit-plugin-github — Moonlit first-party github plugin

Four middlewares — `related-items`, `create-release`, `commit-files`, `write-variables` — built
to a `wasm32-wasip2` component over the GitHub REST API.

## Use

```yaml
plugins:
  - name: github
    url: "oci://registry.moonlit.rs/wolfware/github:0.1.0"
    config:
      token: $(GITHUB_TOKEN)
    permissions:
      network: ["api.github.com", "*.github.com"]
      exec: ["git", "sh"]
      env: ["GITHUB_*"]
      filesystem: read-only
```

Every middleware runs `git remote get-url origin`; `commit-files` also runs `git rev-parse HEAD` when `expectedHeadSha` is unset; `write-variables` also runs `sh` and reads `$GITHUB_OUTPUT` / `$GITHUB_ENV` (`env: ["GITHUB_*"]`). `commit-files` reads working-tree files (`filesystem: read-only`) and calls the GraphQL API under `api.github.com`.

Released independently; see the [releases](https://github.com/wolfware-labs/moonlit-plugins/releases?q=github-v) for versions.

## Build

    cargo build -p moonlit-plugin-github --target wasm32-wasip2 --release

Requires: `rustup target add wasm32-wasip2`. Verify with
`moonlit plugin inspect target/wasm32-wasip2/release/github.wasm`.

## `create-release`

Creates a GitHub Release, then comments on and labels related PRs/issues.

| Input | Type | Description |
| --- | --- | --- |
| `name` | string, required | Release name/title. |
| `tag` | string, required | Git tag the release points at. |
| `targetCommitish` | string | Commit SHA or branch the tag is created at when it does not exist yet. Defaults to the repository's default branch. Sent as `target_commitish` only when set. |
| `body` | string | Release body markdown. Overrides the rendered changelog. |
| `changelog` | category[] | Structured changelog rendered into the body when `body` is unset. |
| `label` | string | Label added to related PRs/issues. |
| `draft` | bool | Create as a draft. Defaults to false. |
| `prerelease` | bool | Mark as a prerelease. Defaults to false. |
| `pullRequests` | `{number}`[] | Pull requests to comment on and label. |
| `issues` | `{number}`[] | Issues to comment on and label. |

Output: `name`, `url`.

## `commit-files`

Commits working-tree files to a branch through GitHub's GraphQL
`createCommitOnBranch` mutation. GitHub signs commits made through the API with
an App token, so they satisfy "require signed commits" with no signing key in CI.

| Input | Type | Description |
| --- | --- | --- |
| `branch` | string, required | Branch to commit to. |
| `message` | string, required | Commit message; the first line is the headline, the rest the body. |
| `files` | string[], required | Repository-relative paths whose working-tree contents are committed. Must not leave the repository. |
| `expectedHeadSha` | string | Commit the branch must still point at. Defaults to the local `HEAD`. |

Output: `sha`, `url` of the new commit.

Permissions: `network: ["api.github.com"]`, `exec: ["git"]`, `filesystem: read-only`.
