# 056 — Wiegand Protocol Dual-Wire Access Keypad / RFID

**Status:** ❌ not possible yet
**ESPHome lego:** `wiegand:` + `binary_sensor: platform: wiegand`

## ESPHome

```yaml
wiegand:
  id: rfid_reader
  d0_pin: GPIO2
  d1_pin: GPIO3

binary_sensor:
  - platform: wiegand
    wiegand_id: rfid_reader
    key: 0x123456
    name: "Master Keycard Detected"
    on_press:
      - switch.turn_on: door_latch
```

Wiegand is not a UART. Each bit is a 50–100 µs pulse on one of two lines: D0
for a 0, D1 for a 1. A 26-bit frame is 24 payload bits, one leading even parity
bit and one trailing odd parity bit; between bits more than 25 ms of silence
ends the frame.

## espforge

`wiegand.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `d0_pin` / `d1_pin` | proposed `<wiegand id="reader" d0="2" d1="3" />` device |
| bit pulses | none: no pin interrupts with timestamps |
| frame gap (25 ms silence) | none: no idle-gap timer |
| `binary_sensor: platform: wiegand, key:` | proposed `<wiegand_key id="master_card" reader="$reader" code="0x123456" />` |
| `on_press: switch.turn_on` | `latch.on()` on an `<led>` is real |
| `name:` | none; the card match is logged |

Polling is not an option: a 100 µs pulse is gone before a `forever` loop can
look at the pin, so this block cannot be 🟡. It needs edge capture, which is the
same missing piece as block 017's pulse counter.

### What is missing

A pin-interrupt capture API with microsecond timestamps, a Wiegand framer with
parity checking, and a key/card matching entity. Block 081's Geiger counter
needs the same capture.
