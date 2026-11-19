# TLV493D-A1B6 on I²C with a project-local driver

An espforge project whose `<tlv493d>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/tlv493d.kdl`, a project-local
manifest, and the Rust half is the `tlv493d-driver` crate beside it. It runs
against the emulator's virtual TLV493D-A1B6 I²C part, so the same generated
firmware talks to `parts/tlv493d.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `tlv493d` after `tca95xx`, which is
already here; `sht4x` and `ssd1306` before them are bundled
`espforge-drivers` manifests, so this is the next entry that was not
implemented. Only the main happy path is implemented: clock out the ten-byte
frame once a second and report the field on the three axes and the
temperature.

## What is here

| File | What it is |
|---|---|
| `tlv493d-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<tlv493d id="field" bus="$i2c0" address="0x5E" />` |
| `src/app.rs` | reads one frame a second and logs `TEST:` lines |
| `drivers/tlv493d.kdl` | **the external driver**: `dependency "tlv493d-driver" { path "../tlv493d-driver" }`, no `codegen.feature` |
| `tlv493d-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/tlv493d/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/tlv493d.wasm` | the built virtual part, from `just tlv493d-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/tlv493d/drivers/tlv493d.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just tlv493d-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: a fresh part reads no field at 25.00 C
PASS: set:bx=0.98 -> 0.98 mT
PASS: set:bz=-0.49 -> -0.49 mT
PASS: set:temperature=26.1 -> 26.10 C
tlv493d-sensor PASS
```

## The steps one at a time

```sh
just tlv493d-part-test      # the frame, the factory bytes and the inputs, on the host
just tlv493d-part           # -> parts/tlv493d.wasm
just tlv493d-driver-test    # the frame layout and the decode, on the host
just tlv493d-check          # manifest discovered, lowers, `path` dependency resolves
just tlv493d-image          # the generated project compiles -> build/tlv493d/tlv493d.bin
just tlv493d-run            # run with the control channel on 127.0.0.1:5587
```

In a second terminal, while `tlv493d-run` is going:

```sh
just tlv493d-part-input "part list"
just tlv493d-part-input "part field state"
just tlv493d-part-input "part field input set:bx=0.98"
just tlv493d-part-input "part field input set:bz=-0.49"
just tlv493d-part-input "part field input set:temperature=26.1"
```

After editing `drivers/tlv493d.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just tlv493d-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x5E, its address with the ADDR strap low (0x1F needs a
reset sequence that holds SDA high, which neither the driver nor the part
models). There is no register to write before a read: the part's address
pointer is at 0x00 after power-up, so one read clocks out the whole frame and
the pointer walks it:

| Register | Meaning |
|---|---|
| `0x00` | Bx, bits 11:4 |
| `0x01` | By, bits 11:4 |
| `0x02` | Bz, bits 11:4 |
| `0x03` | temperature bits 11:8, then the frame counter (3:2) and the channel (1:0) |
| `0x04` | Bx bits 3:0 in the high nibble, By bits 3:0 in the low nibble |
| `0x05` | Bz bits 3:0 in the low nibble; bit 4 is the power-down flag |
| `0x06` | temperature bits 7:0 |
| `0x07`–`0x09` | the part's factory values, `0x18 0xFF 0x1F` |

`new()` reads one frame and drops it, which is the check that the sensor
answers. `read()` then clocks out ten bytes in one transaction and converts
them:

```text
Bx, By, Bz = signed 12-bit count * 0.098 mT
temperature = 25 °C + (raw - 340) * 1.1 °C
```

The part holds no field at 25 °C until a test moves it, which is the power-on
state: Bx raw 0, By raw 0, Bz raw 0, temperature raw 340 (`0x154`). The frame
counter in register 0x03 advances by one per read and the channel stays 0, as
the datasheet's single-measurement frame has it.

A write's first byte sets the register the next read starts at — the part
wraps at the end of the frame — and the MOD1/MOD2 bytes after it are accepted
and ignored. A write to a register the part lacks is NACKed.

The attach line is in `build/tlv493d/emu.log` (the firmware console is
`build/tlv493d/sensor.stdout`):

```text
co-simulation: part field (tlv493d) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x5E on GPIO 6/5
```

## Limits

- Happy path only: the part measures continuously, so the driver writes no
  configuration at all — no MOD1/MOD2, no parity bit, no low-power,
  ultra-low-power, fast or master-controlled mode, no power-down wake-up and
  no interrupt pin.
- No frame validation: the frame counter and the channel are decoded past, not
  checked, and a read is taken as one measurement.
- The address strap is fixed at 0x5E: 0x1F is only reachable with the
  datasheet's reset sequence, which the driver does not bit-bang.
- The part models the frame and the frame counter, nothing else: no ADC
  noise, no offset, no measurement timing, and the field is held to the
  ±130 mT linear range and the temperature to -40…125 °C.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just tlv493d-clean
```
