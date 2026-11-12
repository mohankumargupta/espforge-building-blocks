# 015 — Addressable RGB LED Strip (RMT Engine)

**Status:** ❌ not possible yet
**ESPHome lego:** `light: platform: esp32_rmt_led_strip`

## ESPHome

```yaml
light:
  - platform: esp32_rmt_led_strip
    rgb_order: GRB
    pin: GPIO3
    num_leds: 30
    rmt_channel: 0
    chipset: ws2812
    name: "Desk Ambient Light"
    effects:
      - pulse:
          name: "Fast Pulse"
          transition_length: 500ms
```

A WS2812 strip is not a PWM output: every pixel needs a 24-bit word at roughly
800 kHz, with pulse widths of a few hundred nanoseconds. ESPHome drives that
timing from the RMT peripheral; the light entity adds colour order, brightness
and effects.

## espforge

`rmt-led-strip.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `platform: esp32_rmt_led_strip` | proposed `<ws2812 id="desk_light" gpio="$gpio3" num_leds="30" rgb_order="grb" chipset="ws2812" />` device |
| `pin: GPIO3` | `<gpio id="gpio3" pin="3" direction="output" />`, the same checked-direction form `<led>` uses |
| `rmt_channel: 0` | no attribute: the driver would claim a free RMT channel the way `<led>` claims a pin |
| `num_leds`, `rgb_order`, `chipset` | proposed attributes |
| `effects: pulse` + `transition_length` | Rust in `forever`; there is no effect registry and no transitions |
| `light:` brightness/on/off | none: the device handle would expose `fill`/`show`, not a light entity |
| `name:` | none; the `id` is the handle |

The proposed device owns the pixels, not the light entity: ESPHome's brightness
curve and effect list become app code here. `<led>` cannot stand in — it drives
one pin at one level and has no framebuffer.

### What is missing

RMT pulse generation, a WS2812 encoder (chipset timing, RGB/GRB order, the
reset gap), a pixel framebuffer, and a non-blocking effect scheduler. The open
question is the same one block 008 raises: whether brightness is linear or
gamma-corrected, and whether transitions belong to the device or to the
generated scheduler.
