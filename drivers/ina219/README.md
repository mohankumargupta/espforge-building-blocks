# INA219 on I²C with a project-local driver

An espforge project whose `<ina219>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/ina219.kdl`, a project-local
manifest, and the Rust half is the `ina219-driver` crate beside it. It runs
against the emulator's virtual INA219 I²C part, so the same generated firmware
talks to `parts/ina219.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `ina219` after `hdc302x`, which is already
here. Only the main happy path is implemented: configure the part, load the
calibration for a 0.1 Ω shunt, and read bus voltage, shunt voltage, current and
power once a second.

## What is here

| File | What it is |
|---|---|
| `ina219-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<ina219 id="power" bus="$i2c0" address="0x40" />` |
| `src/app.rs` | takes one reading a second and logs `TEST:` lines |
| `drivers/ina219.kdl` | **the external driver**: `dependency "ina219-driver" { path "../ina219-driver" }`, no `codegen.feature` |
| `ina219-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/ina219/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/ina219.wasm` | the built virtual part, from `just ina219-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/ina219/drivers/ina219.kdl` — the nested `drivers/` is the tier, not a
typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just ina219-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the default load (5.00 V, 100.00 mA)
PASS: set:voltage=12 -> 12.00 V
PASS: set:current=1 -> 1000.00 mA
ina219-sensor PASS
```

## The steps one at a time

```sh
just ina219-part-test       # the register file, the scale factors and the inputs, on the host
just ina219-part            # -> parts/ina219.wasm
just ina219-driver-test     # the configuration, the calibration and the decode, on the host
just ina219-check           # manifest discovered, lowers, `path` dependency resolves
just ina219-image           # the generated project compiles -> build/ina219/ina219.bin
just ina219-run             # run with the control channel on 127.0.0.1:5577
```

In a second terminal, while `ina219-run` is going:

```sh
just ina219-part-input "part list"
just ina219-part-input "part power state"
just ina219-part-input "part power input set:voltage=12"
just ina219-part-input "part power input set:current=1"
just ina219-part-input "part power input rest"
```

After editing `drivers/ina219.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just ina219-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x40 (0x41 to 0x4F with the address pins strapped
differently). Every register is 16-bit big-endian after its address byte:

| Register | Meaning |
|---|---|
| `0x00` | `configuration`, powers up at `0x399F` |
| `0x01` | `shunt_voltage`, 10 µV per signed count |
| `0x02` | `bus_voltage`, 4 mV per count in bits 15:3 |
| `0x03` | `power`, 2 mW per count |
| `0x04` | `current`, 100 µA per signed count |
| `0x05` | `calibration`, powers up at 0 |

`new()` writes `0x399F` to the configuration (16 V range, ±320 mV gain, 12-bit
32-sample averaging, shunt and bus continuous) and 4096 to the calibration,
which is `0.04096 / (100 µA * 0.1 Ω)` — a 0.1 Ω shunt at a 100 µA current LSB,
the common breakout's arrangement. `read()` then reads the four data registers
from 0x01 in one transaction and converts:

```text
bus voltage   = (raw >> 3) * 4 mV
shunt voltage = raw * 10 µV        (signed)
current       = raw * 100 µA       (signed)
power         = raw * 2 mW
```

The part holds 5 V at 100 mA, which encodes as 1000 (`0x03E8`) shunt counts,
`0x2712` bus counts (1250 counts of 4 mV plus the conversion-ready flag),
250 (`0x00FA`) power counts and 1000 (`0x03E8`) current counts, until a test
changes them.

The attach line is in `build/ina219/emu.log` (the firmware console is
`build/ina219/sensor.stdout`):

```text
co-simulation: part power (ina219) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x40 on GPIO 6/5
```

## Limits

- Happy path only: no configuration register variants (32 V range, other gain
  settings, 9/10/11-bit or averaged conversions), no reset bit, no triggered,
  power-down or ADC-off modes, no alert pin, no `init="deferred"`, no negative
  boards.
- The shunt is fixed at 0.1 Ω and the current LSB at 100 µA, so the range is
  ±3.2 A (±320 mV); `set:current` is held to it, and the bus is 0 to 16 V (`set:voltage`).
- No calibration maths in the part: the data registers are encoded with those
  fixed scale factors whatever the calibration register holds. Writing 0 to
  the calibration (the power-on value) does not zero the current register the
  way a real part would.
- No timing: a real part needs 1 ms after reset and the averaging time per
  conversion. The driver waits for neither and the emulator completes both at
  once.
- No identification: the INA219 has no id register, so `new()` only writes the
  two registers and `Ina219Error` covers bus failures alone.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just ina219-clean
```
