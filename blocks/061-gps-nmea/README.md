# 061 — NMEA 0183 GPS Positioning & Sentence Decoding

**Status:** 🟡 partly possible — the UART is real; the NMEA parser is in `app.rs`
**ESPHome lego:** `uart:` + `gps:`

## ESPHome

```yaml
uart:
  id: gps_uart
  rx_pin: GPIO20
  tx_pin: GPIO21
  baud_rate: 9600

gps:
  id: gnss_module
  uart_id: gps_uart

sensor:
  - platform: gps
    gps_id: gnss_module
    latitude:
      name: "Node Latitude"
    longitude:
      name: "Node Longitude"
    altitude:
      name: "Node Altitude"
    satellites:
      name: "Visible Satellites"
```

A GNSS module streams ASCII sentences at 1 Hz: `$GPRMC` carries time, status
and position, `$GPGGA` carries altitude and the satellite count. Every sentence
ends with `*` and a two-digit XOR checksum.

## espforge

`gps-nmea.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `uart: rx_pin: GPIO20, tx_pin: GPIO21` | `<uart id="uart1" uart="UART1" tx="0" rx="1" baud_rate="9600" />` — GPIO20/21 are UART0, the console |
| `baud_rate: 9600` | `baud_rate="9600"` |
| `gps: platform` | proposed `<gps id="gnss" uart="$uart1" />` |
| `latitude` / `longitude` | `$GPRMC`: `ddmm.mmmm` plus the hemisphere letter |
| `altitude` / `satellites` | `$GPGGA` fields 9 and 7 |
| `name:` | none; the fix is logged |
| sentence checksum | XOR of the bytes between `$` and `*`, verified here |

Reading is line-based (`read_line`), which is what a 1 Hz ASCII stream wants.
The parser only accepts `$GPRMC` with an `A` status, so a fix that has not
settled does not overwrite a good position with zeros.

### What is missing

A `<gps>` device that owns the UART, parses all the sentence types it needs
(`$GPGSA`, `$GPGSV`), and publishes latitude/longitude/altitude/satellites as
entities.
