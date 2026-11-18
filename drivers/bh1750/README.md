# BH1750 on I²C with a project-local driver

An espforge project whose `<bh1750>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/bh1750.kdl`, a project-local
manifest, and the Rust half is the `bh1750-driver` crate beside it. It runs
against the emulator's virtual BH1750 I²C part, so the same generated firmware
talks to `parts/bh1750.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `bh1750` after `as7262`, which is already
here. Only the main happy path is implemented: power the part on, take one
one-shot high-resolution measurement a second, log the lux.

## What is here

| File | What it is |
|---|---|
| `bh1750-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<bh1750 id="light" bus="$i2c0" address="0x23" />` |
| `src/app.rs` | reads the lux once a second and logs `TEST:` lines |
| `drivers/bh1750.kdl` | **the external driver**: `dependency "bh1750-driver" { path "../bh1750-driver" }`, no `codegen.feature` |
| `bh1750-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/bh1750/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/bh1750.wasm` | the built virtual part, from `just bh1750-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/bh1750/drivers/bh1750.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just bh1750-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the default light (300.0 lux)
PASS: set:lux=1000 -> the reading moves
PASS: dark -> 0.0 lux
bh1750-sensor PASS
```

## The steps one at a time

```sh
just bh1750-part-test      # the commands, the modes and the inputs, on the host
just bh1750-part           # -> parts/bh1750.wasm
just bh1750-driver-test    # the power-on, the read and the lux conversion, on the host
just bh1750-check          # manifest discovered, lowers, `path` dependency resolves
just bh1750-image          # the generated project compiles -> build/bh1750/bh1750.bin
just bh1750-run            # run with the control channel on 127.0.0.1:5572
```

In a second terminal, while `bh1750-run` is going:

```sh
just bh1750-part-input "part list"
just bh1750-part-input "part light state"
just bh1750-part-input "part light input set:lux=1000"
just bh1750-part-input "part light input dark"
just bh1750-part-input "part light input rest"
```

After editing `drivers/bh1750.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just bh1750-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x23 (its ADDR pin low; the other strapping is 0x5C, which
this fixture does not use). It takes **one-byte commands** and answers a
measurement with **two big-endian bytes** of raw counts:

| Command | Meaning |
|---|---|
| `0x00` | power down |
| `0x01` | power on |
| `0x07` | reset: back to power down, measurement cleared |
| `0x10` | continuous, high resolution (1 lx) |
| `0x11` | continuous, high resolution 2 (0.5 lx) |
| `0x13` | continuous, low resolution (4 lx) |
| `0x20` | one-shot, high resolution (1 lx) |
| `0x21` | one-shot, high resolution 2 (0.5 lx) |
| `0x23` | one-shot, low resolution (4 lx) |

`new()` sends `0x01`; `read()` sends `0x20`, then reads the two data bytes and
divides the count by 1.2 — the datasheet's counts per lux in high-resolution
mode — so 360 counts is 300.0 lx. It logs `lux` with one decimal, which is why
`1200 / 1.2` prints as `1000.0` even though the float is just below it.

The attach line is in `build/bh1750/emu.log` (the firmware console is
`build/bh1750/sensor.stdout`):

```text
co-simulation: part light (bh1750) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x23 on GPIO 6/5
```

## Limits

- Happy path only: no negative boards, no `init="deferred"`, no ADDR-high
  address, no continuous-mode polling, no MTreg (measurement-time) adjustment.
- No timing: a real part's one-shot takes up to 180 ms. The driver does not
  wait, and the emulator latches the measurement the moment the command
  arrives.
- The part encodes the light at the mode's counts per lux when a measurement
  command arrives, so the low-resolution mode (0.15 counts per lux) and high
  resolution 2 (2.4) are readable but the driver only uses high resolution.
- Continuous modes latch once and stay, since there is no time to keep
  measuring; power down only changes the power state, not the latched counts.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just bh1750-clean
```
