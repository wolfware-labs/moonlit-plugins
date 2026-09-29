# moonlit-plugin-gitlab — Moonlit first-party gitlab plugin

Three middlewares — `related-items`, `create-release`, `write-variables` — built
to a `wasm32-wasip2` component over the GitLab REST API v4.

## Use

```yaml
plugins:
  - name: gitlab
    url: "oci://registry.moonlit.rs/wolfware/gitlab:0.1.0"
    config:
      token: $(GITLAB_TOKEN)
    permissions:
      network: ["gitlab.com"]
      exec: ["git"]
      filesystem: read-write
```

`related-items` and `create-release` run `git remote get-url origin`; `write-variables` appends to a file in the working directory (`filesystem: read-write`). For a self-hosted instance, add its host to `network`.

Released independently; see the [releases](https://github.com/wolfware-labs/moonlit-plugins/releases?q=gitlab-v) for versions.

## Build

    cargo build -p moonlit-plugin-gitlab --target wasm32-wasip2 --release

Requires: `rustup target add wasm32-wasip2`. Verify with
`moonlit plugin inspect target/wasm32-wasip2/release/gitlab.wasm`.
