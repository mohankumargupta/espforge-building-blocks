# 089 — ICMP Network Ping & Host Presence Watchdog

**Status:** ❌ not possible yet
**ESPHome lego:** `binary_sensor: platform: ping`

## ESPHome

```yaml
binary_sensor:
  - platform: ping
    ip_address: "192.168.1.1"
    name: "Default Gateway Reachable"
    timeout: 1s
    interval: 10s
```

A ping is a raw ICMP echo request: a small IP packet with a checksummed header
and a sequence number, sent straight to the host, with the reply measured for
round-trip time. It is the cheapest way to answer "is this device on the
network", because it needs nothing on the other side.

## espforge

`ping.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `binary_sensor: platform: ping` | proposed `<ping id="gateway" network="$home" host="192.168.1.1" />` |
| `timeout: 1s` | proposed `timeout_ms="1000"` |
| `interval: 10s` | `PROBE_MS = 10_000` |
| RTT | none: no ICMP socket, so no timestamp to subtract |
| `name:` | none; reachability is logged |

embassy-net can carry raw sockets, but the runtime exposes TCP clients only —
`<http_client>` and its TLS variants. An ICMP socket also needs a checksum
implementation and an identifier that does not collide with the stack's own.

### What is missing

A raw-socket or ICMP API on the network abstraction, plus a `<ping>` binary
sensor. The `<http_client>` route (`GET /` and see if it answers) is the closest
available substitute, and it is not the same thing: many hosts drop HTTP but
answer ping.
