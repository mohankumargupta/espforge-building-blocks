# 077 — 2D Addressable LED Matrix Display Engine

**Status:** ❌ not possible yet
**ESPHome lego:** `display: platform: addressable_light`

## ESPHome

```yaml
light:
  - platform: esp32_rmt_led_strip
    id: raw_led_strip
    pin: GPIO3
    num_leds: 256
    chipset: ws2812
    rgb_order: GRB

display:
  - platform: addressable_light
    id: led_panel
    light_id: raw_led_strip
    width: 16
    height: 16
    pixel_mapper: |-
      if (y % 2 == 0) return (y * 16) + x;
      return (y * 16) + (15 - x);
    lambda: |-
      it.print(0, 0, id(font_default), "HI");
```

A matrix is a strip folded into rows, and the panel is usually wired in a
serpentine so every other row runs backwards. The `pixel_mapper` is the
inverse of that wiring: it maps an `(x, y)` in screen space to a strip index.
Everything above it (glyphs, scrolling) is a normal framebuffer.

## espforge

`led-matrix.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp32_rmt_led_strip` | proposed `<led_strip id="panel_strip" pin="3" leds="256" chipset="ws2812" />` |
| `width: 16` / `height: 16` | proposed `width="16" height="16"` on the display |
| `pixel_mapper:` lambda | proposed a built-in `layout="serpentine"` instead of a lambda |
| text drawing | none: no framebuffer in RGB space |
| `id:` | none |
| `name:` | none |

The pixel mapper is the interesting part and it is trivial arithmetic; the
blocking dependency is the same RMT strip as blocks 015 and 062. A 16×16 panel
also needs a colour framebuffer, which is 768 bytes — affordable, but it does
not exist.

### What is missing

The RMT LED strip (block 015), a `<matrix>` display with width, height and a
serpentine/column-major layout option, and a font renderer that writes colour
pixels instead of 1-bit ones.
