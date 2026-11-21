# MAX7219 with a project-local, busless, manifest-only driver

An espforge project whose `<max7219>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/max7219.kdl`, a project-local
manifest, and there is no Rust half at all. The manifest uses `bus "none"`:
the chip's three-wire interface is bit-banged on GPIO lines, so the project
builds no SPI peripheral. It names the published `max7219` 0.5.0 crate, whose
`MAX7219::from_pins(1, data, cs, sck)` already fits espforge's busless type and
constructor slots. It runs against the virtual MAX7219 part in
`parts/max7219/`, which decodes the pin edges and latches the same frame the
firmware writes.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `max7219` is in `spi.tsv` and had no fixture here or in
`espforge-drivers`. Only the main happy path is implemented: configure the
chip, clear the digits, set the intensity, power it on, and write the same
frame to all eight digits once a second.

## What is here

| File | What it is |
|---|---|
| `max7219-test.espf.txt` | the espforge project: no peripherals, `<max7219 id="matrix" data="3" cs="7" sck="6" />` |
| `src/app.rs` | sets decode mode, clears the digits, enables all eight, sets intensity 0x0F, powers on, writes `AA` to the eight digits a second, logs `TEST:` lines |
| `drivers/max7219.kdl` | **the external driver**: `dependency "max7219" { version "0.5.0" }`, `bus "none"`, three output pins, no `codegen.feature`, no driver crate |
| `board.json` | wires the part's `DATA` to GPIO 3, `CS` to 7, `SCK` to 6 |
| `parts/max7219/` | the virtual display's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/max7219.wasm` | the built virtual display, from `just max7219-part` |
| `.espforge/drivers.lock` | written by the first `check`/`build`; re-record with `just max7219-driver-record` |

The spec sits directly in this folder, so the manifest tier is
`drivers/max7219/drivers/max7219.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Why there is no driver crate

Manifest-only needs a published crate whose constructor already fits the
manifest's slots. `max7219` 0.5.0's `MAX7219::from_pins(displays, data, cs, sck)`
takes the fixed display count first and then the three GPIOs, and returns a
`Result` (hence `fallible #true`). `bus "none"` means espforge binds no
peripheral and leaves every constructor argument in place, so the fixed `1`
does not have to be a bus handle. The generated type is
`MAX7219<PinConnector<OutputPin, OutputPin, OutputPin>>`; the crate uses
embedded-hal 1.0 like the runtime.

The same crate's hardware-SPI constructor, `from_spi(displays, spi)`, could not
be used on a bus driver: espforge requires `{bus_device}` to be the first
argument, and this constructor takes it second. That is not a problem for the
busless shape, which has no bus argument at all.

## Try it in one command

```sh
just max7219-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's pins
PASS: a frame was written to the digits
ok decode=00 digits=aaaaaaaaaaaaaaaa intensity=0f power=on scan=7 test=off
PASS: the part latched the frame and the driver's control registers
max7219-sensor PASS
```

## The steps one at a time

```sh
just max7219-part-test     # the register file, the latch and the state, on the host
just max7219-part          # -> parts/max7219.wasm
just max7219-check         # manifest discovered, lowers, the crate dependency resolves
just max7219-image         # the generated project compiles -> build/max7219/max7219.bin
just max7219-run           # run with the control channel on 127.0.0.1:5592
```

In a second terminal, while `max7219-run` is going:

```sh
just max7219-part-input "part list"
just max7219-part-input "part matrix state"
```

After editing `drivers/max7219.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just max7219-driver-record
```

Editing `src/app.rs`, the `.espf.txt` or `board.json` needs no re-record: only
the manifest text is pinned.

## Reading the wire

The chip is write-only, so the manifest declares no MISO and no `<spi>`
peripheral. `MAX7219::from_pins` builds a `PinConnector` that drives the lines
itself:

1. CS goes low.
2. For each of the sixteen bits, MSB first: DATA is set to the bit, then SCK
   rises (the chip samples), then SCK falls.
3. CS rises, latching the word.

`setup` puts the chip in the state the recipe expects:

| Word | Meaning |
|---|---|
| `09 00` | decode mode off (raw segment/row bytes) |
| `01 00` … `08 00` | clear the eight digits |
| `0B 07` | scan limit: all eight digits |
| `0A 0F` | maximum intensity |
| `0C 01` | power on |

Once a second `forever` sends `01 AA` … `08 AA`: one sixteen-bit word per
digit, so the frame is `AA` on every row.

The part shows the latched register file, which is how the recipe proves the
pin edges carried the bytes:

```text
ok decode=00 digits=aaaaaaaaaaaaaaaa intensity=0f power=on scan=7 test=off
```

The attach line is in `build/max7219/emu.log` (the firmware console is
`build/max7219/sensor.stdout`):

```text
co-simulation: part matrix (max7219) attached: wired CS=GPIO7 DATA=GPIO3 SCK=GPIO6; drives no pin
```

## Limits

- One display: the constructor's first argument is `1`. The crate supports up
  to eight chained displays; the part models one.
- Bit-banged, not hardware SPI: the driver claims three GPIOs and toggles them
  in software. The emulator models the logic levels, not electrical setup/hold
  times or a maximum clock rate.
- Write-only: no reads and no read-back from the chip. The part models the
  latch and register file, not LEDs, segment current, multiplexing or
  brightness beyond storing the intensity byte.
- Only the last sixteen bits of one CS assertion latch, as with one display on
  real hardware; earlier words are shifted through, and a short pulse latches
  nothing.
- The crate's constructor returns a `Result`; a failed construction is reported
  by `Devices::new` as a device init error.
- Blocking only: no async variant, and `init="deferred"` is offered by the
  manifest but not exercised.
- Happy path only: no negative boards and no missing-pin case.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just max7219-clean
```
