# DS3231 on I²C with a project-local driver

An espforge project whose `<ds3231>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/ds3231.kdl`, a project-local
manifest, and the Rust half is the `ds3231-driver` crate beside it. It runs
against the emulator's virtual DS3231 I²C part, so the same generated firmware
talks to `parts/ds3231.wasm` with no clock crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `ds3231` after `bmxx80`, which is already
here. Only the main happy path is implemented: clear the oscillator-stop flag,
check the time registers, read the date, the time and the die temperature once
a second.

## What is here

| File | What it is |
|---|---|
| `ds3231-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<ds3231 id="rtc" bus="$i2c0" address="0x68" />` |
| `src/app.rs` | reads the date, time and temperature once a second and logs `TEST:` lines |
| `drivers/ds3231.kdl` | **the external driver**: `dependency "ds3231-driver" { path "../ds3231-driver" }`, no `codegen.feature` |
| `ds3231-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/ds3231/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/ds3231.wasm` | the built virtual part, from `just ds3231-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/ds3231/drivers/ds3231.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just ds3231-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the power-on time (2026-01-01 00:00:00, day 1)
PASS: the die temperature (25.00 C)
PASS: set:* -> 2030-06-15 08:30:00 on day 3
PASS: set:temperature=21.5 -> 21.50 C
ds3231-sensor PASS
```

## The steps one at a time

```sh
just ds3231-part-test      # the registers, the BCD and the inputs, on the host
just ds3231-part           # -> parts/ds3231.wasm
just ds3231-driver-test    # the flag, the decode and the temperature, on the host
just ds3231-check          # manifest discovered, lowers, `path` dependency resolves
just ds3231-image          # the generated project compiles -> build/ds3231/ds3231.bin
just ds3231-run            # run with the control channel on 127.0.0.1:5574
```

In a second terminal, while `ds3231-run` is going:

```sh
just ds3231-part-input "part list"
just ds3231-part-input "part rtc state"
just ds3231-part-input "part rtc input set:year=2030"
just ds3231-part-input "part rtc input set:month=6"
just ds3231-part-input "part rtc input set:day=15"
just ds3231-part-input "part rtc input set:hour=8"
just ds3231-part-input "part rtc input set:minute=30"
just ds3231-part-input "part rtc input set:temperature=21.5"
just ds3231-part-input "part rtc input rest"
```

After editing `drivers/ds3231.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just ds3231-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x68, its only address. Every transaction is a register
address byte followed by the read or write, as the datasheet's I²C protocol
describes; a write of several bytes fills the registers that follow, which is
how the driver writes a whole time in one go.

| Register | Meaning |
|---|---|
| `0x00` | seconds, BCD, bit 7 unused |
| `0x01` | minutes, BCD |
| `0x02` | hours, BCD; bit 6 clear is 24-hour mode, bit 5 is PM in 12-hour mode |
| `0x03` | day of week, 1 to 7, user-defined |
| `0x04` | date (day of month), BCD |
| `0x05` | month, BCD; bit 7 is the century |
| `0x06` | year, BCD: the last two digits |
| `0x07..=0x0A` | alarm 1 |
| `0x0B..=0x0D` | alarm 2 |
| `0x0E` | control: the square wave, the interrupt output and the alarm enables |
| `0x0F` | status: bit 7 oscillator stop, bit 3 32 kHz enable, bits 1:0 the alarm flags |
| `0x10` | aging offset |
| `0x11..=0x12` | die temperature: signed whole degrees, then the quarters in bits 7:6 |

`new()` reads the status byte and clears the oscillator-stop flag, which is set
on a part whose oscillator stopped and which a firmware clears once the time is
known. It then reads the seven time registers: 0x68 is a crowded address, and a
part that answers there without valid BCD is not a DS3231, which is
`Ds3231Error::InvalidTime`. `datetime()` reads those registers again and
decodes them; `temperature()` reads the two temperature bytes and returns
`MSB + (LSB >> 6) * 0.25` degrees Celsius.

The time is BCD: `0x26` is 26, and every field is validated on the way in and
out. The part holds 2026-01-01 00:00:00, day 1, and 25.00 °C until a test
changes them.

The attach line is in `build/ds3231/emu.log` (the firmware console is
`build/ds3231/sensor.stdout`):

```text
co-simulation: part rtc (ds3231) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x68 on GPIO 6/5
```

## Limits

- Happy path only: no alarms, no square-wave or interrupt configuration, no
  aging offset, no 32 kHz enable, no `init="deferred"`, no negative boards.
  Those registers are plain storage in the part and are never touched by the
  driver beyond the status byte.
- No time: the part holds the time it is given and does not count seconds, and
  the driver never waits for the part's one-second update. A test moves the
  clock with `part rtc input set:second=...` instead.
- The century bit is modelled, so 2100 to 2199 read back, but the part does not
  treat February in a leap year differently from any other month: the driver
  checks the field ranges, not the calendar.
- The 12-hour register is decoded by the driver and covered by its host tests,
  but the part always writes 24-hour time, so the fixture never exercises it.
- The temperature is quarter-degree steps, held by the part as a float and
  rounded on the way out; a real chip has the same 0.25 °C resolution.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just ds3231-clean
```
