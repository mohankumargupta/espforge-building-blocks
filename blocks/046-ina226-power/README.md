# 046 — High-Precision I²C Shunt Power Monitor (INA226)

**Status:** 🟡 partly possible — the I²C bus and raw register access are real; the scaling is in `app.rs`
**ESPHome lego:** `sensor: platform: ina226`

## ESPHome

```yaml
sensor:
  - platform: ina226
    address: 0x40
    i2c_id: i2c0
    shunt_resistance: 0.002 ohm
    max_current: 20.0A
    bus_voltage:
      name: "Bus Voltage"
    current:
      name: "Load Current"
    power:
      name: "Total Power"
    update_interval: 5s
```

The INA226 measures the differential voltage across a shunt and the bus
voltage to ground. Both are raw 16-bit registers with fixed LSBs (2.5 µV and
1.25 mV); current and power only become meaningful after the calibration
register is written from the shunt value and the expected maximum current.

## espforge

`ina226-power.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0` | `<i2c id="i2c0" sda="4" scl="5" />` |
| `address: 0x40` | `ADDRESS` |
| `shunt_resistance: 0.002 ohm` | `SHUNT_OHMS`, which sets `CURRENT_LSB` |
| `max_current: 20.0A` | `MAX_CURRENT_A`; `CURRENT_LSB = max / 32768` |
| `current_lsb` and the calibration register | `0.00512 / (CURRENT_LSB * SHUNT_OHMS)`, written to 0x05 |
| `bus_voltage` / `current` / `power` | registers 0x02, 0x04, 0x03 decoded by hand |
| `update_interval: 5s` | `delay_ms(5_000)` |
| `name:` | none; the readings are logged |

Averaging and conversion time come from the configuration register (0x00),
which the app writes once in `setup`: 16 averages over 1.1 ms conversions.

### What is missing

An `<ina226>` device with declared sub-sensors, so the current and power
registers are computed by the driver and published like any other reading.
Block 082 makes the same raw-bus argument for a single-register part.
