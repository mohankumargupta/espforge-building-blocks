# 003 — Native API Transport

**Status:** ❌ not possible yet
**ESPHome lego:** `api:` (the native client protocol)

## ESPHome

```yaml
api:
  encryption:
    key: "base64-encoded-key"
```

ESPHome's own binary protocol (protobuf over TCP) for hub discovery, state
streaming and commands. Entities declared anywhere in the YAML become visible to
the client without extra code.

## espforge

`native-api.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `api: encryption.key` | proposed `<api id="api" network="$home" encryption.key="..." />` service |
| entities auto-exposed | would be generated from `# devices` — espforge has no entity registry yet |
| — | today's network vocabulary is client-only: `http_client`, `https_client`, `ws_client`, `wss_client`, `ntp` all ride on a `network`-providing service |

The file uses a dotted attribute (`encryption.key`) to mirror ESPHome's nested
YAML, so the translation needs no new value shapes — only a new element and a
generated server.

### What is missing

Everything server-side: an `<api>` service, a discovery/connection runtime, and
a mapping from declared devices to exposed state. `src/app.rs` is unchanged from
block 002 because exposing entities is the compiler's job, not the app's; that
is the design point this block makes.
