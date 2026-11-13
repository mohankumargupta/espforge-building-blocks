# 075 — I²C Time-of-Flight Laser Distance Ranging (VL53L0X)

**Status:** ❌ not possible yet
**ESPHome lego:** `sensor: platform: vl53l0x`

## ESPHome

```yaml
sensor:
  - platform: vl53l0x
    name: "Water Tank Headspace Distance"
    i2c_id: i2c0
    address: 0x29
    update_interval: 500ms
    enable_pin: GPIO2
```

A VL53L0X measures flight time of infrared photons against a single-photon
avalanche diode array and reports millimetres. Getting there is not a register
read: the part needs a tuning sequence of dozens of register writes, a
reference calibration, and a range-profile choice, and it must be held in reset
(`XSHUT`) while the bus is brought up.

## espforge

`vl53l0x.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0`, `address: 0x29` | `<i2c id="i2c0" sda="4" scl="5" />`, `ADDRESS` |
| `enable_pin: GPIO2` | `<led id="tof_xshut" pin="2" />` drives the reset line |
| `platform: vl53l0x` | none: no driver, and no tuning sequence |
| single range read | the 16-bit result register, if the part were initialised |
| `update_interval: 500ms` | `delay_ms(500)` |
| `name:` | none; the distance is logged |

The bus and the reset pin are real, but unlike the MAX6675 (block 025) the
missing part is a whole driver: a mandatory init table, reference calibration
and the range-profile budget. A partial implementation returns zeros.

### What is missing

A `<vl53l0x>` driver with its tuning sequence, calibration and a declared
sensor entity. Blocks 075 and 052 are the two places where "the bus works" is
not enough to make a block 🟡.
