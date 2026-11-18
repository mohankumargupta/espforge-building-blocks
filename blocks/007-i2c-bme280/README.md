# 007 — I²C Bus & Multi-Value Sensors

**Status:** ✅ possible today — I²C plus the project-local `<bme280_i2c>` driver
**ESPHome lego:** `i2c:` + `sensor: platform: bme280` with per-metric children

## ESPHome

```yaml
i2c:
  sda: GPIO4
  scl: GPIO5
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
| `i2c.sda/scl` | `<i2c id="i2c0" sda="4" scl="5" />` (declare `frequency` for a faster bus) |
| `i2c.scan: true` | none: bus scanning is an example (`i2c_scanner`), not a bus attribute |
| `sensor: platform: bme280, address` | `<bme280_i2c id="climate" bus="$i2c0" address="0x76" />` |
| `temperature.name` / `humidity.name` | one device handle returning both values; `name`s do not exist |
| `update_interval` | a `delay_ms(60_000)` in `forever` |

`src/app.rs` reads both metrics from one `climate` handle and logs them. A
multi-value driver fits espforge well: one element, one bus claim, one device
field, several readings.

### The driver

There is no bundled `<bme280>` element. The driver is project-local, exactly
the shape `drivers/bme280_i2c` uses:

- `drivers/bme280_i2c.kdl` — the manifest: an I²C device at `0x76..=0x77`,
  constructed as `bme280_i2c_driver::Bme280I2c::new(...)`, with no
  `codegen.feature`.
- `bme280-i2c-driver/` — the Rust half, a copy of the crate in
  `drivers/bme280_i2c/`. It is copied rather than referenced because espforge
  refuses a `dependency ... path` that escapes the project directory.

`Cargo.extra.toml` carries the I²C overlay described in
`i2c-drivers-workaround.md`: without it every transaction fails
`AcknowledgeCheckFailed(Data)` under `esp-emu`, and the constructor never
returns.

```sh
just check blocks/007-i2c-bme280    # schema, driver discovery, lock
just build blocks/007-i2c-bme280    # generate and compile
```

The first `check` writes `.espforge/drivers.lock`. Editing the manifest after
that is `DRIVER_LOCK_STALE`; re-record it with
`just espforge "driver blocks/007-i2c-bme280 add --local blocks/007-i2c-bme280/drivers/bme280_i2c.kdl"`.
Editing the crate, `src/app.rs` or the `.espf.txt` needs no re-record.

For the part on a virtual board, `drivers/bme280_i2c` is the full fixture: the
same driver against `parts/bme280_i2c.wasm`, with `just bme280-i2c-sensor`.
