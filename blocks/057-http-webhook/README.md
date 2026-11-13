# 057 — Outbound HTTP Client & Webhook Telemetry

**Status:** ✅ possible today
**ESPHome lego:** `http_request:` + `on_press: http_request.post`

## ESPHome

```yaml
http_request:
  useragent: "espforge-node/1.0"
  timeout: 5s

binary_sensor:
  - platform: gpio
    pin: GPIO3
    name: "Panic Button"
    on_press:
      then:
        - http_request.post:
            url: "https://webhook.site/alert-endpoint"
            json:
              event: "button_pressed"
              node: "office_node"
```

An event-driven outbound POST: the request is built on the stack, sent over an
existing TLS/TCP connection, and the response is discarded. Nothing listens on
the node.

## espforge

`http-webhook.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `http_request:` | `<http_client id="webhook" network="$home" rx_buffer_size="1024" tx_buffer_size="512" timeout_ms="5000" />` |
| `useragent:` | no attribute: the runtime sets its own header |
| `timeout: 5s` | `timeout_ms="5000"` |
| `url:` | `ENDPOINT` in `app.rs`, passed to `post` |
| `json:` | a hand-written `&[u8]` body with `Content-Type: application/json` |
| `binary_sensor: platform: gpio` | `<button id="panic_button" pin="3" pull_up="true" />` |
| `on_press:` | an edge check on `is_pressed()` in `forever` |
| `https://` | swap in `<https_client>`; the call is the same |

The request body is a fixed byte string rather than a serialiser, which is the
honest trade: `serde_json` costs flash the panic button does not need. The
client's `tx_buffer_size` must exceed the body, and the response buffer has to
exist even though the webhook's reply is ignored.

### Notes

- `post` returns the response body; this block asks for one byte of it, just
  enough to keep the connection state sane.
- Retries are the app's: a second press during an in-flight POST is dropped by
  the single connection slot. Raise `connections="2"` if that matters.
