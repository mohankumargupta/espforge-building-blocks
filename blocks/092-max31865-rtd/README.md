# 092 — High-Precision Platinum RTD Temperature (MAX31865)

**Status:** 🟡 partly possible — the SPI bus and the RTD registers are real; the Callendar-Van Dusen math is in `app.rs`
**ESPHome lego:** `sensor: platform: max31865`

## ESPHome

```yaml
spi:
  clk_pin: GPIO6
  miso_pin: GPIO2
  mosi_pin: GPIO7

sensor:
  - platform: max31865
    name: "High-Temp Flue Probe"
    cs_pin: GPIO10
    reference_resistor: 430.0 ohm
    rtd_nominal_resistance: 100.0 ohm
    rtd_wires: 3
    update_interval: 2s
```

A PT100 changes resistance by about 0.385 Ω/°C, so a flue probe at 300 °C is
~215 Ω against a 430 Ω reference. The MAX31865 measures that ratio with a
15-bit delta-sigma ADC and reports it as a 15-bit code; converting the ratio to
temperature is the Callendar-Van Dusen equation, done on the host side.

## espforge

`max31865-rtd.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `spi:` bus | `<spi id="spi2" sclk="6" mosi="7" miso="2" mode="1" frequency="1000kHz" />` |
| `cs_pin: GPIO10` | `cs="10"` on the bus; the driver would own its own CS |
| `reference_resistor: 430.0 ohm` | `REFERENCE_OHMS` |
| `rtd_nominal_resistance: 100.0 ohm` | `NOMINAL_OHMS` |
| `rtd_wires: 3` | configuration register: 3-wire bit, 50/60 Hz filter |
| 15-bit code | registers 0x01/0x02, fault bit in the high register |
| Callendar-Van Dusen | `rtd_to_celsius` in `app.rs` |
| `update_interval: 2s` | `delay_ms(2_000)` |
| `name:` | none |

The register access is a two-byte SPI transaction and the conversion is a
quadratic, so the app is a real driver minus the entity. SPI mode 1 is not a
detail: the part samples on the falling edge of the clock.

### What is missing

A `<max31865>` sensor element that owns the configuration register, the wire
count, the fault flags and the conversion, with the RTD type as an attribute.
