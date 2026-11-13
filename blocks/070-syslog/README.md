# 070 — UDP Syslog Remote Diagnostic Streaming

**Status:** ❌ not possible yet
**ESPHome lego:** `syslog:`

## ESPHome

```yaml
syslog:
  ip_address: "192.168.1.50"
  port: 514
  facility: LOCAL0
  log_level: DEBUG
```

The logger writes every record twice: to the console as before, and as a single
UDP datagram to a collector. Syslog packets carry a priority (`facility * 8 +
severity`), a timestamp, the hostname and the message, in RFC 3164 or RFC 5424
form.

## espforge

`syslog.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `syslog:` service | proposed `<syslog id="collector" network="$home" host="192.168.1.50" port="514" />` |
| `facility: LOCAL0` | priority `16 * 8 + severity` in the datagram |
| `log_level: DEBUG` | `<logger level="debug" />` is real, but it filters the console logger only |
| the second sink | none: no logger hook, so a record cannot be duplicated |
| UDP | none: the runtime has TCP clients only |
| `ip_address:` | a literal, or `host="logs.local"` for the DNS path |

Two gaps: a UDP socket, and a logger sink that can be added next to
`esp-println`. The `<logger>` element already owns filtering, so a remote sink
is a natural extension of it rather than a separate service.

### What is missing

A UDP socket API and a multi-sink logger. Blocks 080 (SNMP) and 089 (ICMP ping)
need the same socket layer.
