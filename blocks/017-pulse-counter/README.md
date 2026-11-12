# 017 — Hardware Pulse Counter & Rate Metering

**Status:** 🟡 partly possible — polling a `<button>` counts edges; there is no hardware counter
**ESPHome lego:** `sensor: platform: pulse_counter`

## ESPHome

```yaml
sensor:
  - platform: pulse_counter
    pin: GPIO4
    name: "Water Flow Rate"
    update_interval: 10s
    filters:
      - multiply: 0.1
```

A flow sensor emits one pulse per fixed volume of water; counting edges over a
window and multiplying by the meter constant gives litres per minute. ESPHome
counts in hardware (the ESP32's pulse counter), so the CPU is free between
reads and short pulses are not missed.

## espforge

`pulse-counter.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `platform: pulse_counter`, `pin` | none: today `<button id="flow_sensor" gpio="$gpio4" pull_up="true" />` polls the pin |
| hardware edge counter | none: the app samples every 1 ms and counts low-to-high transitions |
| glitch filtering | Rust (`GLITCH_MS`): an edge closer than 2 ms to the last one is ignored |
| `filters: multiply` | arithmetic: `hz * 60 / PULSES_PER_LITRE` (450 pulses/L for a YF-S201) |
| `update_interval: 10s` | the sampling window in `forever` |
| `name:` | none; the `id` is the handle |

The loop is honest about the trade-off: 1 ms sampling sees pulses up to about
500 Hz and spends the whole CPU polling. A dedicated counter would do it in
hardware and timestamp the edges.

### What is missing

A PCNT/timer-capture peripheral element and a counter device with a
counted-window API. There is also no interrupt handle for apps and no
declarative filter chain (block 005), so both the counting and the debounce are
Rust.
