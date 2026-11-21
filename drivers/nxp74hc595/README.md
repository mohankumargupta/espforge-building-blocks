# 74HC595 with a project-local, busless, manifest-only driver

An espforge project whose `<nxp74hc595>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/nxp74hc595.kdl`, a
project-local manifest, and there is no Rust half at all. The manifest uses
`bus "none"`: the chip's five control lines are bit-banged on GPIOs, so the
project builds no SPI peripheral. It names the published `drive_74hc595` 0.2.0
crate, whose `ShiftRegister::new(oe, ser, srclr, srclk, rclk)` already fits
espforge's busless type and constructor slots (embedded-hal 1.0, like the
runtime), and builds the device without failing (`fallible #false`). It runs
against the virtual 74HC595 part in `parts/nxp74hc595/`, which decodes the pin
edges and latches the same byte the firmware shifts out.

`nxp74hc595` is in `spi.tsv` and had no fixture here or in
`espforge-drivers`. Only the main happy path is implemented: enable the
outputs, clear the latches, load one pattern, then the same steady byte once a
second.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only.

## What is here

| File | What it is |
|---|---|
| `nxp74hc595-test.espf.txt` | the espforge project: no peripherals, `<nxp74hc595 id="shift" oe="4" ser="3" srclr="5" srclk="6" rclk="7" />` |
| `src/app.rs` | enables the outputs, clears the latches, loads `5A`, then loads `AA` once a second, logging `TEST:` lines |
| `drivers/nxp74hc595.kdl` | **the external driver**: `dependency "drive_74hc595" { version "0.2.0" }`, `bus "none"`, five output pins, no `codegen.feature`, no driver crate |
| `board.json` | wires the part's `OE` to GPIO 4, `SER` to 3, `SRCLR` to 5, `SRCLK` to 6, `RCLK` to 7 |
| `parts/nxp74hc595/` | the virtual register's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/nxp74hc595.wasm` | the built virtual register, from `just nxp74hc595-part` |
| `.espforge/drivers.lock` | written by `just nxp74hc595-driver-record`; re-record after editing the manifest |

The spec sits directly in this folder, so the manifest tier is
`drivers/nxp74hc595/drivers/nxp74hc595.kdl` — the nested `drivers/` is the
tier, not a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Why there is no driver crate

Manifest-only needs a published crate whose constructor already fits the
manifest's slots. `drive_74hc595` 0.2.0 is generic over five
`embedded_hal::digital::OutputPin`s and its `new` returns `Self` directly. A
busless manifest declares no bus argument, so the five pin holes can be passed
in the crate's own order:

```text
ShiftRegister::new({oe}, {ser}, {srclr}, {srclk}, {rclk})
```

The generated type is
`ShiftRegister<OutputPin, OutputPin, OutputPin, OutputPin, OutputPin>` with
`espforge_runtime::OutputPin`, the same `esp_hal::gpio::Output<'static>` the
runtime hands every other output pin. Its `begin`, `enable_output`,
`output_clear` and `load` are all infallible, which is why the app has no error
arms.

## Try it in one command

```sh
just nxp74hc595-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's pins
PASS: the startup pattern was loaded
PASS: the steady frame was loaded
ok latch=aa output=enabled shift=aa
PASS: the part latched the byte and enabled its outputs
nxp74hc595-sensor PASS
```

## The steps one at a time

```sh
just nxp74hc595-part-test     # the shift register, the latch and the state, on the host
just nxp74hc595-part          # -> parts/nxp74hc595.wasm
just nxp74hc595-driver-record # .espforge/drivers.lock for the local manifest
just nxp74hc595-check         # manifest discovered, lowers, the crate dependency resolves
just nxp74hc595-image         # the generated project compiles -> build/nxp74hc595/nxp74hc595.bin
just nxp74hc595-run           # run with the control channel on 127.0.0.1:5593
```

In a second terminal, while `nxp74hc595-run` is going:

```sh
just nxp74hc595-part-input "part shift state"
```

After editing `drivers/nxp74hc595.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it with
`just nxp74hc595-driver-record`. Editing `src/app.rs`, the `.espf.txt` or
`board.json` needs no re-record: only the manifest text is pinned.

## Reading the wire

The chip is write-only, so the manifest declares no MISO and no `<spi>`
peripheral. The crate drives the lines itself:

1. `begin()` pulls OE low (outputs enabled) and SRCLR high.
2. `output_clear()` pulses SRCLR low, then RCLK, so the latches take zero.
3. `load(byte)` sets SER to each bit, MSB first, pulses SRCLK high then low,
   and finally pulses RCLK to move the eight bits into the output latches.

The part watches those edges:

| Edge | Effect |
|---|---|
| SRCLR falling | clear the shift register; the latches keep their value |
| SRCLK rising, SRCLR high | shift SER in (`shift = shift << 1 | ser`) |
| SRCLK rising, SRCLR low | the register is held clear |
| RCLK rising | copy the shift register to the output latches |

The part shows the latched byte, which is how the recipe proves the pin edges
carried the bits:

```text
ok latch=aa output=enabled shift=aa
```

The attach line is in `build/nxp74hc595/emu.log` (the firmware console is
`build/nxp74hc595/sensor.stdout`):

```text
co-simulation: part shift (nxp74hc595) attached: wired ...
```

## Limits

- One register: the crate's `load` shifts eight bits and pulses RCLK. Chained
  registers are not modelled; the part keeps one 8-bit latch.
- Bit-banged, not hardware SPI: the driver claims five GPIOs and toggles them
  in software. The emulator models the logic levels, not setup/hold times or a
  maximum clock rate.
- Write-only: no reads and no read-back from the chip. The part models the
  shift register and the latches, not output drive current or the Q7' serial
  output.
- No time in the part model: a pulse takes effect at its edges, ignoring
  propagation delay.
- Blocking only: no async variant, and `init="deferred"` is offered by the
  manifest but not exercised.
- Happy path only: no negative boards and no missing-pin case.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just nxp74hc595-clean
```
