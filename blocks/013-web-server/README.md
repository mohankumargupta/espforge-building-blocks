# 013 — Embedded Web Server & Local REST API

**Status:** ❌ not possible yet
**ESPHome lego:** `web_server:`

## ESPHome

```yaml
web_server:
  port: 80
  version: 2
  include_internal: false
```

The device serves a small UI and JSON endpoints for its entities, so a browser
can read state and send commands without Home Assistant in the middle.

## espforge

`web-server.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `web_server.port` / `version` | proposed `<web_server id="web" network="$home" port="80" version="2" />` service |
| `include_internal: false` | proposed attribute: filtering needs entity metadata that does not exist |
| static assets | none: no filesystem. A generated server would embed its HTML/JS at compile time |
| REST/JSON per entity | no entity registry: `<led>` and friends are fields on `Context`, not published entities |
| `web_server` + networking | today's network vocabulary is client-only: `http_client`, `https_client`, `ws_client`, `wss_client`, `ntp` |

The app waits for Wi-Fi and idles: serving is a runtime task, and the compiler
would generate the routes from `# devices`. That is the same division block 003
makes for the native API; the difference is HTTP and JSON instead of protobuf
over TCP.

### What is missing

Everything server-side: a listener, request routing, a JSON serializer, and the
entity registry. The open design question is where routes come from — generated
from devices like ESPHome, or hand-written handlers in `app.rs`.
