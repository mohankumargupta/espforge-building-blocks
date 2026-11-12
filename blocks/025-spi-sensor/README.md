# 025 — SPI Bus & High-Speed Sensor Transceivers

**Status:** 🟡 partly possible — `<spi>` and a raw chip-select device work; the MAX6675 protocol lives in `app.rs`
**ESPHome lego:** `spi:` + `sensor: platform: max6675`

## ESPHome

```yaml
spi:
  clk_pin: GPIO6
  miso_pin: GPIO2
  mosi_pin: GPIO7

sensor:
  - platform: max6675
    name: "Furnace Thermocouple"
    cs_pin: GPIO10
    update_interval: 5s
```

A MAX6675 is a read-only SPI slave: assert CS, clock out 16 bits, release CS.
Twelve of those bits are the thermocouple temperature in 0.25 °C steps, and one
bit reports an open thermocouple. The bus is shared, so each part owns its own
chip select and clock configuration.

## espforge

`spi-sensor.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `spi.clk_pin` / `mosi_pin` / `miso_pin` | `<spi id="spi2" sclk="6" mosi="7" miso="2" cs="10" mode="0" frequency="4000kHz" />` |
| `sensor: platform: max6675` | none: no `<max6675>` driver; the 16-bit read and conversion are in `app.rs` |
| `cs_pin` | `cs="10"` on the `<spi>`: a bus with no device gets a raw `SpiDevice` owning that line |
| `update_interval: 5s` | a `delay_ms(5_000)` in `forever` |
| `name:` | none; `device!(ctx, spi2)` is the handle |

`<spi>` with a `cs` but no device is the raw-bus case: the generated `Context`
carries `spi2: SpiDevice`, and `write_read` asserts CS around the transfer. The
app shifts out two dummy bytes, then converts:

- bits 15..3 are the 12-bit temperature, `raw >> 3`, at 0.25 °C per count;
- bit 2 is set when the thermocouple is open.

`GPIO2` is a strapping pin on the ESP32-C3, so the compiler warns about it; the
MAX6675 wiring uses the SPI2 default pins, where that warning is expected.

### What is missing

Only the driver: `<max6675 id="furnace" bus="$spi2" cs="10" />` exposing
`temperature_c()` and an open-thermocouple error. Bus arbitration, per-device
mode/frequency and CS ownership already exist, which is why this block is a
small driver rather than new architecture.
