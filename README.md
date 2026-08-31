# Moonlit Plugins

First-party plugins for [Moonlit](https://github.com/wolfware-labs/moonlit),
built as `wasm32-wasip2` WebAssembly components on
[`moonlit-pdk`](https://crates.io/crates/moonlit-pdk).

| Plugin | Purpose |
| --- | --- |
| `git` | Commit/tag/push and repo context |
| `github` / `gitlab` | Releases, variables, related items |
| `docker` | Build, login, push, deploy |
| `dotnet` / `nodejs` | Build, pack, test, publish |
| `semantic-release` | Conventional-commit versioning + changelog |
| `slack` | Notifications |
| `moonlit` | Run nested Moonlit modules |

Build a plugin: `cargo build --target wasm32-wasip2 --release` (requires
`rustup target add wasm32-wasip2`).

## Testing

`cargo test` runs each plugin's unit tests and its engine-driven integration
tests (the plugin is loaded as a WebAssembly component and exercised through
the engine).

End-to-end tests that drive the plugins through the `moonlit` CLI are gated
behind the `cli-e2e` feature so the default run needs no external binary. To
run them, provide a `moonlit` binary and enable the feature:

```
MOONLIT_BIN=/path/to/moonlit cargo test --features cli-e2e
```

Without `MOONLIT_BIN`, the tests look for `moonlit` on `PATH`.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
