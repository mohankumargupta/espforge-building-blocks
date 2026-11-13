# 059 — BLE Human Interface Device (HID) Media Remote

**Status:** ❌ not possible yet
**ESPHome lego:** `esp32_ble_server:` + `ble_keyboard:`

## ESPHome

```yaml
esp32_ble_server:

ble_keyboard:
  name: "Living Room Remote"

binary_sensor:
  - platform: gpio
    pin: GPIO3
    name: "Mute Button"
    on_press:
      then:
        - ble_keyboard.action:
            action: MEDIA_MUTE
```

A HID-over-GATT peripheral: the node advertises the HID service with a report
descriptor, the host subscribes to input reports, and a key press becomes a
2-byte consumer report (`0x00 0xE2` for mute) followed by a release.

## espforge

`ble-hid.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp32_ble_server:` | proposed `<ble id="radio" />` peripheral, in peripheral role |
| `ble_keyboard:` | proposed `<ble_hid id="remote" ble="$radio" kind="consumer" />` |
| HID report descriptor | none: no descriptor builder or GATT server |
| `ble_keyboard.action:` | proposed `send(report)` with a press/release pair |
| `binary_sensor: platform: gpio` | `<button id="mute_button" pin="3" pull_up="true" />` is real |
| `name:` | none; bonding and the advertised name are both absent |

The button and the report bytes are the only real parts. Everything between
them — GAP advertising, the GATT service table, report descriptors, bonding —
is missing, and block 097 needs the same GATT server for a custom service.

### What is missing

A peripheral-role BLE controller with a GATT server, a HID report descriptor
layer and bonding. Blocks 048, 069 and 097 are waiting on the same controller.
