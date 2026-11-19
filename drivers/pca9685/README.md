# PCA9685 on I²C with a project-local driver

An espforge project whose `<pca9685>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/pca9685.kdl`, a project-local
manifest, and the Rust half is the `pca9685-driver` crate beside it. It runs
against the emulator's virtual PCA9685 I²C part, so the same generated firmware
talks to `parts/pca9685.wasm` with no controller crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `pca9685` after `sht4x`; `sht4x` is
already a bundled `espforge-drivers` manifest, so this is the next entry that
was not implemented. Only the main happy path is implemented: initialise the
part for 50 Hz and hold two channels at fixed duty cycles.

## What is here

| File | What it is |
|---|---|
| `pca9685-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<pca9685 id="pwm" bus="$i2c0" address="0x40" />` |
| `src/app.rs` | sets channel 0 to 25 % and channel 1 to 50 %, once a second |
| `drivers/pca9685.kdl` | **the external driver**: `dependency "pca9685-driver" { path "../pca9685-driver" }`, no `codegen.feature` |
| `pca9685-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/pca9685/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/pca9685.wasm` | the built virtual part, from `just pca9685-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/pca9685/drivers/pca9685.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just pca9685-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: ch0 at 25 % and ch1 at 50 %
PASS: 50 Hz prescale
PASS: ch0 reads back at 25 %
pca9685-sensor PASS
```

## The steps one at a time

```sh
just pca9685-part-test      # the register file, the ins and the state, on the host
just pca9685-part           # -> parts/pca9685.wasm
just pca9685-driver-test    # the init sequence and the duty writes, on the host
just pca9685-check          # manifest discovered, lowers, `path` dependency resolves
just pca9685-image          # the generated project compiles -> build/pca9685/pca9685.bin
just pca9685-run            # run with the control channel on 127.0.0.1:5585
```

In a second terminal, while `pca9685-run` is going:

```sh
just pca9685-part-input "part list"
just pca9685-part-input "part pwm state"
just pca9685-part-input "part pwm input rest"
```

After editing `drivers/pca9685.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just pca9685-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x40 with every address pin low; 0x40 to 0x7F are the
straps. It is register-based, so one transaction is a register index followed
by the bytes written to it, walked by the auto-increment bit:

| Register | Meaning |
|---|---|
| `0x00` | MODE1: sleep (`0x10`), auto-increment (`0x20`) |
| `0x01` | MODE2 |
| `0x06` | channel 0's on low, then on high, off low, off high; four bytes per channel |
| `0xFE` | prescaler |

`new()` sleeps the part (`MODE1 ← 0x10`), writes the prescaler for 50 Hz
(`0x79`, from `round(25 MHz / (4096 * 50)) - 1`), then wakes it with
auto-increment (`MODE1 ← 0x20`).

`set_duty(channel, percent)` writes the channel's four duty registers in one
transaction: on = 0, off = `round(percent / 100 * 4096)`, low byte first. 100 %
uses the off high byte's bit 4 instead, because 4096 counts do not fit the
12-bit field:

| Channel | 25 % |
|---|---|
| 0 | `06 00 00 00 04` |

The attach line is in `build/pca9685/emu.log` (the firmware console is
`build/pca9685/sensor.stdout`):

```text
co-simulation: part pwm (pca9685) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x40 on GPIO 6/5
```

## Limits

- Happy path only: a fixed 50 Hz frame rate, so no `set_frequency`, and no
  inverted output, totem-pole/open-drain choice, external clock, ALLCALL or
  SUBADR addressing, restart or sleep control after construction.
- No read-back through the driver: the part's register file answers reads, but
  the driver only writes what it sets.
- The part models the register file and nothing else: no output waveform, no
  LED drive characteristics, no oscillator tolerance.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just pca9685-clean
```
