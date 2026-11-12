# 029 — Synthetic Template Entities & Derived Math

**Status:** ✅ possible today, as Rust in `forever` (no template entity)
**ESPHome lego:** `sensor: platform: template`

## ESPHome

```yaml
sensor:
  - platform: template
    name: "Calculated Dew Point"
    unit_of_measurement: "°C"
    accuracy_decimals: 1
    lambda: |-
      float t = id(room_temperature).state;
      float rh = id(room_humidity).state;
      return t - ((100.0 - rh) / 5.0);
```

A template sensor derives a value from other entities. The lambda runs in the
firmware, reads the current state of two sensors, and publishes a new number.

## espforge

`template-sensor.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `sensor: platform: template` | none: no template entity; `dew_point()` is ordinary Rust |
| `lambda: id(room_temperature).state` | `device!(ctx, air).read()` → `temperature_centi_c` |
| `id(room_humidity).state` | the same reading's `humidity_centi_percent` |
| `unit_of_measurement` / `accuracy_decimals` | the log format: `{:.1} C` |
| `update_interval` | `delay_ms(60_000)` |
| `name:` | none; the reading has no name |

Both inputs come from one `<scd4x>` measurement, so they are sampled together
rather than at two different times. The formula is the roadmap's approximation,
`t - (100 - rh) / 5`, not the full Magnus equation: `ln` is not in `core`, and
this block deliberately adds no math crate.

### What is missing

A template sensor entity and a publish path, plus an expression language for
lambdas. As in blocks 005 and 009, the computation itself already works; what
is missing is the entity that would carry the result to a dashboard.
