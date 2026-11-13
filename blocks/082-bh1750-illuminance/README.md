# 082 — High Dynamic Range I²C Ambient Light Sensor (BH1750)

**Status:** 🟡 partly possible — the I²C bus and the two-byte lux register are real
**ESPHome lego:** `sensor: platform: bh1750`

## ESPHome

```yaml
sensor:
  - platform: bh1750
    name: "Ambient Illuminance"
    address: 0x23
    i2c_id: i2c0
    measurement_duration: 69
    update_interval: 10s
```

The BH1750 integrates a photodiode current over a fixed measurement time and
reports lux directly as a 16-bit value divided by 1.2. Longer integration
times reach lower light levels; `measurement_duration: 69` is the datasheet's
default of 69 ms, which covers roughly 0–65 klx.

## espforge

`bh1750-illuminance.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0`, `address: 0x23` | `<i2c id="i2c0" sda="4" scl="5" />`, `ADDRESS` |
| `measurement_duration: 69` | command `0x40` for continuous high-resolution mode (1 lx) |
| `update_interval: 10s` | `delay_ms(10_000)` |
| lux conversion | `raw / 1.2`, the datasheet's divisor |
| `platform: bh1750` | none: no driver |
| `name:` | none; the reading is logged |

One command byte starts continuous measurement; after that each read returns
the latest result with no further writes. That is small enough to be a 🟡 block:
the app is essentially the driver minus its entity.

### What is missing

A `<bh1750>` sensor entity with the mode as an attribute (high/low resolution,
one-shot vs continuous) and the lux value published.
