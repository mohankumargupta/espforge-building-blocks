# 026 — Bluetooth LE Scanner & Presence Tracking

**Status:** ❌ not possible yet
**ESPHome lego:** `esp32_ble_tracker:` + `binary_sensor: platform: ble_presence`

## ESPHome

```yaml
esp32_ble_tracker:

binary_sensor:
  - platform: ble_presence
    mac_address: "AA:BB:CC:DD:EE:FF"
    name: "Key Beacon Presence"
```

BLE presence tracking has two layers: a controller that scans the three
advertising channels, and a parser that turns each advertisement into a MAC,
an RSSI and optional payload data. A beacon is "present" while its
advertisements keep arriving.

## espforge

`ble-tracker.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp32_ble_tracker:` | proposed `<ble_tracker id="ble" radio="$radio" />` service |
| the BLE controller | proposed `<ble id="radio" />` peripheral, parallel to `<wifi>` |
| `binary_sensor: platform: ble_presence, mac_address` | proposed `<ble_presence id="key_beacon" mac="AA:BB:CC:DD:EE:FF" />` |
| advertisement parsing | none: no HCI/controller API and no payload parser |
| RSSI tracking | none |
| `name:` | none; the `id` is the handle |

The app reads `is_present()` and `rssi_dbm()` from the proposed device. BLE and
Wi-Fi share the 2.4 GHz radio, so a real implementation would also need a
coexistence decision; ESPHome makes that choice for its tracker, espforge has
no radio scheduler to make it in.

### What is missing

A NimBLE (or equivalent) controller integration, a scanning service, an
advertisement parser with MAC filtering, and a presence device. The radio
peripheral also needs to coexist with `<wifi>` rather than claim the same
hardware block exclusively.
