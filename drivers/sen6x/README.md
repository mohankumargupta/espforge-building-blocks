# SEN6x on I²C with a project-local driver

An espforge project whose `<sen6x>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/sen6x.kdl`, a project-local
manifest, and the Rust half is the `sen6x-driver` crate beside it. It runs
against the emulator's virtual SEN6x I²C part, so the same generated firmware
talks to `parts/sen6x.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `sen6x` after `scd4x`, which was the
previous fixture. Only the main happy path is implemented: start continuous
measurement and read the nine SEN66 values once a second.

## What is here

| File | What it is |
|---|---|
| `sen6x-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<sen6x id="air" bus="$i2c0" address="0x6B" />` |
| `src/app.rs` | takes one reading a second and logs `TEST:` lines |
| `drivers/sen6x.kdl` | **the external driver**: `dependency "sen6x-driver" { path "../sen6x-driver" }`, no `codegen.feature` |
| `sen6x-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/sen6x/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/sen6x.wasm` | the built virtual part, from `just sen6x-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/sen6x/drivers/sen6x.kdl` — the nested `drivers/` is the tier, not a
typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just sen6x-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the default indoor air (8.0 / 12.5 / 15.0 / 20.0 µg/m³)
PASS: the default 22.50 °C / 45.00 %RH / 400 ppm
PASS: set:pm2-5=50 -> 50.0 µg/m³
PASS: set:co2=1000 -> 1000 ppm
PASS: set temperature and humidity -> 30.00 °C / 60.00 %RH
sen6x-sensor PASS
```

## The steps one at a time

```sh
just sen6x-part-test        # the encoding, the CRC and the inputs, on the host
just sen6x-part             # -> parts/sen6x.wasm
just sen6x-driver-test      # the commands, the CRC check and the decode, on the host
just sen6x-check            # manifest discovered, lowers, `path` dependency resolves
just sen6x-image            # the generated project compiles -> build/sen6x/sen6x.bin
just sen6x-run              # run with the control channel on 127.0.0.1:5582
```

In a second terminal, while `sen6x-run` is going:

```sh
just sen6x-part-input "part list"
just sen6x-part-input "part air state"
just sen6x-part-input "part air input set:pm2-5=50"
just sen6x-part-input "part air input set:co2=1000"
just sen6x-part-input "part air input set:temperature=30"
just sen6x-part-input "part air input set:humidity=60"
just sen6x-part-input "part air input rest"
```

After editing `drivers/sen6x.kdl` — even a comment — the lock is stale and the
next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just sen6x-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x6B, its only address. It is command-driven and has no
register pointer; a command is two bytes, big-endian, without a CRC byte:

| Command | Meaning |
|---|---|
| `0x0021` | start continuous measurement |
| `0x0104` | stop continuous measurement |
| `0x0300` | select the SEN66 measured values for the next read |

`new()` starts continuous measurement. `read()` sends `0x0300` and then reads
27 bytes: nine 16-bit big-endian words, each followed by a CRC-8 (poly `0x31`,
init `0xFF`):

| Word | Meaning |
|---|---|
| 1–4 | PM1.0, PM2.5, PM4.0, PM10.0, `/ 10` µg/m³ |
| 5 | ambient temperature, `/ 200` °C |
| 6 | ambient humidity, `/ 100` % |
| 7–8 | VOC index, NOx index, `/ 10` |
| 9 | CO2 in ppm |

The default fixtures encode as `0x0050`, `0x007D`, `0x0096`, `0x00C8`,
`0x1194`, `0x1194`, `0x03E8`, `0x000A` and `0x0190`: 8.0, 12.5, 15.0 and
20.0 µg/m³, 22.50 °C, 45.00 %RH, VOC 100.0, NOx 1.0 and 400 ppm. Before
`start`, every value reads at its unknown upper limit (`0xFFFF` or `0x7FFF`).

The attach line is in `build/sen6x/emu.log` (the firmware console is
`build/sen6x/sensor.stdout`):

```text
co-simulation: part air (sen6x) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x6B on GPIO 6/5
```

## Limits

- Happy path only: no data-ready polling, stop, reset, product name, serial
  number, device status, fan cleaning, temperature offset or CO2
  recalibration, no `init="deferred"`, no negative boards.
- No measurement timing: the part answers at once whatever its interval, and
  the driver waits for none of it.
- Only the SEN66 measured-values layout (`0x0300`) is modelled; SEN62, SEN63C,
  SEN65, SEN68 and SEN69C use other commands and payloads.
- The part's inputs are held to the datasheet ranges; there is no noise,
  drift or cross-sensitivity.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just sen6x-clean
```
