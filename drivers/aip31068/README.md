# AiP31068 on I²C with a project-local driver

An espforge project whose `<aip31068>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/aip31068.kdl`, a project-local
manifest, and the Rust half is the `aip31068-driver` crate beside it. It runs
against the emulator's virtual AiP31068 I²C part, so the same generated
firmware talks to `parts/aip31068.wasm` with no LCD crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `aip31068` after `aht20`, which is already
here. Only the main happy path is implemented: initialize the panel and write
one string on each of its two lines.

## What is here

| File | What it is |
|---|---|
| `aip31068-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<aip31068 id="lcd" bus="$i2c0" address="0x3E" />` |
| `src/app.rs` | writes `aip31068-lcd` and `hello-i2c` once a second and logs `TEST:` lines |
| `drivers/aip31068.kdl` | **the external driver**: `dependency "aip31068-driver" { path "../aip31068-driver" }`, no `codegen.feature` |
| `aip31068-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/aip31068/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/aip31068.wasm` | the built virtual part, from `just aip31068-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/aip31068/drivers/aip31068.kdl` — the nested `drivers/` is the tier,
not a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just aip31068-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the firmware wrote both lines
PASS: the panel shows aip31068-lcd and hello-i2c
aip31068-sensor PASS
```

## The steps one at a time

```sh
just aip31068-part-test     # the instruction set, DDRAM and the state, on the host
just aip31068-part          # -> parts/aip31068.wasm
just aip31068-driver-test   # the initialization and the two rows, on the host
just aip31068-check         # manifest discovered, lowers, `path` dependency resolves
just aip31068-image         # the generated project compiles -> build/aip31068/aip31068.bin
just aip31068-run           # run with the control channel on 127.0.0.1:5569
```

In a second terminal, while `aip31068-run` is going:

```sh
just aip31068-part-input "part list"
just aip31068-part-input "part lcd state"
```

`part lcd state` answers `ok line1=aip31068-lcd line2=hello-i2c`: the panel is
write-only, so its DDRAM is the only way to check what the firmware sent.
Spaces in a value are escaped as `\ ` by the control channel, which is why the
fixture's two lines have none.

After editing `drivers/aip31068.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just aip31068-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The AiP31068's I²C interface takes bytes whole, so every transaction is a
control byte and what it applies to:

| First byte | Meaning |
|---|---|
| `0x00` | the bytes after it are instructions (`RS` clear) |
| `0x40` | the bytes after it are display data (`RS` set) |

`new()` sends the HD44780 power-on sequence for an 8-bit, 2-line panel, one
instruction per write:

| Instruction | Meaning |
|---|---|
| `0x38` | function set: 8-bit interface, 2 lines, 5×8 font |
| `0x0C` | display control: display on, cursor off, blink off |
| `0x01` | clear display, home the cursor |
| `0x06` | entry mode: cursor increments, display does not shift |

`write_line(row, text)` then writes `0x80 | row_start` — `0x00` for the top
line, `0x40` for the bottom — followed by `0x40` and the string's bytes, in
chunks of 16. The panel's cursor advances one DDRAM cell per byte, as the entry
mode says. Row 0 is DDRAM `0x00..=0x0F` and row 1 is `0x40..=0x4F`; addresses
past those windows exist in the 128-cell DDRAM but are not shown, so a line
longer than 16 characters runs off the row rather than wrapping.

The attach line is in `build/aip31068/emu.log` (the firmware console is
`build/aip31068/sensor.stdout`):

```text
co-simulation: part lcd (aip31068) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x3e on GPIO 6/5
```

## Limits

- Happy path only: no negative boards, no `init="deferred"`, no custom
  characters, no backlight (a Grove-style module's PCA9633 at 0x62 is a second
  chip and is not modelled).
- No timing: a real part needs 1.53 ms after a clear and after returning home.
  The driver does not wait, and the emulator completes at once.
- Write-only: the part ignores the control bytes other than `0x00` and
  `0x40` (read requests, continuation), and the driver never reads.
- No cursor or display shift, and no CGRAM: those instructions are decoded but
  change nothing visible. The display-control bits `C` and `B` are stored, not
  shown.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just aip31068-clean
```
