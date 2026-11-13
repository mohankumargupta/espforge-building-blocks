# 079 — Multicast DNS (mDNS) & Custom ZeroConf Discovery

**Status:** ❌ not possible yet
**ESPHome lego:** `mdns:`

## ESPHome

```yaml
mdns:
  disabled: false
  services:
    - service: "_custom_sensor"
      protocol: "_tcp"
      port: 8080
      txt:
        version: "1.0.0"
        chip: "esp32c3"
```

mDNS puts a `.local` name on the link and answers queries for it on
`224.0.0.251:5353` without a DNS server. DNS-SD goes further: `_services._dns-sd`
enumerates the service types a device offers, and each instance publishes a
PTR, an SRV (host and port) and a TXT record (free-form key/value).

## espforge

`mdns.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `mdns: disabled: false` | proposed `<mdns id="names" network="$home" hostname="sensor-node" />` service |
| `name:` from `esphome:` | the hostname would default to `<project name>` |
| `services: service / protocol / port` | proposed `<mdns_service service="_custom_sensor" protocol="_tcp" port="8080" />` |
| `txt: version / chip` | proposed `txt="version=1.0.0,chip=esp32c3"` |
| multicast UDP | none: the runtime has unicast TCP clients |
| the `.local` responder | none: no DNS packet encoder |

The stack underneath (embassy-net + smoltcp) does have multicast and UDP, so
this is runtime work rather than silicon: a UDP socket bound to 5353, joined to
the multicast group, plus a DNS message encoder.

### What is missing

A UDP multicast socket, an mDNS responder and a DNS-SD service registry. Blocks
070 and 080 need the same socket layer.
