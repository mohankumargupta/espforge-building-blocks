# 037 — Matrix Keypad Scanning & Anti-Ghosting

**Status:** 🟡 partly possible — rows and columns are GPIO, the scan is Rust; there is no keypad element
**ESPHome lego:** `matrix_keypad:`

## ESPHome

```yaml
matrix_keypad:
  id: num_pad
  rows:
    - pin: GPIO0
    - pin: GPIO1
  columns:
    - pin: GPIO2
    - pin: GPIO3
  keys: "1234"
  has_diodes: false

binary_sensor:
  - platform: matrix_keypad
    keypad_id: num_pad
    key: "1"
    name: "Keypad Digit 1"
```

A matrix keypad wires rows and columns as a grid: the firmware drives one row
low at a time, reads the columns, and the intersection that goes low is the
pressed key. Without diodes, three simultaneous keys can create a ghost path,
so firmware reports at most one key until the grid is released.

## espforge

`matrix-keypad.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `rows: pin` | `<led id="row0" pin="0" />` / `<led id="row1" pin="1" />`; `off()` drives the row low |
| `columns: pin` | `<button id="col0" pin="2" pull_up="true" />` / `<button id="col1" pin="3" …>` |
| `keys: "1234"` | `scan()` returns 1..4, in row-major order |
| scanning | one row strobed per pass, 5 µs settle, then both columns read |
| `debounce` | the row settles before the read; `LAST_KEY` suppresses repeats |
| `has_diodes: false` | no diodes: only single-key lockout, no true anti-ghosting |
| `binary_sensor: platform: matrix_keypad, key` | none: the app logs the digit |
| `name:` | none |

GPIO2 is a strapping pin on the C3, so the second column is pulled only after
boot; the README notes it and `check` warns about it. The scan itself is real
GPIO work — the same trade as block 023's polled encoder.

### What is missing

A keypad element that owns the row/column table and emits key events, plus
diode-aware anti-ghosting and an interrupt-driven scan. Here a held key blocks
every other key by design.
