# 080 — SNMP Management Agent & OID Telemetry Provider

**Status:** ❌ not possible yet
**ESPHome lego:** no `snmp:` element; this is the roadmap's extension

## ESPHome

```yaml
snmp:
  contact: "admin@myhome.net"
  location: "Rack 1"
  community: "public"
  objects:
    - oid: "1.3.6.1.4.1.53864.1.1.0"
      sensor: room_temperature
    - oid: "1.3.6.1.4.1.53864.1.2.0"
      sensor: battery_voltage
```

An SNMP agent answers `GET`/`GETNEXT` on UDP 161. Requests and responses are
ASN.1 BER-encoded: a sequence of a version, a community string and a
get-request with an object identifier. A monitoring system then polls the OID
tree on its own schedule.

## espforge

`snmp.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `snmp:` | proposed `<snmp id="agent" network="$home" community="public" />` service |
| `contact:`, `location:` | the standard `sysContact`/`sysLocation` OIDs, from attributes |
| `objects: oid / sensor` | proposed `<snmp_object oid="..." sensor="$temp" />` |
| UDP 161 | none: no UDP socket |
| BER encode/decode | none: no ASN.1 layer |
| MIB / OID tree walk | none: no ordered object table |

The sensor values it would export are real, but the protocol work is
substantial: a BER decoder, an ordered OID table for `GETNEXT`, and a UDP
server. None of it exists.

### What is missing

A UDP server socket, an ASN.1 BER codec, and an SNMP agent with a sortable OID
table. Block 089's ICMP ping and block 070's syslog need the same UDP layer.
