# moonlit-plugin-git — Moonlit first-party git plugin

Five middlewares — `repo-context`, `latest-tag`, `commits`, `tag`, `push` — built
to a `wasm32-wasip2` component that shells out to the `git` CLI.

## Use

```yaml
plugins:
  - name: git
    url: "oci://registry.moonlit.rs/wolfware/git:0.1.0"
    permissions:
      exec: ["git"]
```

The plugin spawns only `git`.

Released independently; see the [releases](https://github.com/wolfware-labs/moonlit-plugins/releases?q=git-v) for versions.

## Build

    cargo build -p moonlit-plugin-git --target wasm32-wasip2 --release

Requires: `rustup target add wasm32-wasip2`. Verify with
`moonlit plugin inspect target/wasm32-wasip2/release/git.wasm`.

## Middlewares

| Middleware | Purpose |
| --- | --- |
| `repo-context` | Current branch and the `origin` remote URL. |
| `latest-tag` | Newest tag matching a pattern; stores its commit SHA for `commits`. |
| `commits` | Commits in a range, newest first. |
| `tag` | Create a lightweight or annotated tag. |
| `push` | Push the current branch, and optionally tags, to a remote. |

Every middleware fails when the working directory is not inside a git repository.

## `repo-context`

Reports the current branch and the `origin` remote URL. Takes no input.

Output: `branch`, `remoteUrl`.

## `latest-tag`

Finds the newest tag matching a pattern and remembers its commit SHA so a later `commits` step can start after it.

| Input | Type | Description |
| --- | --- | --- |
| `prefix` | string | Text stripped from the front of a matched tag before the version (e.g. `v`). |
| `suffix` | string | Text stripped from the end of a matched tag after the version. |
| `pattern` | string | Regex the tag core (between prefix and suffix) must match. Defaults to a semver `X.Y.Z` pattern. |

Output: `name`, `fullName`, `commitSha`. All are absent, with a warning, when no tag matched.

## `commits`

Lists commits in a range.

| Input | Type | Description |
| --- | --- | --- |
| `sinceSha` | string | Commit SHA to start after (exclusive). Takes precedence over `since` and the shared context. |
| `useSharedContext` | bool | When no explicit boundary is set, start after the tag found by the `latest-tag` step. Defaults to true. |
| `since` | string | Git revision to start after (exclusive), resolved to a commit (e.g. a tag or branch name). |
| `until` | string | Git revision to stop at (inclusive). Defaults to `HEAD`. |

Output: `details` (newest first; each `sha`, `author`, `email`, `date`, `message`) and `count`.

## `tag`

Creates a tag. An existing tag of the same name is left alone with a warning.

| Input | Type | Description |
| --- | --- | --- |
| `tagName` | string, required | Name of the tag to create (e.g. `v1.2.0`). |
| `message` | string | Annotation message. When set, creates an annotated tag instead of a lightweight one. |

No output.

## `push`

Pushes the current branch, then optionally the tags.

| Input | Type | Description |
| --- | --- | --- |
| `remote` | string | Name of the remote to push to. Defaults to `origin`. |
| `pushTags` | bool | Also push tags after the branch. Defaults to true. |

No output.
