# SGP30 on I²C with a project-local driver

An espforge project whose `<sgp30>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/sgp30.kdl`, a project-local
manifest, and the Rust half is the `sgp30-driver` crate beside it. It runs
against the emulator's virtual SGP30 I²C part, so the same generated firmware
talks to `parts/sgp30.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `sgp30` after `sen6x`, which was the
previous fixture. Only the main happy path is implemented: initialise air
quality measurement and read eCO2 and TVOC once a second.

## What is here

| File | What it is |
|---|---|
| `sgp30-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<sgp30 id="air" bus="$i2c0" address="0x58" />` |
| `src/app.rs` | takes one reading a second and logs `TEST:` lines |
| `drivers/sgp30.kdl` | **the external driver**: `dependency "sgp30-driver" { path "../sgp30-driver" }`, no `codegen.feature` |
| `sgp30-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/sgp30/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/sgp30.wasm` | the built virtual part, from `just sgp30-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/sgp30/drivers/sgp30.kdl` — the nested `drivers/` is the tier, not a
typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just sgp30-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the default 400 ppm / 0 ppb
PASS: set:eco2=1000 -> 1000 ppm
PASS: set:tvoc=137 -> 137 ppb
sgp30-sensor PASS
```

## The steps one at a time

```sh
just sgp30-part-test        # the encoding, the CRC and the inputs, on the host
just sgp30-part             # -> parts/sgp30.wasm
just sgp30-driver-test      # the commands, the CRC check and the decode, on the host
just sgp30-check            # manifest discovered, lowers, `path` dependency resolves
just sgp30-image            # the generated project compiles -> build/sgp30/sgp30.bin
just sgp30-run              # run with the control channel on 127.0.0.1:5583
```

In a second terminal, while `sgp30-run` is going:

```sh
just sgp30-part-input "part list"
just sgp30-part-input "part air state"
just sgp30-part-input "part air input set:eco2=1000"
just sgp30-part-input "part air input set:tvoc=137"
just sgp30-part-input "part air input rest"
```

After editing `drivers/sgp30.kdl` — even a comment — the lock is stale and the
next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just sgp30-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x58, its only address. It is command-driven and has no
register pointer; a command is a two-byte big-endian word followed by its
CRC-8:

| Command | Meaning |
|---|---|
| `0x2003` | initialise air quality measurement |
| `0x2008` | select the IAQ measurement for the next read |

`new()` sends `0x2003` and its CRC. `read()` sends `0x2008` and its CRC, then
reads six bytes: eCO2 in ppm and TVOC in ppb, each 16-bit big-endian followed
by a CRC (poly `0x31`, init `0xFF`).

| Word | Meaning |
|---|---|
| 1 | equivalent CO2, ppm |
| 2 | total volatile organic compounds, ppb |

The default fixtures encode as `0x0190` and `0x0000`: 400 ppm and 0 ppb. A
command whose CRC byte does not match is acknowledged and ignored, as an
unknown command is.

The attach line is in `build/sgp30/emu.log` (the firmware console is
`build/sgp30/sensor.stdout`):

```text
co-simulation: part air (sgp30) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x58 on GPIO 6/5
```

## Limits

- Happy path only: no IAQ baseline read or write, no absolute-humidity
  compensation, no raw H2/ethanol signals, no feature-set version, no
  self-test, no `init="deferred"`, no negative boards.
- No measurement timing: a real SGP30 wants 15 s between initialisation and
  the first measurement and 12 ms per measurement, and the part answers at
  once whatever its interval.
- The part's inputs are held to the datasheet ranges; there is no noise,
  drift or cross-sensitivity.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just sgp30-clean
```
