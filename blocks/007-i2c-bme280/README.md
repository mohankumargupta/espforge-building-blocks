# 007 — I²C Bus & Multi-Value Sensors

**Status:** 🟡 partly possible — I²C and a multi-value driver (`<scd4x>`) work
today; `<bme280>` does not exist
**ESPHome lego:** `i2c:` + `sensor: platform: bme280` with per-metric children

## ESPHome

```yaml
i2c:
  sda: GPIO21
  scl: GPIO22
  scan: true

sensor:
  - platform: bme280
    address: 0x76
    update_interval: 60s
    temperature:
      name: "Room Temperature"
    humidity:
      name: "Room Humidity"
```

One bus, one part, several entities: the driver owns the bus and publishes a
temperature and a humidity reading with independent names.

## espforge

`i2c-bme280.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c.sda/scl` | `<i2c id="i2c0" sda="21" scl="22" />` (declare `frequency` for a faster bus) |
| `i2c.scan: true` | none: bus scanning is an example (`i2c_scanner`), not a bus attribute |
| `sensor: platform: bme280, address` | proposed `<bme280 id="climate" bus="$i2c0" address="0x76" />` |
| `temperature.name` / `humidity.name` | one device handle returning both values; `name`s do not exist |
| `update_interval` | a `delay_ms(60_000)` in `forever` |

`src/app.rs` reads both metrics from one `climate` handle and logs them. A
multi-value driver fits espforge well: one element, one bus claim, one device
field, several readings.

### Working substitute today

The bundled SCD4x driver is the same lego block (an I²C part with several
readings, with an address fixed by the part):

```xml
<i2c id="i2c0" sda="21" scl="22" frequency="100kHz" />
<scd4x id="climate" bus="$i2c0" address="0x62" />
```

```rust
let climate = device!(ctx, climate);
climate.start()?;
let reading = climate.read()?;   // co2_ppm, temperature_centi_c, humidity_centi_percent
```

The gap is the part, not the architecture: `<bme280>` needs a driver manifest
plus a Rust driver returning `temperature_c` / `humidity_percent`, and the
multi-value shape is already proven by SCD4x.
