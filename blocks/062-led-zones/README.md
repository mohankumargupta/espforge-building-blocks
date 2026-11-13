# 062 — Addressable LED Strip Partitioning & Virtual Zones

**Status:** ❌ not possible yet
**ESPHome lego:** `light: platform: esp32_rmt_led_strip` + `light: platform: partition`

## ESPHome

```yaml
light:
  - platform: esp32_rmt_led_strip
    id: physical_strip
    pin: GPIO3
    num_leds: 60
    rgb_order: GRB
    chipset: ws2812

  - platform: partition
    name: "Desk Left Zone"
    segments:
      - id: physical_strip
        from: 0
        to: 29

  - platform: partition
    name: "Desk Right Zone"
    segments:
      - id: physical_strip
        from: 30
        to: 59
```

One RMT buffer, three entities. The partition light is a view: writing a colour
into `0..29` updates the shared buffer, and the next flush sends all 60 pixels
in one RMT burst. Non-overlapping ranges keep the two zones from fighting.

## espforge

`led-zones.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `platform: esp32_rmt_led_strip` | proposed `<led_strip id="physical_strip" pin="3" leds="60" chipset="ws2812" order="GRB" />` |
| `num_leds: 60` | `leds="60"` |
| `rgb_order: GRB` | `order="GRB"`: the wire order, not the API order |
| `platform: partition`, `from`/`to` | proposed `<light_zone id="desk_left" strip="$physical_strip" from="0" to="29" />` |
| the shared framebuffer + one flush | none: no RMT channel and no strip driver |
| `name:` | none; the ids are the handles |

The zones are the interesting design point, not the protocol: a zone must own an
index range that the compiler can prove does not overlap another zone's, which
is exactly the kind of check espforge already does for pins and peripherals.

### What is missing

An RMT LED-strip device (block 015) and a `<light_zone>` that shares its buffer
with an index range, with overlap validation across zones.
