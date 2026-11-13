# 083 — Soil Moisture Capacitive Probing & Piecewise Calibration

**Status:** ❌ not possible yet
**ESPHome lego:** `sensor: platform: adc`

## ESPHome

```yaml
sensor:
  - platform: adc
    pin: GPIO0
    name: "Soil Moisture"
    unit_of_measurement: "%"
    update_interval: 30s
    attenuation: 11db
    filters:
      - calibrate_linear:
          - 2.45 -> 0.0   # Completely dry in free air
          - 1.10 -> 100.0 # Submerged in cup of water
      - lambda: |-
          if (x < 0.0) return 0.0;
          if (x > 100.0) return 100.0;
          return x;
```

A capacitive probe outputs a voltage that falls as the soil wets: 2.45 V in
dry air, 1.10 V in water. The 11 dB attenuation setting is what makes that
range measurable at all — the raw ADC full scale is about 1.1 V.

## espforge

`soil-moisture.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin: GPIO0` | proposed `<adc id="adc1" attenuation="11dB" resolution="12bit" />` peripheral |
| `attenuation: 11db` | the attribute that sets the input range |
| the reading | proposed `<analog_sensor id="soil" adc="$adc1" channel="0" />` |
| `calibrate_linear: 2.45 -> 0, 1.10 -> 100` | `DRY_VOLTS` / `WET_VOLTS` in `app.rs` |
| the clamp lambda | `clamp(0, 100)` |
| `update_interval: 30s` | `delay_ms(30_000)` |
| `name:` | none |

There is no ADC in the vocabulary at all: `<gpio>`, `<i2c>`, `<spi>`, `<uart>`
and `<wifi>` are the whole `# peripherals` section. Even the calibration cannot
run, because nothing produces a voltage. Block 006 is the same gap.

### What is missing

An `<adc>` peripheral (unit, attenuation, resolution) and an analog sensor
element, plus the ADC2/Wi-Fi interlock from `top5structural.md` — on the C3
the ADC unit is shared with the radio.
