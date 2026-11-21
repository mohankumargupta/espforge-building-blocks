# HT16K33 on I²C with a project-local driver

An espforge project whose `<ht16k33>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/ht16k33.kdl`, a project-local
manifest, and the Rust half is the `ht16k33-driver` crate beside it. It runs
against the emulator's virtual HT16K33 I²C part, so the same generated firmware
talks to `parts/ht16k33.wasm` with no display crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `ht16k33` is in `i2c.tsv` and had no fixture here or in
`espforge-drivers`. Only the main happy path is implemented: start the
oscillator and the display, load a frame into the display RAM a second, read it
back, and set the dimming level.

## What is here

| File | What it is |
|---|---|
| `ht16k33-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<ht16k33 id="display" bus="$i2c0" address="0x70" />` |
| `src/app.rs` | writes a diagonal frame a second, reads it back and logs `TEST:` lines |
| `drivers/ht16k33.kdl` | **the external driver**: `dependency "ht16k33-driver" { path "../ht16k33-driver" }`, no `codegen.feature` |
| `ht16k33-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/ht16k33/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/ht16k33.wasm` | the built virtual part, from `just ht16k33-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/ht16k33/drivers/ht16k33.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just ht16k33-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the frame reads back from the part's RAM
ht16k33-sensor PASS
```

## The steps one at a time

```sh
just ht16k33-part-test      # the commands, the RAM wrapping and the inputs, on the host
just ht16k33-part           # -> parts/ht16k33.wasm
just ht16k33-driver-test    # the setup commands, the frame write and the read-back, on the host
just ht16k33-check          # manifest discovered, lowers, `path` dependency resolves
just ht16k33-image          # the generated project compiles -> build/ht16k33/ht16k33.bin
just ht16k33-run            # run with the control channel on 127.0.0.1:5589
```

In a second terminal, while `ht16k33-run` is going:

```sh
just ht16k33-part-input "part list"
just ht16k33-part-input "part display state"
just ht16k33-part-input "part display input set:brightness=9"
just ht16k33-part-input "part display input off"
just ht16k33-part-input "part display input rest"
```

After editing `drivers/ht16k33.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just ht16k33-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x70 (0x71 to 0x77 with the three address jumpers
strapped differently). It is **command-addressed**: the first byte of a write
is the command and the bytes after it are that command's parameters.

| Command | Meaning |
|---|---|
| `0x00..=0x0F` | display RAM address; the bytes after it fill RAM from there, auto-incrementing and wrapping at 16 |
| `0x20` / `0x21` | system setup: oscillator off / on |
| `0x80..=0x8F` | display setup: bit 0 display on, bits 2:1 blink rate |
| `0xE0..=0xEF` | dimming: the low nibble is the level, 0…15 |

`new()` sends `0x21` (oscillator on) and then `0x81` (display on, no blink).
`write_ram` sends `0x00` and the sixteen frame bytes in one transaction, so the
address auto-increments through the whole RAM. `read_ram` sends `0x00` and then
reads the sixteen bytes back; a read started at a later address wraps at the
end, as the part's pointer does.

The RAM is 16 bytes, two per row: row 0 is bytes 0–1, row 1 bytes 2–3, and so
on to row 7. The fixture's frame is a diagonal line:

```text
01 02 04 08 10 20 40 80 01 02 04 08 10 20 40 80
```

The part starts with the oscillator and display off, the RAM zeroed, the blink
rate 0 and the dimming level 0, and it keeps its RAM until a test changes it.
A keyscan command (`0x40..=0x47`) is not modelled and is NACKed, as is any
other command the part does not have.

The attach line is in `build/ht16k33/emu.log` (the firmware console is
`build/ht16k33/sensor.stdout`):

```text
co-simulation: part display (ht16k33) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x70 on GPIO 6/5
```

## Limits

- Happy path only: no blinking in the app, no keyscan (the part has no key
  matrix), no interrupt output, no 1/16-duty or external-clock variants, no
  `init="deferred"`, no negative boards.
- The app writes the same diagonal frame every second: there is no animation
  state, and the part does not model LED persistence or refresh timing.
- `set_brightness` is in the driver and tested on the host, but the app leaves
  the level at its power-on value; the fixture's recipes do not assert it.
- The part models the command byte, the RAM and the four registers it needs. It
  does not model the multiplexing, the row drivers or the keyscan RAM, and a
  command it does not know is NACKed rather than accepted and ignored.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just ht16k33-clean
```
