# 045 — 24-Bit Strain Gauge (HX711) & Weight Calibration

**Status:** 🟡 partly possible — both lines are GPIO and the shift clock is bit-banged in `app.rs`
**ESPHome lego:** `sensor: platform: hx711`

## ESPHome

```yaml
sensor:
  - platform: hx711
    name: "Hopper Scale"
    dout_pin: GPIO3
    sck_pin: GPIO2
    gain: 128
    update_interval: 1s
    filters:
      - calibrate_linear:
          - 0 -> 0.0
          - 452300 -> 5.0
      - sliding_window_moving_average:
          window_size: 10
          send_every: 1
```

The HX711 is a two-wire part: DOUT goes low when a conversion is ready, then
each SCK pulse shifts out one bit, most significant first. Twenty-four bits are
the signed conversion; the extra pulses after them select the gain of the next
conversion (25 for 128, 26 for 64, 27 for 32).

## espforge

`hx711-scale.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `dout_pin: GPIO3` | `<button id="dout" pin="3" />` — the module drives it, so no pull-up |
| `sck_pin: GPIO2` | `<led id="sck" pin="2" />`, pulsed 25 times per read |
| `gain: 128` | `GAIN_PULSES = 25` (24 data bits plus one gain bit) |
| 24-bit signed conversion | `(raw << 8) >> 8` on the third byte's sign bit |
| `calibrate_linear` | `OFFSET`/`SCALE` from the two points (0, 452300) -> (0.0, 5.0) |
| `sliding_window_moving_average` | a 10-reading ring in `app.rs` |
| `update_interval: 1s` | `delay_ms(1_000)` |
| `name:` | none; the reading is logged |

The read is honest but slow to settle: SCK is clocked with `delay_ns`, so one
conversion costs a few hundred microseconds of foreground time. GPIO2 is a C3
strapping pin, so the module's SCK line must not be held low at boot (it is
idle low only after the first read in this app, which is why the pin starts
released).

### What is missing

A `<hx711>` device that owns the two lines, the gain bit and the rate-pin mode
(`update_interval` of 10 Hz or 80 Hz), plus the declarative filter pipeline both
this block and 005 want.
