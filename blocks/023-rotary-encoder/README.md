# 023 — Quadrature Rotary Encoders & Knob Inputs

**Status:** 🟡 partly possible — two `<button>` inputs are polled and decoded in `app.rs`; there is no PCNT or interrupt input
**ESPHome lego:** `sensor: platform: rotary_encoder`

## ESPHome

```yaml
sensor:
  - platform: rotary_encoder
    name: "Volume Dial"
    pin_a: GPIO6
    pin_b: GPIO7
    resolution: 4
    min_value: 0
    max_value: 100
```

A quadrature encoder closes two switches in a Gray-code sequence. Decoding the
A/B transitions gives direction; counting four transitions per detent gives one
step. ESPHome uses pin interrupts, so even a fast turn is not missed.

## espforge

`rotary-encoder.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin_a: GPIO6` / `pin_b: GPIO7` | two `<button pull_up="true">` devices on GPIO6/7 |
| `resolution: 4` | `QUARTER_STEPS` accumulates transitions; every 4 becomes one knob step |
| `min_value` / `max_value` | `clamp(VALUE, 0, 100)` |
| interrupt-driven decoding | none: the app samples every 1 ms |
| `filters` / debounce | an invalid two-bit jump is ignored; no time-based filter |
| `name:` | none; the `id` is the handle |

The decoder table accepts only the four adjacent Gray-code transitions and
ignores anything else, which rejects most contact bounce without a timer. A
fast turn can still outrun the 1 ms sampler, and the encoder direction can be
reversed by swapping A and B (or the two branches of the table).

### What is missing

A PCNT peripheral or an interrupt-capable input device, plus a real encoder
element with `pin_a`/`pin_b`, `resolution` and bounds. That element would own
the debounce and the detent math; the polling version here owns neither.
