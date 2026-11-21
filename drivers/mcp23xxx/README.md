# MCP23xxx on I²C with a project-local driver

An espforge project whose `<mcp23xxx>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/mcp23xxx.kdl`, a project-local
manifest, and the Rust half is the `mcp23xxx-driver` crate beside it. It runs
against the emulator's virtual MCP23017 I²C part, so the same generated
firmware talks to `parts/mcp23xxx.wasm` with no expander crate glued in by
hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `mcp23xxx` is in `i2c.tsv` and had no fixture here or in
`espforge-drivers`. Only the main happy path is implemented: make both ports
outputs at construction, set one port to inputs with pull-ups, drive the other,
and read both back.

The tag covers the whole family (`MCP23008`, `MCP23S08`, `MCP23017`,
`MCP23S17`, …). This fixture implements the **MCP23017**: the 16-bit I²C part,
in its power-on `IOCON.BANK = 0` register layout.

## What is here

| File | What it is |
|---|---|
| `mcp23xxx-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<mcp23xxx id="io" bus="$i2c0" address="0x20" />` |
| `src/app.rs` | drives port A, reads both ports a second and logs `TEST:` lines |
| `drivers/mcp23xxx.kdl` | **the external driver**: `dependency "mcp23xxx-driver" { path "../mcp23xxx-driver" }`, no `codegen.feature` |
| `mcp23xxx-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/mcp23xxx/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/mcp23xxx.wasm` | the built virtual part, from `just mcp23xxx-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/mcp23xxx/drivers/mcp23xxx.kdl` — the nested `drivers/` is the tier,
not a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just mcp23xxx-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: port A drives its outputs and port B reads its pins
PASS: set:levels=0x5A00 -> port B reads 0x5a
mcp23xxx-sensor PASS
```

## The steps one at a time

```sh
just mcp23xxx-part-test      # the register file, the pins and the inputs, on the host
just mcp23xxx-part           # -> parts/mcp23xxx.wasm
just mcp23xxx-driver-test    # the constructor, the per-port writes and the read, on the host
just mcp23xxx-check          # manifest discovered, lowers, `path` dependency resolves
just mcp23xxx-image          # the generated project compiles -> build/mcp23xxx/mcp23xxx.bin
just mcp23xxx-run            # run with the control channel on 127.0.0.1:5590
```

In a second terminal, while `mcp23xxx-run` is going:

```sh
just mcp23xxx-part-input "part list"
just mcp23xxx-part-input "part io state"
just mcp23xxx-part-input "part io input set:levels=0x5A00"
just mcp23xxx-part-input "part io input rest"
```

After editing `drivers/mcp23xxx.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just mcp23xxx-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x20 (0x21 to 0x27 with the address pins strapped
differently). One transaction is a **register index followed by the bytes for
it**, and the pointer auto-increments between them, so a multi-byte write walks
the file. The power-on `IOCON.BANK = 0` layout:

| Registers | Meaning |
|---|---|
| `0x00` / `0x01` | IODIRA / IODIRB: a set bit is an input |
| `0x02` / `0x03` | IPOLA / IPOLB: input polarity inversion |
| `0x0A` / `0x0B` | IOCON (both addresses are the same byte in bank 0) |
| `0x0C` / `0x0D` | GPPUA / GPPUB: the 100 kΩ input pull-ups |
| `0x0E` / `0x0F` | INTFA / INTFB, read-only |
| `0x10` / `0x11` | INTCAPA / INTCAPB, read-only |
| `0x12` / `0x13` | GPIOA / GPIOB: the pins |
| `0x14` / `0x15` | OLATA / OLATB: the output latches |

`new()` sends `[0x00, 0x00, 0x00]`: IODIRA then IODIRB, in one transaction, so
all sixteen pins become outputs. `write_outputs(port, value)` writes `OLATA`
(`0x14`) or `OLATB` (`0x15`) — the latch, not GPIO, so a read of the port does
not disturb the output pins. `read_inputs(port)` writes `0x12`/`0x13` and reads
the byte back. `set_direction` writes `0x00`/`0x01` and `set_pull_ups` writes
`0x0C`/`0x0D`.

An output pin reads back its latch; an input pin reads the level on the pin,
with the polarity register applied. The part starts with every pin an input,
every latch and every other register low, and 0x00 levels on the pins; a write
to a register the part does not have (0x16 and above) is NACKed.

The app sets port B to inputs with its pull-ups and drives port A with `0xAA`,
so the steady line is:

```text
TEST: mcp23xxx a=aa b=00
```

`set:levels=0x5A00` drives port B's pins with `0x5A` (port A is in the low
byte), so the next line is `TEST: mcp23xxx a=aa b=5a`.

The attach line is in `build/mcp23xxx/emu.log` (the firmware console is
`build/mcp23xxx/sensor.stdout`):

```text
co-simulation: part io (mcp23xxx) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x20 on GPIO 6/5
```

## Limits

- MCP23017 only: no 8-bit MCP23008 register layout (same registers at an
  offset), no MCP23S17 SPI variant, no `IOCON.BANK = 1` mirrored layout.
- Interrupts are not modelled: GPINTEN, DEFVAL and INTCON read back but drive
  nothing, INTF and INTCAP always read zero, and there is no `INTA`/`INTB` pin.
- The pull-ups are recorded in GPPU and read back, but they do not change what a
  pin reads: the part's `levels` are the world's drive, whatever the pull-ups
  say.
- No `init="deferred"`, no negative boards, and the app leaves port A's
  direction alone after construction — port B's is the only one it changes.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just mcp23xxx-clean
```
