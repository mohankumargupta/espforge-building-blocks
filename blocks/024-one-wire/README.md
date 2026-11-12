# 024 — 1-Wire Bus & Multi-Drop Temperature Networks

**Status:** ❌ not possible yet
**ESPHome lego:** `one_wire:` + `sensor: platform: dallas_temp`

## ESPHome

```yaml
one_wire:
  - platform: gpio
    pin: GPIO4
    id: ow_bus

sensor:
  - platform: dallas_temp
    one_wire_id: ow_bus
    index: 0
    name: "Water Tank Temperature"
    update_interval: 15s
```

One pin carries power, data and multiple devices. The master pulls the line low
for a reset pulse, listens for a presence pulse, then addresses one ROM code at
a time with 1 µs time slots. `index: 0` is the first device found by the Search
ROM algorithm, and every DS18B20 scratchpad ends with a CRC8.

## espforge

`one-wire.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `one_wire: platform: gpio, pin` | proposed `<one_wire id="ow_bus" pin="4" />` peripheral |
| `sensor: platform: dallas_temp, one_wire_id, index` | proposed `<dallas_temp id="water_tank" one_wire="$ow_bus" index="0" />` |
| ROM search / multi-drop | none: no enumeration, so `index` cannot resolve |
| scratchpad CRC8 | none: belongs to the driver |
| reset/presence and 1 µs slots | none: needs an open-drain line and hardware-timed delays |
| `update_interval: 15s` | a `delay_ms(15_000)` in `forever` |
| `name:` | none; the `id` is the handle |

`<gpio direction="bidirectional">` is in the vocabulary, but it is a declaration
that a *driver* claims; a standalone `<gpio>` never becomes a handle on
`Context`, so `app.rs` cannot bit-bang the line itself. The runtime's
`Bidirectional` type has the drive/release/level API this block needs, but no
element exposes it to the app.

### What is missing

The `<one_wire>` peripheral (open-drain plus a timing-aware reset/read/write
slot API), the Search ROM algorithm, and the `<dallas_temp>` driver with its
CRC8 check. Timing is the hard part: a 1 µs slot is shorter than a scheduler
tick, so this cannot be polling code in `forever`.
