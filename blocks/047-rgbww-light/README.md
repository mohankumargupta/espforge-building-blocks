# 047 — 5-Channel RGBWW & Color Temperature Blending

**Status:** ❌ not possible yet
**ESPHome lego:** `output: platform: ledc` (×5) + `light: platform: rgbww`

## ESPHome

```yaml
output:
  - platform: ledc
    pin: GPIO0
    id: out_r
  - platform: ledc
    pin: GPIO1
    id: out_g
  - platform: ledc
    pin: GPIO2
    id: out_b
  - platform: ledc
    pin: GPIO3
    id: out_cw
  - platform: ledc
    pin: GPIO10
    id: out_ww

light:
  - platform: rgbww
    name: "Architectural Ceiling Light"
    red: out_r
    green: out_g
    blue: out_b
    cold_white: out_cw
    warm_white: out_ww
    cold_white_color_temperature: 6500 K
    warm_white_color_temperature: 2700 K
```

Five independent PWM channels: RGB for colour, and a cold/warm white pair whose
ratio sets the colour temperature in mireds. Every channel is gamma-corrected
so a dimmed light looks linear to the eye.

## espforge

`rgbww-light.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: ledc` ×5 | proposed `<pwm id="out_r" pin="0" frequency="1kHz" resolution="10bit" />` ×5 |
| `light: platform: rgbww` | proposed `<light id="ceiling" kind="rgbww" ... />` with one reference per channel |
| `cold_white_color_temperature: 6500 K` | `COLD_K = 6500`, one end of the blend |
| `warm_white_color_temperature: 2700 K` | `WARM_K = 2700` |
| mireds blending | `mireds()` in `app.rs`: 1e6 / kelvin, interpolated |
| gamma correction | `GAMMA = 2.2` applied per channel |
| `name:` | none; the id is the handle |

The ESPF file is shape only: there is no LEDC output, so five `<pwm>` elements
and one `<light>` entity would all have to be invented. The blend arithmetic in
`app.rs` is real, and it is the part that would survive into a driver.

### What is missing

A `<pwm>`/LEDC peripheral (one timer, five channels here) and a `<light>`
entity that owns colour spaces, transitions and mireds — block 008's missing
half, five times over.
