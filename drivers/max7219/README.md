# MAX7219 with a project-local, manifest-only driver

An espforge project whose `<max7219>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/max7219.kdl`, a project-local
manifest, and there is no Rust half at all. The manifest names the published
`max7219-display` crate, whose `Max7219::new(spi)` already fits espforge's type
and constructor slots. It runs against the virtual MAX7219 part in
`parts/max7219/`, so the same firmware writes a frame the part latches with no
display code glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `max7219` is in `spi.tsv` and had no fixture here or in
`espforge-drivers`. Only the main happy path is implemented: initialize the
chip, power it on, set the intensity, and write the same frame to all eight
digits once a second.

## What is here

| File | What it is |
|---|---|
| `max7219-test.espf.txt` | the espforge project: write-only SPI2 on SCK 6 / MOSI 3, `<max7219 id="matrix" bus="$spi2" cs="7" />` |
| `src/app.rs` | calls `init()`, powers on, sets intensity 0x0F, writes `AA` to the eight digits a second, logs `TEST:` lines |
| `drivers/max7219.kdl` | **the external driver**: `dependency "max7219-display" { version "0.1.5" }`, no `codegen.feature`, no driver crate |
| `board.json` | wires the part's `CS` to GPIO 7, `SCK` to 6, `MOSI` to 3 |
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
manifest's slots. `max7219-display` 0.1.5's `Max7219::new(spi)` is the whole
constructor: it takes an `embedded-hal` 1.0 `SpiDevice`, which is espforge's
`ConfiguredSpiDevice`, and returns the driver without a `Result` (hence
`fallible #false`). The `SpiDevice` transaction is also what drives CS, so the
manifest declares no separate load pin.

The first MAX7219 crate checked, `max7219` 0.5.0, could not be used: its
`from_spi(displays, spi)` takes the bus second, and a manifest constructor must
take `{bus_device}` first.

## Try it in one command

```sh
just max7219-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's CS
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

The chip is write-only. One transaction asserts CS, clocks two bytes — the
register address then its value — and releases CS, which latches the word.
There is no MISO, so `<spi>` omits it and the runtime builds a write-only bus.
The generated `ConfiguredSpiDevice` drives CS as a plain GPIO output around each
transaction, which is the path this board exercises.

`Max7219::new` only stores the bus. `init()` in `setup` sends the setup
sequence:

| Word | Meaning |
|---|---|
| `0C 01` | power on |
| `0F 00` | display test off |
| `0B 07` | scan limit: all eight digits |
| `09 00` | decode mode off (raw segment/row bytes) |
| `01 00` … `08 00` | clear the eight digits |

`setup` then repeats `0C 01` (the app's own `power_on()`) and writes `0A 0F`
(maximum intensity). Once a second `forever` sends `01 AA` … `08 AA`: one
two-byte transfer per digit, so the frame is `AA` on every row.

The part shows the latched register file, which is how the recipe proves the
bytes arrived:

```text
ok decode=00 digits=aaaaaaaaaaaaaaaa intensity=0f power=on scan=7 test=off
```

The attach line is in `build/max7219/emu.log` (the firmware console is
`build/max7219/sensor.stdout`):

```text
co-simulation: part matrix (max7219) attached: wired CS=GPIO7 MOSI=GPIO3 SCK=GPIO6; drives no pin; is the SPI device on chip select GPIO 7
```

## Limits

- One display: the crate defaults to a device count of 1, and the manifest's
  constructor does not call `with_device_count`. The crate itself supports up
  to eight chained displays; the part models one.
- Write-only: no reads and no read-back from the chip. The part models the
  latch and register file, not LEDs, segment current, multiplexing or
  brightness beyond storing the intensity byte.
- Only the last sixteen bits of a transfer latch, as with one display on real
  hardware; earlier words are shifted through.
- `init()` is explicit in `setup`: `Devices::new` only builds the driver. A
  failed init logs an error and the app keeps running, because the manifest's
  constructor is infallible.
- SPI mode 0 at 1 MHz is declared; the chip's 10 MHz limit and the emulator's
  lack of polarity/phase modelling are not exercised.
- Blocking only: no async variant, and `init="deferred"` is offered by the
  manifest but not exercised.
- Happy path only: no negative boards and no missing-CS case.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just max7219-clean
```
