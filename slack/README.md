# moonlit-plugin-slack

First-party Moonlit plugin. Posts messages to Slack via the Web API
(`chat.postMessage`) over the host HTTP capability.

## Use

```yaml
plugins:
  - name: slack
    url: "oci://registry.moonlit.rs/wolfware/slack:0.1.0"
    config:
      token: $(SLACK_TOKEN)
    permissions:
      network: ["slack.com"]
```

The plugin calls the Slack Web API.

Released independently; see the [releases](https://github.com/wolfware-labs/moonlit-plugins/releases?q=slack-v) for versions.

## Config

Plugin-level: `token` (required) — a Slack bot/user OAuth token. Missing or
blank fails at init with `Slack API token is required.`

## Middlewares

| Middleware | Config | Outputs | Behavior |
|---|---|---|---|
| `send-notification` | `channel` (req), `message` (req) | — | `POST chat.postMessage {channel, text}`. Blank channel/message fail before any request; a Slack `ok:false` response fails with the Slack error code. |

## Permissions

`network: ["slack.com"]`

## Build

    cargo build -p moonlit-plugin-slack --target wasm32-wasip2 --release

Requires: `rustup target add wasm32-wasip2`. Verify with
`moonlit plugin inspect target/wasm32-wasip2/release/slack.wasm`.

## Design notes

- **Always-200 API** — Slack returns HTTP 200 even for logical failures,
  encoding the outcome in a JSON `ok` field. The client parses the body and
  branches on `ok`, surfacing the Slack `error` code on failure.
- **No state, no outputs** — `send-notification` is fire-and-report; it holds
  no shared state and emits no output values.
