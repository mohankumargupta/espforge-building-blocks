# 085 — 4-Wire Tachometer Fan & Closed-Loop RPM Sensing

**Status:** ❌ not possible yet
**ESPHome lego:** `fan: platform: speed` + `sensor: platform: pulse_counter`

## ESPHome

```yaml
output:
  - platform: ledc
    pin: GPIO6
    id: fan_pwm_output
    frequency: 25kHz

sensor:
  - platform: pulse_counter
    pin:
      number: GPIO7
      mode: INPUT_PULLUP
    name: "Cooling Fan RPM"
    unit_of_measurement: "RPM"
    update_interval: 3s
    filters:
      - multiply: 30 # 2 pulses per revolution: (pulses / 2) * (60s / sample_time)

fan:
  - platform: speed
    output: fan_pwm_output
    name: "Chassis Fan"
```

A 4-wire fan separates power, ground, PWM and tach. The PWM line wants 25 kHz
so it does not audibly modulate the motor, and the tach gives two pulses per
revolution — 30× a 3 s count is RPM.

## espforge

`tach-fan.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: ledc`, `frequency: 25kHz` | proposed `<pwm id="fan_drive" pin="6" frequency="25kHz" />` |
| `pin: number / mode: INPUT_PULLUP` (tach) | `<button id="fan_tach" pin="7" pull_up="true" />` |
| `pulse_counter` | none: no PCNT, and the tach is far too fast to poll |
| `multiply: 30` | `RPMS_PER_COUNT = 30` for a 3 s window |
| `fan: platform: speed` | none: no fan entity (block 036) |
| closed loop | the app would compare RPM to the request and trim duty |
| `name:` | none |

Both halves are missing and both are hardware: a 25 kHz PWM output and an edge
counter that can keep up with a fan at 3000 RPM (100 Hz, 2 pulses per turn).
Polling would need a sample period well under 5 ms, which the scheduler cannot
promise.

### What is missing

An LEDC output for the drive and a PCNT counter for the tach — the same pair
blocks 036 and 017 need — plus a `<fan>` entity that closes the loop.
