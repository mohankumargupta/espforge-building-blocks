# 055 — Metal-Oxide Gas Sensing & VOC Index Algorithms

**Status:** 🟡 partly possible — the I²C bus reaches the SGP40 and the raw signal is decodable; the index transform is missing
**ESPHome lego:** `sensor: platform: sgp40`

## ESPHome

```yaml
sensor:
  - platform: sgp40
    name: "VOC Index"
    i2c_id: i2c0
    address: 0x59
    compensation:
      temperature: room_temperature
      humidity: room_humidity
    update_interval: 10s
```

The SGP40 is not a VOC sensor in the usual sense: its hotplate resistance gives
a raw tick count that drifts with absolute humidity, and Sensirion's algorithm
turns that count into a 0–500 index over hours of baseline learning. Temperature
and humidity are inputs, not decoration.

## espforge

`voc-index.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0` | `<i2c id="i2c0" sda="4" scl="5" />` |
| `address: 0x59` | `ADDRESS` |
| `compensation: temperature / humidity` | one real `<scd4x>` reading for both |
| `update_interval: 10s` | `delay_ms(10_000)` |
| `platform: sgp40` | none: no driver, so `measure_raw` and its CRC8 are here |
| index 0–500 | none: the algorithm needs a long baseline and is not in the firmware |

The raw signal is real: a 0x260F command with the absolute-humidity ticks,
CRC8-checked on both the command and the two result words. The index itself is
the missing half — it is a stateful model, not a formula.

### What is missing

An `<sgp40>` driver with the command/CRC layer, plus a VOC-index model (or a
raw-resistance sensor) and the publish path for it.
