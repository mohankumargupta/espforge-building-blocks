# 051 — PID Feedback Loop & Precision Temperature Regulation

**Status:** ❌ not possible yet — the PID math is real over a real sensor, but there is no PWM heater output
**ESPHome lego:** `climate: platform: pid` + `output: platform: ledc`

## ESPHome

```yaml
output:
  - platform: ledc
    pin: GPIO3
    id: heater_pwm

climate:
  - platform: pid
    name: "Precision Reflow Plate"
    sensor: thermocouple_temp
    default_target_temperature: 150.0
    heat_output: heater_pwm
    control_parameters:
      kp: 0.04
      ki: 0.001
      kd: 0.1
      output_averaging_samples: 5
```

A PID climate is not bang-bang: the error and its integral and derivative are
summed into a duty cycle, the output is clamped to 0–100 %, the integral stops
accumulating while clamped (anti-windup), and the derivative is taken on the
measurement rather than the error so a setpoint change does not kick the
output.

## espforge

`pid-climate.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `sensor: thermocouple_temp` | a real `<scd4x>` temperature reading (the thermocouple is block 025) |
| `heat_output: heater_pwm` | proposed `<pwm id="heater" pin="3" />`; there is no LEDC |
| `kp: 0.04`, `ki: 0.001`, `kd: 0.1` | `KP`/`KI`/`KD` in `app.rs` |
| `default_target_temperature: 150.0` | `TARGET_CENTI` |
| `output_averaging_samples: 5` | a five-sample average of the computed duty |
| `climate: platform: pid` | none: no climate entity or mode machine |
| anti-windup, derivative filtering | implemented in `app.rs` |

The controller runs and logs a duty cycle every five seconds; the only reason
this block is ❌ is that the duty has nowhere to go. A `<led>` would give
bang-bang, which is exactly what a PID is meant to replace.

### What is missing

An `<output>` element with a duty cycle (LEDC), a `<climate>` entity with
target/mode plumbing, and the publish path for the current action. The control
math is already expressible.
