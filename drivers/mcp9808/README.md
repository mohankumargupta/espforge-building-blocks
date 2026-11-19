# MCP9808 on I²C with a project-local driver

An espforge project whose `<mcp9808>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/mcp9808.kdl`, a project-local
manifest, and the Rust half is the `mcp9808-driver` crate beside it. It runs
against the emulator's virtual MCP9808 I²C part, so the same generated firmware
talks to `parts/mcp9808.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `mcp9808` after `ina219`, which is already
here. Only the main happy path is implemented: identify the part, set its
resolution, and read the ambient temperature once a second.

## What is here

| File | What it is |
|---|---|
| `mcp9808-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<mcp9808 id="temperature" bus="$i2c0" address="0x18" />` |
| `src/app.rs` | takes one reading a second and logs `TEST:` lines |
| `drivers/mcp9808.kdl` | **the external driver**: `dependency "mcp9808-driver" { path "../mcp9808-driver" }`, no `codegen.feature` |
| `mcp9808-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/mcp9808/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/mcp9808.wasm` | the built virtual part, from `just mcp9808-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/mcp9808/drivers/mcp9808.kdl` — the nested `drivers/` is the tier, not a
typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just mcp9808-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the default 24.00 °C
PASS: set:temperature=30 -> 30.00 °C
PASS: set:temperature=-5.5 -> -5.50 °C
mcp9808-sensor PASS
```

## The steps one at a time

```sh
just mcp9808-part-test       # the register file, the encoding and the inputs, on the host
just mcp9808-part            # -> parts/mcp9808.wasm
just mcp9808-driver-test     # the identification, the resolution and the decode, on the host
just mcp9808-check           # manifest discovered, lowers, `path` dependency resolves
just mcp9808-image           # the generated project compiles -> build/mcp9808/mcp9808.bin
just mcp9808-run             # run with the control channel on 127.0.0.1:5578
```

In a second terminal, while `mcp9808-run` is going:

```sh
just mcp9808-part-input "part list"
just mcp9808-part-input "part temperature state"
just mcp9808-part-input "part temperature input set:temperature=30"
just mcp9808-part-input "part temperature input set:temperature=-5.5"
just mcp9808-part-input "part temperature input rest"
```

After editing `drivers/mcp9808.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just mcp9808-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x18 (0x19 to 0x1F with the address pins strapped
differently). Every register is 16-bit big-endian after its address byte:

| Register | Meaning |
|---|---|
| `0x00` | `configuration`, powers up at `0x0000` |
| `0x05` | `ambient_temperature`, 0.0625 °C per signed count in bits 12:0 |
| `0x06` | `manufacturer_id`, `0x0054` |
| `0x07` | `device_id`, `0x0400` |
| `0x08` | `resolution`, powers up at `0x0003` (0.0625 °C per count) |

`new()` reads the manufacturer and device id and rejects anything else with
`Mcp9808Error::UnknownDevice`, then writes `0x0003` to the resolution, which is
the part's power-on value anyway. `read()` reads the ambient register and
decodes it:

```text
counts  = raw & 0x1FFF, minus 8192 when bit 12 is set   (two's complement)
degrees = counts * 0.0625
```

Bits 15:13 are the alert flags (critical, upper, lower) and are masked off. The
part holds 24.00 °C, which encodes as `0x0180` (384 counts), until a test
changes it.

The attach line is in `build/mcp9808/emu.log` (the firmware console is
`build/mcp9808/sensor.stdout`):

```text
co-simulation: part temperature (mcp9808) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x18 on GPIO 6/5
```

## Limits

- Happy path only: no alert limits (upper, lower, critical), no alert output or
  hysteresis configuration, no shutdown, power-down or one-shot modes, no
  critical-lock, no `init="deferred"`, no negative boards.
- The resolution is fixed at 0.0625 °C (register `0x08` reads `0x0003`); the
  part always encodes that many counts, whatever is written to it.
- No conversion timing: a real part needs its conversion time after a
  measurement, and the driver waits for none of it — the emulator completes at
  once.
- The alert flags in the ambient register are always clear in the part and
  masked off by the driver, so no reading ever trips an alert.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just mcp9808-clean
```
