# moonlit-plugin-nodejs — Moonlit first-party nodejs plugin

Six middlewares — `install`, `run-script`, `build`, `pack`, `push`, `test` — built to a
`wasm32-wasip2` component. Shells out to the `npm` CLI. See [Use](#use) for the permissions to grant.

## Use

```yaml
plugins:
  - name: nodejs
    url: "oci://registry.moonlit.rs/wolfware/nodejs:0.1.0"
    config:
      token: $(NPM_TOKEN)
    permissions:
      exec: ["npm"]
      filesystem: read-write
```

`pack` writes a tarball and `push` writes a scoped `.npmrc` (removed afterwards) under the working directory, hence `filesystem: read-write`.

Released independently; see the [releases](https://github.com/wolfware-labs/moonlit-plugins/releases?q=nodejs-v) for versions.

## Build

    cargo build -p moonlit-plugin-nodejs --target wasm32-wasip2 --release

Requires: `rustup target add wasm32-wasip2`. Verify with
`moonlit plugin inspect target/wasm32-wasip2/release/nodejs.wasm`.

## Design notes

- **npm only** — the `packageManager` field (yarn/pnpm) is not consulted (MVP scope).
- **`package.json` pre-flight** — `install`/`run-script`/`build`/`pack`/`test` fail with
  `package.json not found in directory: <dir>` before invoking npm, giving a branded
  message and a deterministic offline failure path.
- **`pack` output** — the tarball is written to `.moonlit/npm-pack/` under the middleware
  `directory` (wiped per run), or an explicit `destination` (created if missing, not
  wiped). The emitted `packagePath` is **working-dir-relative** so it chains into `push`.
  A wasm component has no clock and can only read files inside its preopen, so the
  spec's `<tmp>/npm-packs/<name>/<timestamp>/` default is replaced by this scheme.
  `destination` is interpreted as a working-dir-relative path; an absolute path is out
  of contract under the wasm filesystem sandbox.
- **`push` auth** — the token is written to a scoped `.npmrc` under `.moonlit/npm-push/`
  (wiped per run) and passed via `--userconfig`; it never appears on the argv, so it is
  not visible in the host process listing (`/proc/<pid>/cmdline`). A blank token (local
  and plugin config) fails fast with a branded message. Non-zero exits classify to
  `Version already published.` (`EPUBLISHCONFLICT` / cannot-publish-over) or an
  authentication error (`401 (`/`403 (`, `E401`, `ENEEDAUTH`, unauthorized/forbidden);
  everything else is a generic exit-code failure. The `.npmrc` is removed immediately after
  the publish attempt (success or failure), so the token does not linger in the working tree.
  On a native/Unix run it is created owner-only (`0o600`); on the `wasm32-wasip2` artifact,
  WASI has no Unix mode bits, so the file's permissions are set by the **host** — operators
  on shared machines should run the engine with a restrictive `umask` and keep
  `.moonlit/npm-push/` unreadable to other users for the duration of a publish.
- **`test`** implements the spec's optional test stretch, mirroring `dotnet.test`.
