# 073 — H-Bridge DC Motor Actuator & Dynamic Braking

**Status:** ❌ not possible yet
**ESPHome lego:** `fan: platform: hbridge`

## ESPHome

```yaml
output:
  - platform: ledc
    pin: GPIO6
    id: motor_in1
  - platform: ledc
    pin: GPIO7
    id: motor_in2

fan:
  - platform: hbridge
    name: "Conveyor DC Motor"
    pin_a: motor_in1
    pin_b: motor_in2
    decay_mode: slow
```

An H-bridge is two half-bridges. Direction is which input is driven; speed is
how hard. The two inputs must never be driven strongly at once (shoot-through),
and how the bridge decays when the PWM stops decides braking: slow decay coasts,
fast decay brakes.

## espforge

`hbridge-motor.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: ledc` ×2 | proposed `<pwm id="motor_in1" pin="6" />` and `<pwm id="motor_in2" pin="7" />` |
| `pin_a` / `pin_b` | the two PWM outputs |
| `decay_mode: slow` | brake/coast selection in the driver |
| dead-time insertion | none: no complementary output with a dead time |
| `platform: hbridge` fan entity | none: no fan entity (block 036) |
| direction reversing | the driver's job, not `app.rs`'s |
| `name:` | none |

A `<led>` pair can only do on/off, which is full-speed forward, full-speed
reverse and brake — not a motor controller. Two things are missing and both
are hardware-shaped: PWM outputs and a bridge driver that owns the interlocks.

### What is missing

Two `<pwm>`/LEDC channels plus an `<hbridge>` fan that sequences direction,
inserts dead time and selects the decay mode. Blocks 036 and 085 are the same
missing PWM.
