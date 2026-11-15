# MH-Z19B against the virtual part

A complete espforge project — manifest-only `<mhz19b>` driver over a real
UART — running against the Espressif emulator's virtual MH-Z19B CO₂ sensor.
No hardware, no sensor crate glued by hand, and no ESP-IDF firmware: the same
`edrv-mhz19` crate a real board would use talks to `mhz19b.wasm`.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only.

## What is here

| File | What it is |
|---|---|
| `mhz19b.wasm` | the virtual sensor, from the emulator's `examples/parts/mhz19b` |
| `board.json` | wires the part's `TX` to GPIO 5 and its `RX` to GPIO 4 |
| `firmware/mhz19b-test.espf.txt` | the espforge project: UART1 at 9600 8N1 + `<mhz19b id="co2" bus="$uart1" />` |
| `firmware/src/app.rs` | reads the sensor once a second and logs `TEST: co2=<ppm> ppm` |

The wiring is from the chip's end: the part transmits on GPIO 5, so the chip
receives on GPIO 5 (`rx="5"`) and transmits on the part's RX, GPIO 4
(`tx="4"`). UART1 is used because UART0 is the console.

The driver itself is `espforge/espforge-drivers/manifests/mhz19b.kdl`: a
manifest, not Rust in this repo. It names `edrv-mhz19` as a dependency and
builds `edrv_mhz19::blocking::MHZ19` on the generated `UartDevice`. It works
because `espforge_runtime::UartDevice` implements `embedded_io::Read + Write`.

## Prerequisites

- `esp-emu` 1.0.0 on `PATH` (this workspace has it at `~/.local/bin/esp-emu`)
- the Rust target `riscv32imc-unknown-none-elf`
- `cargo-espflash`
- the espforge CLI: `just build_espforge` if `espforge/target/debug/espforge`
  does not exist yet

## Try it in one command

```sh
just mhz19b-sensor
```

This checks and builds the project, boots the emulator with the virtual part,
waits for the boot and the 400 ppm reading, sends `set:co2=1200` over the
control channel, and asserts the next reading is 1200 ppm. It prints:

```text
PASS: firmware boots
PASS: fresh air reads 400 ppm
PASS: set:co2=1200 reads back as 1200 ppm
mhz19b-sensor PASS
```

If something fails, the captured console is in `build/mhz19b/sensor.stdout`
and the emulator's own log in `build/mhz19b/emu.log`.

## Try it by hand

Build the merged image and the ELF once:

```sh
just mhz19b-image
```

That writes `build/mhz19b/mhz19b.bin` and `build/mhz19b/mhz19b.elf`. The
recipe also runs `just check` first, so a bad bus reference or attribute is
reported before the Rust compile.

Then run it, with the control channel on port 5560:

```sh
just mhz19b-run
```

The firmware console appears on stdout:

```text
INFO - TEST: mhz19b boot
INFO - TEST: co2=400 ppm
INFO - TEST: co2=400 ppm
```

In a second terminal, move the air around the sensor:

```sh
just mhz19b-part "part list"                 # ok co2
just mhz19b-part "part co2 state"            # ok co2=400
just mhz19b-part "part co2 input more"       # +100 ppm
just mhz19b-part "part co2 input set:co2=1200"
```

The next reading reflects it:

```text
INFO - TEST: co2=500 ppm
INFO - TEST: co2=1200 ppm
```

The part starts at 400 ppm (fresh outdoor air), `more`/`less` move it by
100 ppm, and `set:co2=<ppm>` sets an exact value, clamped to the sensor's
0–5000 ppm range. Ctrl-C stops the run; `--timeout` ends it on its own.

To confirm the part really attached, look at `build/mhz19b/emu.log`:

```text
co-simulation: part co2 (mhz19b) attached: wired RX=GPIO4 TX=GPIO5; drives no pin;
is the serial device on GPIO 4/5 (TX/RX from the chip's end)
```

## How the exchange works

The driver sends the manual's nine-byte read command
`FF 01 86 00 00 00 00 00 79`; the part answers
`FF 86 HH LL 00 00 00 00 CS`, where the concentration is `HH * 256 + LL` and
`CS` is the two's complement of the sum of bytes 1–7. `read_co2()` validates
the frame and returns `Reading { co2_ppm, .. }`, which is all the app prints.
Neither side knows the other is virtual: the emulator runs the generated
firmware on its UART1 model and hands bytes to the WebAssembly part.

## Limits

This is the happy main use case only.

- No failure cases: there are no recipes for an unwired part, a bad checksum
  or a wrong baud rate. The manifest driver has no timeout either, so
  `read_co2()` blocks forever if the sensor never answers.
- The part models only the `0x86` read command; calibration and range
  commands are accepted and do nothing, and there is no warm-up or
  measurement delay.
- One device per UART port is an espforge rule; a second part on UART1 is not
  expressible.

## Cleanup

`build/` is disposable:

```sh
just mhz19b-clean
```

That removes `build/mhz19b/`; the checked-in part, board and firmware stay.
