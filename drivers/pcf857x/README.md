# PCF857x on I²C with a project-local driver

An espforge project whose `<pcf857x>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/pcf857x.kdl`, a project-local
manifest, and the Rust half is the `pcf857x-driver` crate beside it. It runs
against the emulator's virtual PCF8574 I²C part, so the same generated firmware
talks to `parts/pcf857x.wasm` with no expander crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `pcf857x` is in `i2c.tsv` and had no fixture here or in
`espforge-drivers`. Only the main happy path is implemented: drive a pattern,
read the port back, and let the world pull released pins low.

The tag covers the 8-bit `PCF8574`/`PCF8574A` and the 16-bit `PCF8575`. This
fixture implements the **PCF8574**: eight pins, I²C address 0x20–0x27.

## What is here

| File | What it is |
|---|---|
| `pcf857x-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<pcf857x id="io" bus="$i2c0" address="0x20" />` |
| `src/app.rs` | drives the pins, reads them a second and logs `TEST:` lines |
| `drivers/pcf857x.kdl` | **the external driver**: `dependency "pcf857x-driver" { path "../pcf857x-driver" }`, no `codegen.feature`, `codegen { fallible #false }` |
| `pcf857x-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/pcf857x/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/pcf857x.wasm` | the built virtual part, from `just pcf857x-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/pcf857x/drivers/pcf857x.kdl` — the nested `drivers/` is the tier,
not a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just pcf857x-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the eight pins read back what was written
PASS: set:levels=0x0F -> the pins read 0x0a
pcf857x-sensor PASS
```

## The steps one at a time

```sh
just pcf857x-part-test      # the port, the pins and the world's drive, on the host
just pcf857x-part           # -> parts/pcf857x.wasm
just pcf857x-driver-test    # the port byte and the read, on the host
just pcf857x-check          # manifest discovered, lowers, `path` dependency resolves
just pcf857x-image          # the generated project compiles -> build/pcf857x/pcf857x.bin
just pcf857x-run            # run with the control channel on 127.0.0.1:5591
```

In a second terminal, while `pcf857x-run` is going:

```sh
just pcf857x-part-input "part list"
just pcf857x-part-input "part io state"
just pcf857x-part-input "part io input set:levels=0x0F"
just pcf857x-part-input "part io input rest"
```

After editing `drivers/pcf857x.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just pcf857x-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

There is no register file. One transaction is **one byte**:

- a write byte is the port's output latch, so a multi-byte write leaves its
  last byte on the pins,
- a read byte is the level on the eight pins, and a read of several bytes
  repeats the same pins.

The part answers at 0x20 (0x21 to 0x27 with the address pins strapped
differently). Power-on is `0xFF`: every pin released to its weak pull-up, so
every pin reads high. `Pcf857x::new` therefore sends **no transaction** — there
is nothing to configure — and the manifest says `codegen { fallible #false }`.

The pins are quasi-bidirectional. A latch bit of 0 drives the pin low; a latch
bit of 1 only releases the pin, and a weak current source pulls it high. So a
read is `latch & levels`, where `levels` is the world's drive on the pins:

| Latch | World | Pin reads |
|---|---|---|
| 1 | not pulled low | 1 |
| 1 | pulled low | 0 |
| 0 | anything | 0 |

With nothing else on the board, `write(byte)` then `read()` gives `byte` back,
which is what makes a read-modify-write of one pin work:

```rust
let pins = io.read()?;
io.write(pins & !(1 << 3))?; // drive pin 3 low, leave the rest alone
```

The app writes `0xAA`, so the steady line is:

```text
TEST: pcf857x pins=aa
```

`set:levels=0x0F` makes the world pull the low nibble low, so the next line is
`TEST: pcf857x pins=0a`.

The attach line is in `build/pcf857x/emu.log` (the firmware console is
`build/pcf857x/sensor.stdout`):

```text
co-simulation: part io (pcf857x) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x20 on GPIO 6/5
```

## Limits

- PCF8574 only: no PCF8575's sixteen pins and two-byte port, no PCF8574A
  address range (0x38–0x3F), no PCF8574 with the same silicon at another
  strap.
- No interrupt output: the part has an `INT` pin that pulses when an input
  changes, and the fixture models neither the pin nor the change event.
- The pull-up and the 25 mA sink are not modelled electrically: a pin reads
  `latch & levels` whatever drives it, so there is no drive-strength or
  bus-loading behaviour.
- No pin-level API in the driver (`set_pin`/`get_pin`); a pin write is a
  read-modify-write the app does, as above.
- No `init="deferred"` in the app, no negative boards, happy path only.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just pcf857x-clean
```
