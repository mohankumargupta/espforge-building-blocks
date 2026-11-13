# 093 — I²C Optical Color Sensing & Chromaticity (TCS34725)

**Status:** 🟡 partly possible — the I²C bus and the 8-byte RGBC result are real; the colour math is in `app.rs`
**ESPHome lego:** `sensor: platform: tcs34725`

## ESPHome

```yaml
sensor:
  - platform: tcs34725
    name: "Surface Color Sensor"
    i2c_id: i2c0
    address: 0x29
    gain: 4x
    integration_time: 50ms
    red:
      name: "Color Red Channel"
    green:
      name: "Color Green Channel"
    blue:
      name: "Color Blue Channel"
    color_temperature:
      name: "CCT Illuminance"
```

Four photodiodes with coloured filters integrate simultaneously; the clear
channel is unfiltered and is what makes lux calculable. The part can also cut
its IR filter in, which matters because incandescent light leaks into all three
colour channels and skews CCT.

## espforge

`tcs34725-color.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0`, `address: 0x29` | `<i2c id="i2c0" sda="4" scl="5" frequency="400kHz" />`, `ADDRESS` |
| `gain: 4x` | gain register 0x0F, value 0x01 |
| `integration_time: 50ms` | timing register 0x01, value 0x01 (50 ms) |
| RGBC result | eight bytes from 0x14, clear first |
| `red` / `green` / `blue` | 16-bit channels |
| lux, CCT | `lux()` and `colour_temperature()` in `app.rs` |
| `update_interval` | `delay_ms(2_000)` |
| `name:` | none |

Both registers and the conversion are real, so this is 🟡. CCT here is the
simplified McCamy approximation, which is fine for "warm or cool" and not for
a calibrated measurement — worth saying plainly in a block about chromaticity.

### What is missing

A `<tcs34725>` sensor with per-channel entities, gain and integration time as
attributes, and the IR-filter cut as a separate attribute.
