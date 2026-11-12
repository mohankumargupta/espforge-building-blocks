# 036 — Multi-Speed Fan Controller & State Engine

**Status:** ❌ not possible yet
**ESPHome lego:** `fan: platform: speed` + `output: platform: ledc`

## ESPHome

```yaml
output:
  - platform: ledc
    pin: GPIO3
    id: fan_speed_pwm

fan:
  - platform: speed
    output: fan_speed_pwm
    name: "Ceiling Fan"
    speed_count: 3
```

A three-speed fan maps OFF/LOW/MED/HIGH onto duty-cycle bands of one PWM
output, and a direction relay is interlocked so it can only switch while the
motor is stopped.

## espforge

`fan-speed.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: ledc, pin, frequency` | proposed `<pwm id="fan_speed" pin="3" frequency="25kHz" />` |
| `fan: platform: speed, speed_count` | proposed `<fan id="ceiling" output="$fan_speed" speed_count="3" />` |
| `fan.turn_on: speed: 2` | proposed `device!(ctx, ceiling).set_speed(2)` |
| direction relay interlock | none: needs a `<switch>`-like output plus an entity to enforce it |
| state (`on`/`percentage`) | none: no entity state or command transport |
| `name:` | none |

The duty table is trivial arithmetic; the blocker is that there is no PWM
output to apply it to, and no fan entity to receive a speed command. This is
block 008's missing peripheral plus a state engine on top.

### What is missing

A PWM/LEDC output with runtime duty, a fan entity with discrete speeds, and a
command path (native API, web or MQTT) to set them. As in blocks 003 and 028,
the control logic is the easy half.
