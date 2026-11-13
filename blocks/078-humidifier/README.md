# 078 — Closed-Loop Humidifier & Dehumidifier Controller

**Status:** ✅ possible today, as Rust in `forever` (no humidifier entity)
**ESPHome lego:** `humidifier: platform: template`

## ESPHome

```yaml
humidifier:
  - platform: template
    name: "Cigar Humidor"
    target_humidity: 65%
    humidity_sensor: room_humidity
    target_humidity_range: 2.0%
    action:
      then:
        - switch.turn_on: mist_generator
    idle_action:
      then:
        - switch.turn_off: mist_generator
```

A humidifier entity with a deadband: below 65 − 2 % the mist generator runs,
above 65 + 2 % it stops, and in between nothing changes. The gap is what keeps
the relay from chattering on a reading that hovers at the target.

## espforge

`humidifier.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `humidity_sensor: room_humidity` | a real `<scd4x>` `humidity_centi_pct` |
| `target_humidity: 65%` | `TARGET_CENTI = 6500` |
| `target_humidity_range: 2.0%` | `RANGE_CENTI = 200`: the deadband half-width |
| `action: switch.turn_on` | `mist.on()` on an `<led>` relay |
| `idle_action: switch.turn_off` | `mist.off()` |
| `platform: template` humidifier | none: no humidifier entity |
| compressor anti-short-cycle guard | `MIN_OFF_SECS = 60` before misting restarts |
| `name:` | none |

The sensor, the relay and the hysteresis are all real, so this block is ✅ in
the same sense as block 028: the entity is missing, the behaviour is not. The
minimum off time is there because a humidifier's driver is a compressor in
most real humidors.

### Notes

- Hysteresis exists in the YAML implicitly (`target_humidity_range`); here it
  is the two comparisons. Without it, `>= target` and `< target` would toggle
  every 5 s on a sensor with 0.1 % noise.
