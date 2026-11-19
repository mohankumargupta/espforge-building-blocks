# SCD4x with the bundled driver

An espforge project whose `<scd4x>` driver is the one bundled in
`espforge-drivers`: no project-local manifest, no driver crate, no KDL. The
generated firmware reads the virtual SCD4x part over I²C, with no sensor code
glued in by hand.

The tag is already bundled, and espforge refuses a duplicate tag (`driver
add` and `check` both reject a local `scd4x`), so this fixture takes the same
shape as `drivers/adxl345`: the new code here is the virtual part and the
happy-path app. Everything goes through the root `justfile`, per `AGENTS.md`,
and the firmware is ESP32-C3 only. `i2cdevices.md` lists `scd4x` after
`mcp9808`, which was the previous fixture. Only the main happy path is
implemented: start periodic measurement, read CO2, temperature and humidity
once a second.

## What is here

| File | What it is |
|---|---|
| `scd4x-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<scd4x id="co2" bus="$i2c0" address="0x62" />` |
| `src/app.rs` | starts measurement in `setup`, then reads once a second and logs `TEST:` lines |
| `parts/scd4x/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/scd4x.wasm` | the built virtual part, from `just scd4x-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just scd4x-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the bundled driver starts periodic measurement
PASS: the part attached on the firmware's SDA/SCL
PASS: the default 400 ppm at 22.49 °C / 44.99 %RH
PASS: set:co2=1000 -> 1000 ppm
PASS: set:temperature=30, set:humidity=60 -> 30.00 °C / 60.00 %RH
scd4x-sensor PASS
```

## The steps one at a time

```sh
just scd4x-part-test       # the encoding, the CRC and the inputs, on the host
just scd4x-part            # -> parts/scd4x.wasm
just scd4x-check           # the bundled element and its fixed address
just scd4x-image           # the generated project compiles -> build/scd4x/scd4x.bin
just scd4x-run             # run with the control channel on 127.0.0.1:5581
```

In a second terminal, while `scd4x-run` is going:

```sh
just scd4x-part-input "part list"
just scd4x-part-input "part co2 state"
just scd4x-part-input "part co2 input set:co2=1000"
just scd4x-part-input "part co2 input set:temperature=30"
just scd4x-part-input "part co2 input set:humidity=60"
just scd4x-part-input "part co2 input rest"
```

There is no manifest here to re-record: the driver comes from espforge.

## Reading the wire

The part answers at 0x62, its only address. It is command-driven and has no
register pointer; a command is two bytes, big-endian:

| Command | Meaning |
|---|---|
| `0x21B1` | start periodic measurement |
| `0x3F86` | stop periodic measurement |
| `0xEC05` | select the last measurement for the next read |

`Scd4x::new` has no init sequence; the app calls `start()`. Each `read()`
sends `0xEC05` and then reads nine bytes in one `write_read`: three 16-bit
big-endian words, each followed by a CRC-8 (poly `0x31`, init `0xFF`):

| Word | Meaning |
|---|---|
| 1 | CO2 in ppm |
| 2 | temperature, `-45 + 175 * raw / 65536` °C |
| 3 | relative humidity, `100 * raw / 65536` % |

The bundled driver checks each CRC and reports `Scd4xError::Crc` on a bad
word; it exposes hundredths (`temperature_centi_c`,
`humidity_centi_percent`), which the app divides by 100. Its integer
conversion truncates, so the part's 22.50 °C and 45.00 %RH read back as
22.49 °C and 44.99 %RH; 30 °C and 60 %RH happen to read back exactly.

400 ppm, 22.50 °C and 45.00 %RH encode as `0x0190`, `0x62BE` and `0x7333`,
with CRC bytes `0x4C`, `0xAD` and `0x01`. The part holds those until a test
changes them.

The attach line is in `build/scd4x/emu.log` (the firmware console is
`build/scd4x/sensor.stdout`):

```text
co-simulation: part co2 (scd4x) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x62 on GPIO 6/5
```

## Limits

- Happy path only: no stop, wake-up, data-ready polling, serial number,
  forced recalibration, automatic self-calibration, sensor altitude, ambient
  pressure or temperature-offset commands, no `init="deferred"`, no negative
  boards.
- No measurement timing: the part answers at once whatever its interval, and
  the driver waits for none of it.
- The driver's centi-unit conversion truncates, so values the part encodes
  exactly can read back one hundredth low.
- CO2 is held to 400–40000 ppm in the part; the datasheet's 0–40000 range is
  otherwise unmodelled.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just scd4x-clean
```
