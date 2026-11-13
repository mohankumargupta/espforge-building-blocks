# 064 — AC Phase-Cutting Dimmer & Zero-Cross Triac Timing

**Status:** ❌ not possible yet
**ESPHome lego:** `output: platform: ac_dimmer` + `light: platform: monochromatic`

## ESPHome

```yaml
output:
  - platform: ac_dimmer
    id: light_dimmer
    gate_pin: GPIO3
    zero_cross_pin:
      number: GPIO2
      mode: INPUT_PULLUP

light:
  - platform: monochromatic
    output: light_dimmer
    name: "Filament Chandelier"
```

Mains dimming is a timing problem measured in microseconds. The zero-cross
circuit fires an edge every half cycle (10 ms at 50 Hz); the firmware waits a
fraction of that and then strobes the triac gate for a few tens of
microseconds. Brightness is the complement of the delay, and the conduction
angle is not linear in that delay.

## espforge

`ac-dimmer.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `zero_cross_pin` | proposed `<ac_dimmer id="dimmer" gate="3" zero_cross="2" />` output |
| edge capture on the zero-cross pin | none: no interrupt with timestamps |
| `delay_us` to the gate strobe | none: `delay_ns` is too coarse and jitters under interrupt load |
| `gate_pin: GPIO3` | `<led id="gate" pin="3" />`, which can strobe but not on time |
| `platform: monochromatic` light | none: no light entity |
| `name:` | none; the level would be a float |

This is ❌ rather than 🟡 on purpose: a late gate strobe is visible as flicker
and, worse, an early one fires before the triac has commutated. Neither the
capture nor the microsecond-accurate strobe exists, and software timing cannot
substitute the way it can for a servo.

### What is missing

A zero-cross input with timestamped edges, a hardware timer one-shot for the
gate delay, and a `<pwm>`-like output whose duty is a conduction angle.
