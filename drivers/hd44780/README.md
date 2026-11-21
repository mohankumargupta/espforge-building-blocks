# HD44780 behind a PCF8574T backpack, manifest-only

An espforge project whose `<hd44780>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/hd44780.kdl`, a project-local
manifest, and there is no Rust half at all. The manifest names the published
`i2c-character-display` 0.6.0 crate, whose
`CharacterDisplayPCF8574T::new_with_address(i2c, address, LcdDisplayType::Lcd16x2, delay)`
already fits espforge's bus slot, the element's address, one fixed geometry
value and a `DelayNs`. It is embedded-hal 1.0, like the runtime, and the
constructor cannot fail (`fallible #false`). It runs against the virtual PCF8574T + HD44780 part in
`parts/hd44780/`, which decodes the expander's nibbles and applies the same
commands to its DDRAM.

`hd44780` is in `spi.tsv` (interfaces `GPIO;I2C;SPI`; this fixture uses the
I2C backpack) and had no fixture here or in `espforge-drivers`. Only the main
happy path is implemented: initialise the display, turn the backlight on,
clear it, print one line, move the cursor to row 2 and print another.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only.

## What is here

| File | What it is |
|---|---|
| `hd44780-test.espf.txt` | the espforge project: `<hd44780 id="lcd" bus="$i2c0" address="0x27" />` |
| `src/app.rs` | initialises, enables the backlight, clears and prints two 16-column lines |
| `drivers/hd44780.kdl` | **the external driver**: `dependency "i2c-character-display" { version "0.6.0" }`, `bus "i2c"` with `address allowed="0x20..=0x27"`, no `codegen.feature`, no driver crate |
| `board.json` | wires the backpack's `SCL` to GPIO 6 and `SDA` to 5 |
| `Cargo.extra.toml` | turns on `esp-hal/esp-emu`, which drops esp-hal's `sr.resp_rec` check that the emulator cannot satisfy |
| `parts/hd44780/` | the virtual backpack and controller's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/hd44780.wasm` | the built virtual display, from `just hd44780-part` |
| `.espforge/drivers.lock` | written by `just hd44780-driver-record`; re-record after editing the manifest |

The spec sits directly in this folder, so the manifest tier is
`drivers/hd44780/drivers/hd44780.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Why there is no driver crate

Manifest-only needs a published crate whose constructor already fits the
manifest's slots. `i2c-character-display` 0.6.0's adapters take the bus first
and use embedded-hal 1.0's `I2c` and `DelayNs` directly, so the manifest can
pass `{bus_device}`, the fixed `LcdDisplayType::Lcd16x2`, and
`espforge_runtime::Delay::new()`:

```text
CharacterDisplayPCF8574T::new_with_address({bus_device}, {address}, LcdDisplayType::Lcd16x2, espforge_runtime::Delay::new())
```

The `{address}` sits directly after the bus, as espforge requires, and
`bus "i2c" { address allowed="0x20..=0x27" }` gives the element the backpack's
address range. The fixture straps its PCF8574T to 0x27, the common all-low
address. The generated type is
`CharacterDisplayPCF8574T<CriticalSectionDevice<..>, Delay>`; `init`, `print`,
`clear`, `set_cursor` and `backlight` are inherent methods on
`BaseCharacterDisplay`.

The bundled `<aip31068>` driver and this crate's AIP31068 support are a
different controller; the HD44780 row had no driver before this fixture.

## Try it in one command

```sh
just hd44780-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's pins
PASS: the first line was printed
PASS: the second line was printed
ok backlight=on blink=off cursor=off display=on line1=HD44780\ ok line2=esp32c3
PASS: the part's DDRAM holds both lines
hd44780-sensor PASS
```

## The steps one at a time

```sh
just hd44780-part-test     # the nibble decoder, commands and DDRAM, on the host
just hd44780-part          # -> parts/hd44780.wasm
just hd44780-driver-record # .espforge/drivers.lock for the local manifest
just hd44780-check         # manifest discovered, lowers, the crate dependency resolves
just hd44780-image         # the generated project compiles -> build/hd44780/hd44780.bin
just hd44780-run           # run with the control channel on 127.0.0.1:5595
```

In a second terminal, while `hd44780-run` is going:

```sh
just hd44780-part-input "part lcd state"
```

After editing `drivers/hd44780.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it with
`just hd44780-driver-record`. Editing `src/app.rs`, the `.espf.txt` or
`board.json` needs no re-record: only the manifest text is pinned.

## Reading the wire

The PCF8574T joins the HD44780's four data lines and three control lines to
one I2C byte:

| Bit | PCF8574T pin | HD44780 |
|---|---|---|
| 0 | P0 | RS (0 command, 1 data) |
| 1 | P1 | RW (0 write, 1 read) |
| 2 | P2 | EN |
| 3 | P3 | backlight |
| 4..7 | P4..P7 | D4..D7 |

A display byte is two nibbles: the driver puts the high nibble and RS on the
port, pulses EN high, then low (the controller latches on the falling edge),
and does the same for the low nibble. The part assembles them, runs the
command when RS is low, and writes DDRAM when RS is high.

The busy check is a read: the driver releases the data lines (`0x0F`) with RS
low and RW high, pulses EN and reads the port. The part answers zero, so the
controller is never busy.

The commands the driver sends and the part models:

| Byte | Meaning |
|---|---|
| `01` | clear display, address counter to 0 |
| `02` | return home |
| `04..=07` | entry mode: increment/decrement |
| `08..=0F` | display on/off, cursor, blink |
| `80..=FF` | set DDRAM address (`0x80` is row 1, `0xC0` row 2) |
| other data | one character into DDRAM at the counter |

The part shows both lines, which is how the recipe proves the nibbles carried
the text:

```text
ok backlight=on blink=off cursor=off display=on line1=HD44780\ ok line2=esp32c3
```

The control channel escapes the space in `HD44780 ok` as `\ `.

The attach line is in `build/hd44780/emu.log` (the firmware console is
`build/hd44780/sensor.stdout`):

```text
co-simulation: part lcd (hd44780) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x27 on GPIO 6/5
```

## Limits

- 16x2 only here: the crate supports other geometries through
  `LcdDisplayType`; the manifest fixes `Lcd16x2`.
- PCF8574T backpack only: the crate also speaks the Adafruit backpack, the
  dual-controller 40x4 adapter, the AIP31068 and the ST7032i; this manifest
  names the generic PCF8574T adapter.
- Four-bit mode with the common `P4..P7` data wiring; other backpack pin
  wirings are not modelled.
- No timing: the part latches at the EN edge and ignores the HD44780's busy
  timing. Reads always answer not-busy.
- The part models DDRAM and the driver's commands, not the character ROM,
  CGRAM, cursor movement on screen, display shifting or contrast.
- Blocking only: no async variant, and `init="deferred"` is offered by the
  manifest but not exercised.
- Happy path only: no negative boards and no missing-pin case.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just hd44780-clean
```
