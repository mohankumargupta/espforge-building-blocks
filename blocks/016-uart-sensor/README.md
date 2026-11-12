# 016 — Hardware UART & Serial Sensor Parsing

**Status:** 🟡 partly possible — `<uart>` works; the MHZ19 protocol lives in `app.rs`
**ESPHome lego:** `uart:` + `sensor: platform: mhz19`

## ESPHome

```yaml
uart:
  id: uart_bus
  tx_pin: GPIO21
  rx_pin: GPIO20
  baud_rate: 9600

sensor:
  - platform: mhz19
    uart_id: uart_bus
    co2:
      name: "CO2 Concentration"
    temperature:
      name: "Sensor Temperature"
    update_interval: 30s
```

A UART sensor is a protocol, not just a pin: request a measurement, read a
fixed-length frame, validate its checksum, then split the payload into
entities.

## espforge

`uart-sensor.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `uart.id` / `tx_pin` / `rx_pin` / `baud_rate` | `<uart id="uart1" uart="UART1" tx="6" rx="7" baud_rate="9600" />` |
| — | ESPHome's GPIO21/20 are UART0's default pins; the console owns UART0 here, so the sensor moves to UART1 |
| `sensor: platform: mhz19` | none: no `<mhz19>` driver; framing and checksum are in `app.rs` |
| `co2` / `temperature` children | one parsed frame yields both values; `name`s do not exist |
| `update_interval: 30s` | a `delay_ms(30_000)` in `forever` |
| checksum and framing | Rust: `checksum()` and `parse()` |

The framing fits the runtime's byte API: `write_all` sends the 9-byte read
command and `read` fills the response one chunk at a time. A real driver would
bury the same code behind `read()`; this block keeps it visible because that is
exactly the part a driver would own.

### What is missing

Only the driver: `<mhz19 bus="$uart1">` exposing `co2` and `temperature`. The
transport is real, and validation is three small functions. `UartDevice` has no
timeout helper, which is why the read loop counts its own polls.
