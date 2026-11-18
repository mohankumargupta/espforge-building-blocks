# BME280 on I²C with a project-local driver

An espforge project whose `<bmxx80>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/bmxx80.kdl`, a project-local
manifest, and the Rust half is the `bmxx80-driver` crate beside it. It runs
against the emulator's virtual BME280 I²C part, so the same generated firmware
talks to `parts/bmxx80.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `bmxx80` after `bh1750`, which is already
here. Only the main happy path is implemented: check the chip, configure
normal-mode measurements, read compensated temperature, pressure and humidity
once a second.

## What is here

| File | What it is |
|---|---|
| `bmxx80-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<bmxx80 id="weather" bus="$i2c0" address="0x76" />` |
| `src/app.rs` | reads the weather once a second and logs `TEST:` lines |
| `drivers/bmxx80.kdl` | **the external driver**: `dependency "bmxx80-driver" { path "../bmxx80-driver" }`, no `codegen.feature` |
| `bmxx80-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/bmxx80/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/bmxx80.wasm` | the built virtual part, from `just bmxx80-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/bmxx80/drivers/bmxx80.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just bmxx80-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the default weather (25.00 C, 1013.25 hPa, 45.00 %RH)
PASS: set:temperature=30 -> the reading moves
PASS: set:humidity=60 -> the reading moves again
bmxx80-sensor PASS
```

## The steps one at a time

```sh
just bmxx80-part-test      # the registers, the calibration and the inputs, on the host
just bmxx80-part           # -> parts/bmxx80.wasm
just bmxx80-driver-test    # the chip check, the configuration and the compensation, on the host
just bmxx80-check          # manifest discovered, lowers, `path` dependency resolves
just bmxx80-image          # the generated project compiles -> build/bmxx80/bmxx80.bin
just bmxx80-run            # run with the control channel on 127.0.0.1:5573
```

In a second terminal, while `bmxx80-run` is going:

```sh
just bmxx80-part-input "part list"
just bmxx80-part-input "part weather state"
just bmxx80-part-input "part weather input set:temperature=30"
just bmxx80-part-input "part weather input set:pressure=1000"
just bmxx80-part-input "part weather input set:humidity=60"
just bmxx80-part-input "part weather input rest"
```

After editing `drivers/bmxx80.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just bmxx80-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x76 (SDO low; 0x77 with SDO high). Every transaction is a
register address byte followed by the read or write, as the datasheet's I²C
protocol describes.

| Register | Meaning |
|---|---|
| `0xD0` | chip id, `0x60` for a BME280 |
| `0xE0` | reset: write `0xB6` |
| `0x88..0xA1` | calibration, first block: T1..T3, P1..P9, H1 |
| `0xE1..0xE7` | calibration, second block: H2..H6 |
| `0xF2` | `ctrl_hum`: humidity oversampling |
| `0xF3` | status: bit 3 measuring, bit 0 im_update |
| `0xF4` | `ctrl_meas`: temperature and pressure oversampling, mode |
| `0xF5` | `config`: standby, filter, SPI mode |
| `0xF7..0xFE` | data: pressure, temperature (20-bit, left-aligned) and humidity |

`new()` reads the chip id, soft-resets the part, reads both calibration
blocks, sets humidity oversampling x1, then temperature and pressure
oversampling x1 with normal mode (`0x27`). `read()` takes the eight data
registers and applies the datasheet's integer compensation:

- temperature is `(t_fine * 5 + 128) >> 8` in 0.01 °C,
- pressure is in pascals, so the driver divides by 100 for hPa,
- humidity is `%RH * 1024`, so the driver divides by 1024.

The part's calibration is chosen so those formulas invert exactly:
`dig_T1 = 0`, `dig_T2 = 4096` gives `t_fine = adc_T / 4`; `dig_P1 = 6250` with
the rest zero gives `p = 1048576 - adc_P` pascals; `dig_H2 = 16384` with the
rest zero gives `%RH * 1024 = (adc_H + 1) * 256`. Real chips have different
constants, but the driver runs the datasheet formulas unchanged.

The attach line is in `build/bmxx80/emu.log` (the firmware console is
`build/bmxx80/sensor.stdout`):

```text
co-simulation: part weather (bmxx80) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x76 on GPIO 6/5
```

## Limits

- Happy path only: no negative boards, no `init="deferred"`, no BMP280 or
  BME680 variants, no SPI, no forced mode, no standby/filter/IIR settings.
- No timing: a real part's normal-mode measurement takes 1.5 ms plus the
  standby time. The part model encodes the registers the moment they are read,
  and reports `status` as never measuring.
- The pressure and humidity compensation are exact for this part's published
  calibration; on a real chip the pascals and `%RH * 1024` values are the same
  formulas with its own constants.
- The temperature is quantised to 0.05 °C by the part's `t_fine`, and the
  humidity to 0.25 %RH, because the raw registers are integers.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just bmxx80-clean
```
