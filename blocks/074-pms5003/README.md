# 074 — Laser Particulate Air Quality Sensor (PMS5003)

**Status:** 🟡 partly possible — the UART is real; the 32-byte frame parser is in `app.rs`
**ESPHome lego:** `sensor: platform: pmsx003`

## ESPHome

```yaml
uart:
  id: pms_uart
  tx_pin: GPIO21
  rx_pin: GPIO20
  baud_rate: 9600

sensor:
  - platform: pmsx003
    type: PMS5003
    uart_id: pms_uart
    pm_1_0:
      name: "Particulate Matter PM1.0"
    pm_2_5:
      name: "Particulate Matter PM2.5"
    pm_10_0:
      name: "Particulate Matter PM10.0"
```

A laser scatters light off particles and counts pulses per size bin. The module
streams 32-byte frames: magic `0x42 0x4D`, a length word, two sets of three
concentration values (atmospheric then "standard"), and a checksum that is the
sum of the first 30 bytes.

## espforge

`pms5003.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `uart: tx_pin / rx_pin` | `<uart id="uart1" uart="UART1" tx="0" rx="1" baud_rate="9600" />` — GPIO20/21 are UART0, the console |
| `baud_rate: 9600` | `baud_rate="9600"` |
| `type: PMS5003` | frame length 32, magic `0x424D` |
| `pm_1_0` / `pm_2_5` / `pm_10_0` | 16-bit values at offsets 10, 12, 14 (atmospheric set) |
| `platform: pmsx003` | none: no driver |
| checksum | the sum of bytes 0..30 in `app.rs` |
| `name:` | none; the three values are logged |
| sleep pin | none: no standby/wake control, so the fan runs continuously |

The module streams about once a second, so a 32-byte read with a short timeout
is reliable. This is the same shape as block 016's MHZ19, which is why it is 🟡.

### What is missing

A `<pmsx003>` driver that owns the UART, decodes both concentration sets, and
manages the SET/SLEEP pin for duty-cycled operation.
