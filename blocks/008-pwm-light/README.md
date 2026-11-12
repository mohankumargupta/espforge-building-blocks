# 008 — PWM Output & Dimmable Light

**Status:** ❌ not possible yet
**ESPHome lego:** `output: platform: ledc` + `light: platform: monochromatic`

## ESPHome

```yaml
output:
  - platform: ledc
    pin: GPIO18
    id: pwm_pin

light:
  - platform: monochromatic
    name: "Desk Lamp"
    output: pwm_pin
    default_transition_length: 1s
```

A hardware timer (LEDC) drives a GPIO with a duty cycle; a light entity adds
brightness and fades between levels over one second.

## espforge

`pwm-light.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: ledc, pin, id` | proposed `<pwm id="pwm_pin" pin="18" frequency="1kHz" />` peripheral |
| `light: platform: monochromatic, output` | proposed `<light id="desk_lamp" pwm="$pwm_pin" />` device |
| `default_transition_length: 1s` | runtime responsibility: `set_brightness()` would fade, not step |
| `name:` | none; the `id` is the handle |

`src/app.rs` uses the intended API: `turn_on`, `turn_off`, `set_brightness(0..=100)`.
Transitions must be non-blocking in the runtime — a fade that blocks `forever`
would make a dimmable lamp the only thing the firmware does.

### What is missing

Both halves: there is no PWM/LEDC peripheral element and no light abstraction.
`<led>` is on/off only, so it cannot stand in. The design questions this block
raises are the timer budget (which LEDC channels are free), the duty curve
(linear vs gamma), and whether transition state belongs to the device or the
generated scheduler.
