# Moonlit Plugins

First-party plugins for [Moonlit](https://github.com/wolfware-labs/moonlit),
built as `wasm32-wasip2` WebAssembly components on
[`moonlit-sdk`](https://crates.io/crates/moonlit-sdk).

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

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
