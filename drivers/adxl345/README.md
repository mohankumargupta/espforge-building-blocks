# ADXL345 with the bundled driver

An espforge project whose `<adxl345>` driver is the one bundled in
`espforge-drivers`: no project-local manifest, no driver crate, no KDL. The
generated firmware reads the virtual ADXL345 part over SPI, with no sensor
code glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `planadxl345.md` is the plan and the reasoning.

## What is here

| File | What it is |
|---|---|
| `adxl345-test.espf.txt` | the espforge project: SPI2 on SCK 6 / MOSI 3 / MISO 2, `<adxl345 id="accel" bus="$spi2" cs="7" />` |
| `src/app.rs` | reads once a second and logs `TEST: accel x=<mg> y=<mg> z=<mg> mg` |
| `board.json` | wires the part's `CS` to GPIO 7, `SCK` to 6, `MOSI` to 3, `MISO` to 2 |
| `parts/adxl345/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/adxl345.wasm` | the built virtual part, from `just adxl345-part` |

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just adxl345-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's CS
PASS: flat part reads 1 g on Z
PASS: set:x=0.5 reads back as 512 mg
PASS: set:z=-1 reads back as -1024 mg
adxl345-sensor PASS
```

## The steps one at a time

```sh
just adxl345-part-test   # DEVID, the axes and the inputs, on the host
just adxl345-part        # -> parts/adxl345.wasm
just adxl345-check       # the bundled element, its cs and bus reference
just adxl345-image       # the generated project compiles -> build/adxl345/adxl345.bin
just adxl345-run         # run with the control channel on 127.0.0.1:5562
```

In a second terminal, while `adxl345-run` is going:

```sh
just adxl345-part-input "part list"
just adxl345-part-input "part accel state"
just adxl345-part-input "part accel input set:x=0.5"
just adxl345-part-input "part accel input set:z=-1"
just adxl345-part-input "part accel input rest"
```

## Reading the wire

The driver reads `DEVID` (`0x00`) at construction and fails initialization
unless it is `0xE5`, then reads six bytes from `DATAX0` (`0x32`) with the
read and auto-increment bits set. The generated `ConfiguredSpiDevice` drives
CS as a plain GPIO around each transaction, which is the path this board
exercises.

The driver reports `counts * 4` mg, while the part converts 1 g to 256 counts
(the datasheet's 3.9 mg/LSB), so a part flat on the table reads
`x=0 y=0 z=1024 mg`.

The attach line is in `build/adxl345/emu.log` (the firmware console is
`build/adxl345/sensor.stdout`):

```text
co-simulation: part accel (adxl345) attached: wired CS=GPIO7 MISO=GPIO2 MOSI=GPIO3 SCK=GPIO6; drives no pin; is the SPI device on chip select GPIO 7
```

## Limits

- Happy path only: no negative boards, no missing-CS case, no `init="deferred"`.
- The part never sees SPI mode 3 or the 5 MHz clock: a real ADXL345 needs
  mode 3, but the emulator cannot catch a polarity or timing error.
- The register map beyond `DEVID` and the axes is storage only; ±2 g is
  assumed, with no noise, offset, FIFO or interrupts.
- Blocking only: the bundled async variant (`adxl345-async`) and the I2C
  pinout are out of scope. `repro.md` records why I2C is blocked under this
  emulator anyway.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just adxl345-clean
```
