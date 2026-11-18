# HDC302x on I²C with a project-local driver

An espforge project whose `<hdc302x>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/hdc302x.kdl`, a project-local
manifest, and the Rust half is the `hdc302x-driver` crate beside it. It runs
against the emulator's virtual HDC302x I²C part, so the same generated firmware
talks to `parts/hdc302x.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `hdc302x` after `ccs811`, which is already
here. Only the main happy path is implemented: soft-reset the part, trigger one
on-demand measurement a second, read °C and %RH.

## What is here

| File | What it is |
|---|---|
| `hdc302x-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<hdc302x id="climate" bus="$i2c0" address="0x44" />` |
| `src/app.rs` | takes one measurement a second and logs `TEST:` lines |
| `drivers/hdc302x.kdl` | **the external driver**: `dependency "hdc302x-driver" { path "../hdc302x-driver" }`, no `codegen.feature` |
| `hdc302x-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/hdc302x/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/hdc302x.wasm` | the built virtual part, from `just hdc302x-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/hdc302x/drivers/hdc302x.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just hdc302x-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the default room (22.50 C, 45.00 %RH)
PASS: set:temperature=30 -> 30.00 C
PASS: set:humidity=60 -> 60.00 %RH
hdc302x-sensor PASS
```

## The steps one at a time

```sh
just hdc302x-part-test      # the commands, the conversion and the inputs, on the host
just hdc302x-part           # -> parts/hdc302x.wasm
just hdc302x-driver-test    # the reset, the trigger and the decode, on the host
just hdc302x-check          # manifest discovered, lowers, `path` dependency resolves
just hdc302x-image          # the generated project compiles -> build/hdc302x/hdc302x.bin
just hdc302x-run            # run with the control channel on 127.0.0.1:5576
```

In a second terminal, while `hdc302x-run` is going:

```sh
just hdc302x-part-input "part list"
just hdc302x-part-input "part climate state"
just hdc302x-part-input "part climate input set:temperature=30"
just hdc302x-part-input "part climate input set:humidity=60"
just hdc302x-part-input "part climate input rest"
```

After editing `drivers/hdc302x.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just hdc302x-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x44 (0x45 to 0x47 with the address pins strapped
differently). It has **no register pointer**: every command is a two-byte write,
and a read answers whatever the last command prepared.

| Command | Meaning |
|---|---|
| `0x2400` | trigger one on-demand measurement, low-power mode, no CRC |
| `0x30A2` | soft reset |

`new()` sends the soft reset. `read()` sends `0x2400` and then reads four
bytes: temperature and humidity, each 16-bit big-endian. The datasheet's
conversions turn them into physical units:

```text
T   = -45 + 175 * raw / 65535   °C
RH  =        100 * raw / 65535  %RH
```

The part holds 22.5 °C and 45 %RH, which encode as 25278 (`0x62BE`) and 29491
(`0x7333`), until a test changes them. It keeps what the sensor sees across a
soft reset, and has no answer at all until a measurement is triggered: an
untouched part answers `0xFF 0xFF 0xFF 0xFF`, the floating bus.

The attach line is in `build/hdc302x/emu.log` (the firmware console is
`build/hdc302x/sensor.stdout`):

```text
co-simulation: part climate (hdc302x) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x44 on GPIO 6/5
```

## Limits

- Happy path only: no CRC variants of the commands (the part does not compute
  a CRC), no heater, no auto-measurement mode, no alert thresholds, no
  low-power/accuracy mode selection, no `init="deferred"`, no negative boards.
- No timing: a real part needs 8.5 ms for a measurement and 200 µs after a
  reset. The driver waits for neither and the emulator completes both at once,
  so the fixture's reading is ready immediately.
- No identification: the HDC302x has no plain id register, and the
  manufacturer/device id commands answer with a CRC byte, so `new()` only
  resets the part and `Hdc302xError` covers bus failures alone.
- The part models the four measurement bytes only: the raw temperature of the
  on-demand-with-CRC command, the heater and the alert outputs are absent.
- The conversion is quantised by the 16-bit counts, about 0.0027 °C and
  0.0015 %RH, so printed values are the datasheet's numbers rounded to two
  decimals.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just hdc302x-clean
```
