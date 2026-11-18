# AM2320 on I²C with a project-local driver

An espforge project whose `<am2320>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/am2320.kdl`, a project-local
manifest, and the Rust half is the `am2320-driver` crate beside it. It runs
against the emulator's virtual AM2320 I²C part, so the same generated firmware
talks to `parts/am2320.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `am2320` after `aip31068`, which is
already here. Only the main happy path is implemented: read temperature and
humidity once a second.

## What is here

| File | What it is |
|---|---|
| `am2320-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<am2320 id="climate" bus="$i2c0" address="0x5C" />` |
| `src/app.rs` | reads temperature and humidity once a second and logs `TEST:` lines |
| `drivers/am2320.kdl` | **the external driver**: `dependency "am2320-driver" { path "../am2320-driver" }`, no `codegen.feature` |
| `am2320-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/am2320/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/am2320.wasm` | the built virtual part, from `just am2320-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/am2320/drivers/am2320.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just am2320-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the part reads 22.50 C and 45.00 %RH
PASS: set:temperature=30 -> 30.00 C
PASS: set:humidity=60 -> 60.00 %RH
am2320-sensor PASS
```

## The steps one at a time

```sh
just am2320-part-test     # the registers, the CRC-16 and the inputs, on the host
just am2320-part          # -> parts/am2320.wasm
just am2320-driver-test   # the read command, the decode and the sign, on the host
just am2320-check         # manifest discovered, lowers, `path` dependency resolves
just am2320-image         # the generated project compiles -> build/am2320/am2320.bin
just am2320-run           # run with the control channel on 127.0.0.1:5570
```

In a second terminal, while `am2320-run` is going:

```sh
just am2320-part-input "part list"
just am2320-part-input "part climate state"
just am2320-part-input "part climate input set:temperature=30"
just am2320-part-input "part climate input set:humidity=60"
just am2320-part-input "part climate input rest"
```

After editing `drivers/am2320.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just am2320-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The AM2320 answers a Modbus-style register read. The master writes the
function code, the start register and the count, then reads the answer as a
separate transaction (the part needs a stop before it prepares the frame, so
`write_read` with a repeated start does not work on hardware):

| Write | Meaning |
|---|---|
| `0x03 0x00 0x04` | read the four bytes from register 0x0000: humidity, then temperature |

The answer is eight bytes:

| Byte | Meaning |
|---|---|
| 0 | `0x03`, the function code echoed |
| 1 | `0x04`, the number of register bytes that follow |
| 2–3 | humidity, big-endian, in tenths of a percent |
| 4–5 | temperature, big-endian, in tenths of a degree; bit 15 is the sign |
| 6–7 | CRC-16 (Modbus, polynomial `0xA001`, initial value `0xFFFF`), low byte first |

```text
RH  = register / 10                  %RH
T   = (register & 0x7FFF) / 10       °C, negated when bit 15 is set
```

The part holds 22.5 °C and 45 %RH by default: registers `0x00E1` and `0x01C2`.

`Am2320::new` does not touch the bus — there is no identity register in the
read frame, and every transaction wakes the part — so the first transaction is
the app's first `read()`.

The attach line is in `build/am2320/emu.log` (the firmware console is
`build/am2320/sensor.stdout`):

```text
co-simulation: part climate (am2320) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x5C on GPIO 6/5
```

## Limits

- Happy path only: no negative boards, no `init="deferred"`, no CRC
  verification in the driver (the part computes one the driver ignores).
- No timing: a real part needs 800 µs after the wake-up and 1.5 ms after the
  read command. The driver does not wait, and the emulator completes at once.
- No sleep model: the real part drops into sleep after a few seconds idle and
  needs an address-only write to wake; this part always answers.
- Only the `0x03 0x00 0x04` window is served; the model and version registers
  (`0x02`, `0x08`, `0x0B`) are not.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just am2320-clean
```
