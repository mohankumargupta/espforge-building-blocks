# 021 — Servos & Calibrated Pulse Width Modulation

**Status:** 🟡 partly possible — `<led>` plus `delay_ns` generates the pulse; there is no PWM peripheral
**ESPHome lego:** `output: platform: ledc` + `servo:`

## ESPHome

```yaml
output:
  - platform: ledc
    pin: GPIO3
    id: servo_pwm
    frequency: 50Hz

servo:
  - id: camera_pan
    output: servo_pwm
    min_pulse_width: 500us
    max_pulse_width: 2500us
    transition_length: 500ms
```

A hobby servo is a 50 Hz PWM signal whose high time is the angle: 500 µs at one
end, 2500 µs at the other. The LEDC peripheral keeps the pulse exact in
hardware, and `servo:` adds calibration and a slew rate for transitions.

## espforge

`servo-pwm.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: ledc, pin, frequency` | none: no LEDC/PWM element. `<led id="servo_pwm" pin="3" />` is the pin |
| `servo: min_pulse_width` / `max_pulse_width` | constants in `app.rs`; `pulse_ns(angle)` maps 0..180° |
| `servo.id` / `output` | the `<led>` id: `servo.on()` / `servo.off()` |
| `transition_length: 500ms` | `ANGLE_STEP` per 20 ms frame; 2° per frame is a 1.8 s full sweep |
| 50 Hz frame | `delay_ns(pulse)`, then `delay_ns(20ms - pulse)` |
| `name:` | none; the `id` is the handle |

Every frame takes 20 ms, and both waits are busy-waits: the CPU spends the whole
frame in `app.rs`. That is the cost of software PWM, and the reason the angle is
stepped once per frame rather than driven from a timer interrupt.

### What is missing

A PWM/LEDC peripheral with `frequency` and a duty value, and a servo device on
top of it owning `min_pulse_width` / `max_pulse_width` and `write_angle(deg)`.
Pulse jitter also needs a hardware timer: `delay_ns` is a busy-wait, so an
interrupt or a longer loop path stretches the high time.
