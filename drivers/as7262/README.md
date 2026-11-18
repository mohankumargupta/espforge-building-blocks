# AS7262 on I²C with a project-local driver

An espforge project whose `<as7262>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/as7262.kdl`, a project-local
manifest, and the Rust half is the `as7262-driver` crate beside it. It runs
against the emulator's virtual AS7262 I²C part, so the same generated firmware
talks to `parts/as7262.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `as7262` after `am2320`, which is already
here. Only the main happy path is implemented: configure the part, take one
one-shot measurement a second, log the six raw channels.

## What is here

| File | What it is |
|---|---|
| `as7262-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<as7262 id="spectrum" bus="$i2c0" address="0x49" />` |
| `src/app.rs` | reads the six channels once a second and logs `TEST:` lines |
| `drivers/as7262.kdl` | **the external driver**: `dependency "as7262-driver" { path "../as7262-driver" }`, no `codegen.feature` |
| `as7262-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/as7262/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/as7262.wasm` | the built virtual part, from `just as7262-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/as7262/drivers/as7262.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just as7262-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the default spectrum (1200 2400 3200 3100 2800 2200)
PASS: set:red=999 -> the red channel moves
PASS: dark -> all six channels read 0
as7262-sensor PASS
```

## The steps one at a time

```sh
just as7262-part-test     # the virtual registers, the latching and inputs, on the host
just as7262-part          # -> parts/as7262.wasm
just as7262-driver-test   # the reset, the reads and the decode, on the host
just as7262-check         # manifest discovered, lowers, `path` dependency resolves
just as7262-image         # the generated project compiles -> build/as7262/as7262.bin
just as7262-run           # run with the control channel on 127.0.0.1:5571
```

In a second terminal, while `as7262-run` is going:

```sh
just as7262-part-input "part list"
just as7262-part-input "part spectrum state"
just as7262-part-input "part spectrum input set:red=999"
just as7262-part-input "part spectrum input dark"
just as7262-part-input "part spectrum input rest"
```

After editing `drivers/as7262.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just as7262-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x49. Only three physical registers exist:

| Address | Register | Meaning |
|---|---|---|
| `0x00` | STATUS | bit 1 `TX_VALID` (1 = the WRITE register is occupied), bit 0 `RX_VALID` (1 = a byte waits in READ) |
| `0x01` | WRITE | a virtual address, then the data byte |
| `0x02` | READ | the byte a virtual read prepared |

Every other register is virtual, and the top bit of its address says which
direction the next write is:

| Bytes | Meaning |
|---|---|
| `0x01, addr \| 0x80` then `0x01, value` | write `value` to virtual `addr` |
| `0x01, addr` then read STATUS, then read `0x02` | read virtual `addr` |

`new()` soft-resets the part (virtual `0x04` = `0x80`) and sets the
integration time (virtual `0x05` = 50, i.e. 140 ms). `read()` writes virtual
`0x04` with 16x gain and BANK mode `0b11` — one-shot, both photodiode banks —
checks `DATA_RDY` (bit 1) there, then reads the twelve raw bytes:

| Virtual | Channel |
|---|---|
| `0x08:0x09` | V, violet |
| `0x0A:0x0B` | B, blue |
| `0x0C:0x0D` | G, green |
| `0x0E:0x0F` | Y, yellow |
| `0x10:0x11` | O, orange |
| `0x12:0x13` | R, red |

Each is a big-endian 16-bit count. Reading any data register clears
`DATA_RDY`, which is why every `read()` triggers a fresh one-shot conversion.
The calibrated floating-point registers at `0x14..` are not read.

The attach line is in `build/as7262/emu.log` (the firmware console is
`build/as7262/sensor.stdout`):

```text
co-simulation: part spectrum (as7262) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x49 on GPIO 6/5
```

## Limits

- Happy path only: no negative boards, no `init="deferred"`, no interrupt pin,
  no LED driver, no calibrated float registers, no version check.
- No timing: a real part needs the reset delay and two integration times per
  one-shot conversion. The driver reports `Busy` instead of waiting, and the
  emulator completes at once.
- `TX_VALID` is never set by the part, so the driver's busy path is only
  covered by its host tests.
- Continuous BANK modes 0 and 1 are stored but not modelled: the part only
  latches on a one-shot write (modes 2 and 3), and reports `DATA_RDY` for
  those.
- The LED control register is stored and read back but drives nothing, and the
  version registers are constants (`0x0001`, `0x0021`) rather than a real
  device's.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just as7262-clean
```
