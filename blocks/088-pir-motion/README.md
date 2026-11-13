# 088 — PIR Passive Infrared Motion & Refractory Filtering

**Status:** ✅ possible today, with the filter in `app.rs`
**ESPHome lego:** `binary_sensor: platform: gpio` + `filters: delayed_off`

## ESPHome

```yaml
binary_sensor:
  - platform: gpio
    pin:
      number: GPIO2
      mode: INPUT
    name: "Hallway Motion"
    device_class: motion
    filters:
      - delayed_off: 30s
```

A PIR drives its output high for a second or two per detection. `delayed_off`
extends that to 30 s so a walk through a room is one long "motion" state rather
than a burst of events, and it also suppresses the sensor's re-trigger chatter.

## espforge

`pir-motion.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin: number: GPIO2` | `<button id="hallway_pir" pin="2" />` — the module drives the line, so no pull-up |
| `mode: INPUT` | no attribute: `pull_up` defaults to false |
| `device_class: motion` | none: no entity classes |
| `filters: delayed_off: 30s` | `HOLD_MS = 30_000` in a small state machine |
| `name:` | none; the state is logged |
| refractory blind period | a detection during the hold restarts the timer |

Two details make it behave: a detection while the hold is running restarts the
hold (retriggerable, like `delayed_off`), and the state is only logged on a
transition, so 30 s of motion is two log lines instead of thirty.

### Notes

- GPIO2 is a C3 strapping pin; the PIR output must not be high at reset, or the
  boot mode is affected. This block reads it only after boot, but the wiring is
  worth checking.
- `delayed_on` composes on top if the sensor is noisy: require two consecutive
  samples before starting the hold.
