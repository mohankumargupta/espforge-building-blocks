# 069 — BLE iBeacon Proximity Broadcasting

**Status:** ❌ not possible yet
**ESPHome lego:** `esp32_ble_beacon:`

## ESPHome

```yaml
esp32_ble_beacon:
  type: iBeacon
  uuid: "e2c56db5-dffb-48d2-b060-d0f5a71096e0"
  major: 100
  minor: 1
  tx_power: -59
```

An iBeacon is a fixed advertising payload: the Apple company ID `0x004C`, type
`0x02`, length `0x15`, a 128-bit proximity UUID, a 16-bit major and minor, and
the calibrated RSSI at one metre. Nothing connects; receivers just listen.

## espforge

`ble-ibeacon.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp32_ble_beacon:` | proposed `<ble_beacon id="beacon" ble="$radio" kind="ibeacon" />` |
| `uuid:` | `UUID` in `app.rs`, split into its 16 bytes |
| `major: 100` / `minor: 1` | big-endian fields in the payload |
| `tx_power: -59` | the measured-at-1 m byte, not a radio setting |
| advertising interval | proposed `interval="100ms"`; 100 ms is the iBeacon convention |
| non-connectable advertising | none: no advertising API |
| `name:` | none: an iBeacon has no local name |

The payload is a fixed 25-byte array, so the only missing capability is the
radio: a `<ble>` peripheral in broadcaster mode that accepts a raw
advertisement.

### What is missing

The BLE controller in broadcaster mode, an advertising-data API, and a
`<ble_beacon>` element that composes the iBeacon or Eddystone payload.
