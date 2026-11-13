# 044 — Stepper Motor & Trapezoidal Acceleration Profiling

**Status:** 🟡 partly possible — two GPIOs and the profile are real; the pulse train is software-timed
**ESPHome lego:** `stepper: platform: a4988`

## ESPHome

```yaml
stepper:
  - platform: a4988
    id: linear_actuator
    step_pin: GPIO3
    dir_pin: GPIO2
    max_speed: 1000 steps/s
    acceleration: 400 steps/s ^ 2
    deceleration: 400 steps/s ^ 2
```

An A4988 takes a direction level and one pulse per microstep. The move is a
trapezoid: accelerate at 400 steps/s² up to 1000 steps/s, cruise, then
decelerate so the step count lands exactly on the target.

## espforge

`stepper-motor.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `step_pin` / `dir_pin` | `<led id="step" pin="3" />` and `<led id="dir" pin="2" />` |
| `max_speed: 1000 steps/s` | `MAX_STEPS_PER_SEC = 1000`, the plateau of the profile |
| `acceleration` / `deceleration` | `STEPS_PER_SEC_SQ = 400`, symmetric here |
| `set_target` / `id(id).write` | `move_to(steps)` in `app.rs` |
| microstepping | jumper on the driver: not a firmware setting |
| hardware pulse engine | none: the pulse period comes from `delay_ns` |
| `name:` | none; the ids are the handles |

The step period is computed from the current velocity and passed to
`delay_ns`, which is why this block is 🟡 and not ❌: the trapezoid is real
arithmetic over two real pins. The caveats are honest ones — the CPU spends the
whole move in the loop, and `delay_ns` jitter shows up directly as position
error at the top of the ramp.

### What is missing

A hardware step engine (a timer or RMT channel that emits the pulses), so the
move can run in the background and land on a step count without software
latency.
