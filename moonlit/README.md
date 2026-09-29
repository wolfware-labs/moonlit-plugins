# moonlit-plugin-moonlit

First-party Moonlit plugin. Runs nested Moonlit release files as modules by
shelling out to the `moonlit` CLI (`exec` permission `["moonlit"]`).

## Use

```yaml
plugins:
  - name: moonlit
    url: "oci://registry.moonlit.rs/wolfware/moonlit:0.1.0"
    permissions:
      exec: ["moonlit"]
      filesystem: read-write
```

`run-modules` and `publish-plugin` spawn the `moonlit` CLI. `publish-plugin` stamps `Cargo.toml` and builds into the working directory, hence `filesystem: read-write`.

Released independently; see the [releases](https://github.com/wolfware-labs/moonlit-plugins/releases?q=moonlit-v) for versions.

## Config

No plugin-level config. No shared state.

## Middlewares

| Middleware | Config | Outputs | Behavior |
|---|---|---|---|
| `run-modules` | `modulePaths` (req, non-empty), `stages`, `continueOnModuleError` (default `false`), `arguments` | `results` (array of `{module, successful, durationMs}`), `failedCount` | Spawns `moonlit run -w <dir> [-f <file>] --output plain [-s <stage>]* [-a k=v]*` once per module path. |

`run-modules` invokes `moonlit run -w`, so it requires Moonlit CLI 2.x. CLI
3.0.0 removed `-w`.

### `modulePaths`

Each entry is either a directory (mapped to `-w <dir>` alone) or a `.yml`/
`.yaml` file (case-insensitive; split into `-w <parent-dir>` and
`-f <basename>`, with `-w .` when the file has no parent directory). An
empty `modulePaths` fails before any subprocess spawn with
`No module paths provided for run-modules.`

### `stages` and `arguments`

`stages` becomes one `-s <stage>` flag per entry, in order. `arguments` is a
map and becomes one `-a key=value` flag per entry, in sorted key order.

## Outputs

`results` is a JSON array with one entry per module attempted, each
`{module, successful, durationMs}`. `failedCount` is the number of modules
that did not succeed.

## Permissions

`exec: ["moonlit"]` and `filesystem: read-write`. Both `run-modules` and `publish-plugin` spawn the
`moonlit` CLI; `publish-plugin` also stamps `Cargo.toml` and builds in the working
directory, which needs the filesystem grant.

## Failure semantics

Two modes, controlled by `continueOnModuleError`:

- **Fail-fast (default, `false`)** — the first module that fails stops the
  run immediately; the middleware fails with
  `Module '<path>' failed with exit code <code>.` Later modules are never
  spawned.
- **Continue (`true`)** — every module runs regardless of prior failures.
  The middleware succeeds overall; per-module outcomes are reported in
  `results`, and the count of failed modules is reported in `failedCount`.

## publish-plugin

Stamps a version into a plugin crate's `Cargo.toml`, builds the component,
checks that the built component reports that version, and pushes it to an OCI
registry. Registry versions are immutable, so every check runs before the push.

| Input | Type | Default | Description |
|---|---|---|---|
| `manifestPath` | string (required) | | Crate directory holding the plugin's `Cargo.toml`, relative to the working directory. |
| `version` | string (required) | | Version to write into `Cargo.toml` and publish (`MAJOR.MINOR.PATCH[-PRERELEASE]`). |
| `reference` | string (required) | | OCI reference without a tag, e.g. `oci://registry.moonlit.rs/wolfware/git`. |
| `targetDir` | string | `target` | Cargo target directory, relative to the working directory. |
| `skipIfPublished` | bool | `true` | Succeed without pushing when `<reference>:<version>` already exists. |

Outputs:

- `reference`: the full reference, `<reference>:<version>`.
- `digest`: manifest digest reported by the registry. Absent when the push was skipped.
- `alreadyPublished`: `true` when the version was already in the registry and nothing was pushed.

Permissions: `exec: ["moonlit"]`, `filesystem: read-write`.

Ordering: the version is written to `Cargo.toml` and the component is built
before the registry check, so a re-run after a failed commit still produces the
bump and still builds, which updates `Cargo.lock`. Only the push is skipped
when the version is already published. A registry inspect that exits non-zero
counts as "not published"; one that exits 0 with unusable output fails the run.

## Build

    cargo build -p moonlit-plugin-moonlit --target wasm32-wasip2 --release

Requires: `rustup target add wasm32-wasip2`. Verify with
`moonlit plugin inspect target/wasm32-wasip2/release/moonlit.wasm`.
