# BME280 on I²C with a project-local driver

An espforge project whose `<bme280_i2c>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/bme280_i2c.kdl`, a
project-local manifest, and the Rust half is the `bme280-i2c-driver` crate
beside it. It runs against the emulator's virtual BME280 I²C part, so the same
generated firmware talks to `parts/bme280_i2c.wasm` with no sensor crate glued
in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `drivers/bme280_spi` is the same part on SPI; `blocks/007-i2c-bme280`
is the block that proposed a bundled `<bme280>`.

## What is here

| File | What it is |
|---|---|
| `bme280_i2c-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<bme280_i2c id="climate" bus="$i2c0" address="0x76" />` |
| `src/app.rs` | reads once a second and logs `TEST: climate t=<°C> c p=<hPa> hpa rh=<%RH> %` |
| `drivers/bme280_i2c.kdl` | **the external driver**: `dependency "bme280-i2c-driver" { path "../bme280-i2c-driver" }`, no `codegen.feature` |
| `bme280-i2c-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/bme280_i2c/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/bme280_i2c.wasm` | the built virtual part, from `just bme280-i2c-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5, address 0x76 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the project tier is
`drivers/bme280_i2c/drivers/bme280_i2c.kdl` — the nested `drivers/` is the
tier, not a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just bme280-i2c-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: room conditions
PASS: warmer -> 23.50 C
PASS: set:pressure=1000 -> 1000.00 hPa
PASS: set:humidity=62 -> 62.00 %RH
bme280-i2c-sensor PASS
```

## The steps one at a time

```sh
just bme280-i2c-driver-test   # the identity check and the compensation, on the host
just bme280-i2c-part-test     # the register file and the inputs, on the host
just bme280-i2c-part          # -> parts/bme280_i2c.wasm
just bme280-i2c-check         # manifest discovered, lowers, `path` dependency resolves
just bme280-i2c-image         # the generated project compiles -> build/bme280_i2c/bme280_i2c.bin
just bme280-i2c-run           # run with the control channel on 127.0.0.1:5566
```

In a second terminal, while `bme280-i2c-run` is going:

```sh
just bme280-i2c-part-input "part list"
just bme280-i2c-part-input "part climate state"
just bme280-i2c-part-input "part climate input warmer"
just bme280-i2c-part-input "part climate input higher"
just bme280-i2c-part-input "part climate input wetter"
just bme280-i2c-part-input "part climate input set:pressure=1000"
```

After editing `drivers/bme280_i2c.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just bme280-i2c-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The driver writes the register pointer (one byte, no value) and does the read
as one combined transaction: `ID` (`0xD0`) must be `0x60`, then the calibration
block (0x88..=0xA1 and 0xE1..=0xE7), then `ctrl_hum`/`config`/`ctrl_meas` put
the chip in normal mode. Each reading is the eight bytes at `press_msb`
(`0xF7`), pressure/temperature/humidity, decoded with Bosch's compensation
formulas.

The attach line is in `build/bme280_i2c/emu.log` (the firmware console is
`build/bme280_i2c/sensor.stdout`):

```text
co-simulation: part climate (bme280_i2c) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x76 on GPIO 6/5
```

## Limits

- Happy path only: no negative boards, no `init="deferred"`, no `0x77` variant
  (both the manifest and the part support it; only the checked-in board and
  `.espf.txt` are at 0x76).
- The part never models conversion timing, sleep mode or oversampling: a
  reading is ready whenever the driver asks.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just bme280-i2c-clean
```
