# TMP123 with an external (project-local) driver

A complete espforge project whose `<tmp123>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/tmp123.kdl`, a project-local
manifest, and the Rust half is the `tmp123-driver` crate beside it. It runs
against the emulator's virtual TMP123 SPI part, so the same generated firmware
talks to `tmp123.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `plantmp123.md` is the plan and the reasoning.

## What is here

| File | What it is |
|---|---|
| `tmp123-test.espf.txt` | the espforge project: SPI2 on SCK 6 / MOSI 3 / MISO 2, `<tmp123 id="temp" bus="$spi2" cs="7" />` |
| `src/app.rs` | reads once a second and logs `TEST: reading t=<°C> c` |
| `drivers/tmp123.kdl` | **the external driver**: `dependency "tmp123-driver" { path "../tmp123-driver" }`, no `codegen.feature` |
| `tmp123-driver/` | the driver crate, generic over `embedded_hal::spi::SpiDevice` |
| `board.json` | wires the part's `CS` to GPIO 7, `SCK` to 6, `SO` to 2 |
| `tmp123.wasm` | the virtual sensor, from the emulator's `examples/parts/tmp123` |
| `.espforge/drivers.lock` | written by the first `check`/`build`; git-ignored, re-record with `just tmp123-driver-record` |

The spec sits directly in this folder, so the project tier is
`drivers/tmp123/drivers/tmp123.kdl` — the nested `drivers/` is the tier, not a
typo. Nothing in `espforge/` was changed.

## Why the manifest is not the whole driver

Manifest-only needs a published crate whose constructor already fits
espforge's `type`/`constructor` slots. No TMP121/TMP123 crate exists, so the
manifest names a local crate by `path` instead. When one is published, only the
`dependency` block changes:

```diff
-    dependency "tmp123-driver" {
-        path "../tmp123-driver"
-    }
+    dependency "tmp123-driver" { version "0.1.0" }
```

## Try it in one command

```sh
just tmp123-sensor
```

Builds the crate and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's CS
PASS: fresh part reads 25.00 C
PASS: warmer -> 26.00 C
PASS: set:temperature=31.5 -> 31.50 C
tmp123-sensor PASS
```

## The steps one at a time

```sh
just tmp123-driver-test   # datasheet Table 2, on the host: no toolchain, no emulator
just tmp123-check         # manifest discovered, lowers, `path` dependency resolves
just tmp123-image         # the generated project compiles -> build/tmp123/tmp123.bin
just tmp123-part          # rebuild drivers/tmp123/tmp123.wasm
just tmp123-run           # run with the control channel on 127.0.0.1:5561
```

In a second terminal, while `tmp123-run` is going:

```sh
just tmp123-part-input "part list"
just tmp123-part-input "part thermometer state"
just tmp123-part-input "part thermometer input warmer"
just tmp123-part-input "part thermometer input set:temperature=31.5"
```

After editing `drivers/tmp123.kdl` — even a comment — the lock is stale and the
next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just tmp123-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The sensor has no data input: while CS is low it clocks out one 16-bit word,
MSB first, bits 15..3 = 13-bit two's complement at 0.0625 °C a count. 25 °C is
`0x0C80`. The generated `ConfiguredSpiDevice` drives CS as a plain GPIO output
around each transaction, which is the path this board exercises.

The attach line is in `build/tmp123/emu.log` (the firmware console is
`build/tmp123/sensor.stdout`):

```text
co-simulation: part thermometer (tmp123) attached: wired CS=GPIO7 SCK=GPIO6
SO=GPIO2; drives no pin; is the SPI device on chip select GPIO 7
```

## Limits

- Happy path only: no negative boards, no missing-CS case.
- A failed SPI transfer logs `TEST: reading failed` every second, not an
  initialization error — the constructor is `fallible #false`.
- The part ignores conversion timing, accuracy and the TMP121/TMP123 package
  difference. Mode 0 against real hardware must be checked against the
  datasheet.
- Blocking only: no async variant, and `init="deferred"` is offered by the
  manifest but not exercised.
- One device on `$spi2`; `ConfiguredSpiDevice` owns one CS pad.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just tmp123-clean
```
