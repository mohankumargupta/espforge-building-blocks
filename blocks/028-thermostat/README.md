# 028 — Climate Control & Thermostat Regulation

**Status:** ✅ possible today, as Rust in `forever` (no climate entity)
**ESPHome lego:** `climate: platform: thermostat`

## ESPHome

```yaml
climate:
  - platform: thermostat
    name: "HVAC Zone Controller"
    sensor: bme_temperature
    min_cooling_off_time: 300s
    min_heating_off_time: 300s
    default_target_temperature_low: 20.0
    default_target_temperature_high: 22.0
    heat_action:
      - switch.turn_on: heater_relay
    idle_action:
      - switch.turn_off: heater_relay
```

A bang-bang thermostat: heat below the low setpoint, stop above the high one,
and never re-energise the heater until the minimum off time has elapsed. The
gap between the setpoints is the hysteresis that stops short cycling.

## espforge

`thermostat.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `climate: platform: thermostat` | none: no climate entity; the state machine is Rust |
| `sensor: bme_temperature` | an `<scd4x>` reading's `temperature_centi_c` (the BME280 driver is block 007) |
| `default_target_temperature_low` / `high` | `TARGET_LOW_CENTI` / `TARGET_HIGH_CENTI` |
| `heat_action` / `idle_action` | `heater.on()` / `heater.off()` on an `<led>` (there is no `<switch>`) |
| `min_heating_off_time: 300s` | `SECONDS_SINCE_OFF` must reach 300 before heating restarts |
| `min_cooling_off_time` | unused: this block has no cooler |
| `name:` | none; the ids are the handles |

Hysteresis is the 20.0–22.0 °C gap, and the reading is taken once per 5 s (the
SCD4x's measurement period). The heater state and the time since it last
switched off live in statics because the generated main calls `forever`
repeatedly and locals would reset every tick.

### What is missing

The climate entity, target-temperature commands, heat/cool mode selection and a
publish path for the current action. The control loop and the anti-short-cycle
delay are already expressible; blocks 003, 013 and 014 are the missing command
transports.
