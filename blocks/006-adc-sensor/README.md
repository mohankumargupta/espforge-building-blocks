# 006 — Polling Sensor & Data Transformations

**Status:** ❌ not possible yet
**ESPHome lego:** `sensor: platform: adc` + `filters`

## ESPHome

```yaml
sensor:
  - platform: adc
    pin: GPIO4
    name: "Battery Voltage"
    update_interval: 10s
    filters:
      - multiply: 2.0
      - sliding_window_moving_average:
          window_size: 5
          send_every: 5
```

An analog input sampled every 10 s, scaled by a voltage divider and smoothed
over five samples before it is published.

## espforge

`adc-sensor.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin: GPIO4` | proposed `<adc id="battery_adc" pin="4" attenuation="11db" />` |
| `update_interval: 10s` | not declarative: a `delay_ms(10_000)` at the end of `forever` |
| `filters: multiply` / `moving_average` | Rust arithmetic over repeated `read_millivolts()` calls |
| `name:` | none; the `id` is the handle |

The proposed device is deliberately small: one calibrated millivolt reading per
call, no filters, no publishing. Timing and math belong to `app.rs`, matching how
blocks 004 and 005 split hardware from behaviour.

### What is missing

There is no analog element at all. espforge's bundled sensors (`adxl345`,
`scd4x`) are digital bus parts, and `<gpio>` cannot express an ADC channel. The
hardware side needs a peripheral that names an ADC-capable pin, attenuation and
resolution; the rest of the block is plain Rust and would work unchanged.
