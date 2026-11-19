# TCA95xx on I²C with a project-local driver

An espforge project whose `<tca95xx>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/tca95xx.kdl`, a project-local
manifest, and the Rust half is the `tca95xx-driver` crate beside it. It runs
against the emulator's virtual TCA9535 I²C part, so the same generated firmware
talks to `parts/tca95xx.wasm` with no controller crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `tca95xx` after `ssd1306`, which is
already a bundled `espforge-drivers` manifest, so this is the next entry that
was not implemented. Only the main happy path is implemented: make all sixteen
pins outputs and drive one 16-bit pattern.

## What is here

| File | What it is |
|---|---|
| `tca95xx-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<tca95xx id="io" bus="$i2c0" address="0x20" />` |
| `src/app.rs` | drives `0x00A5` on the output latch, once a second |
| `drivers/tca95xx.kdl` | **the external driver**: `dependency "tca95xx-driver" { path "../tca95xx-driver" }`, no `codegen.feature` |
| `tca95xx-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/tca95xx/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/tca95xx.wasm` | the built virtual part, from `just tca95xx-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/tca95xx/drivers/tca95xx.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just tca95xx-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the app drives 0x00A5
PASS: both ports are outputs
PASS: the part reads the latch back
tca95xx-sensor PASS
```

## The steps one at a time

```sh
just tca95xx-part-test      # the register file, the pins and the state, on the host
just tca95xx-part           # -> parts/tca95xx.wasm
just tca95xx-driver-test    # the configuration and the word write, on the host
just tca95xx-check          # manifest discovered, lowers, `path` dependency resolves
just tca95xx-image          # the generated project compiles -> build/tca95xx/tca95xx.bin
just tca95xx-run            # run with the control channel on 127.0.0.1:5586
```

In a second terminal, while `tca95xx-run` is going:

```sh
just tca95xx-part-input "part list"
just tca95xx-part-input "part io state"
just tca95xx-part-input "part io input set:levels=0x00F0"
just tca95xx-part-input "part io input rest"
```

After editing `drivers/tca95xx.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just tca95xx-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x20 with every address pin low; 0x20 to 0x27 are the
straps. One transaction is a register index followed by the bytes written to
it, and the pointer auto-increments, so a multi-byte write walks the registers:

| Register | Meaning |
|---|---|
| `0x00`, `0x01` | input ports (read-only: the pins answer) |
| `0x02`, `0x03` | output latches |
| `0x04`, `0x05` | polarity inversion |
| `0x06`, `0x07` | configuration: a set bit is an input |

`new()` clears both configuration registers (`06 00 00`), which makes all
sixteen pins outputs. `write_outputs(word)` then writes the latch low port
first, in one transaction:

| Call | Wire |
|---|---|
| `write_outputs(0x00A5)` | `02 A5 00` |
| `write_outputs(0x1234)` | `02 34 12` |

The attach line is in `build/tca95xx/emu.log` (the firmware console is
`build/tca95xx/sensor.stdout`):

```text
co-simulation: part io (tca95xx) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x20 on GPIO 6/5
```

## Limits

- Happy path only: every pin becomes an output at construction, so there is no
  per-pin direction change, no polarity inversion, no input read through the
  driver and no interrupt line.
- The driver writes the whole 16-bit latch; there is no read-modify-write and
  no per-pin helper.
- The part models the register file and the pin levels, nothing else: no drive
  strength, no open-drain outputs, no interrupts.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just tca95xx-clean
```
