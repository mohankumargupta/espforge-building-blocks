# 081 — Radiation Pulse Counter & Geiger-Müller Tube

**Status:** 🟡 partly possible — a polled GPIO counts the slow pulses; the tube factor is in `app.rs`
**ESPHome lego:** `sensor: platform: pulse_counter` + a template sensor

## ESPHome

```yaml
sensor:
  - platform: pulse_counter
    pin:
      number: GPIO4
      mode: INPUT_PULLUP
    name: "Radiation Pulse Count"
    unit_of_measurement: "CPM"
    update_interval: 10s
    filters:
      - multiply: 6 # Convert 10s sample to counts per minute
  - platform: template
    name: "Radiation Dose Equivalent"
    unit_of_measurement: "µSv/h"
    accuracy_decimals: 3
    lambda: |-
      // J305/M4011 tube factor (~0.00812 CPM to µSv/h)
      return id(radiation_pulse_count).state * 0.00812;
```

A Geiger tube fires once per ionising event, and background levels are a few
tens of counts per minute, so a 10 s window has enough counts to be meaningful
once scaled by six. The dose conversion is tube-specific.

## espforge

`geiger-counter.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin: number / mode: INPUT_PULLUP` | `<button id="geiger_pulse" pin="4" pull_up="true" />` |
| `pulse_counter` | none: a sampled edge count in `app.rs` |
| `update_interval: 10s` | `WINDOW_MS = 10_000` |
| `multiply: 6` | `60 / WINDOW_S` applied to the count |
| tube factor `0.00812` | `USV_PER_CPM`, in a comment naming the tube |
| `platform: template` | none: the dose is Rust over the count |
| `name:` | none; both values are logged |

The counts here are slow enough for polling — 10 s at even 100 CPM is one pulse
every 100 ms — so unlike block 072 this works, with a caveat: a pulse shorter
than the 1 ms sample period is missed. The tube's output is normally ~100 µs,
so a hardware counter would be more faithful.

### What is missing

A hardware edge counter (PCNT) with a windowed read, which blocks 017, 072 and
085 also want, and a `<geiger>` sensor that applies the tube factor.
