# CCS811 on I²C with a project-local driver

An espforge project whose `<ccs811>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/ccs811.kdl`, a project-local
manifest, and the Rust half is the `ccs811-driver` crate beside it. It runs
against the emulator's virtual CCS811 I²C part, so the same generated firmware
talks to `parts/ccs811.wasm` with no air-quality crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `ccs811` after `ds3231`, which is already
here. Only the main happy path is implemented: identify the part, leave boot
mode, measure once a second, read eCO2 and TVOC.

## What is here

| File | What it is |
|---|---|
| `ccs811-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<ccs811 id="air" bus="$i2c0" address="0x5A" />` |
| `src/app.rs` | reads eCO2 and TVOC once a second and logs `TEST:` lines |
| `drivers/ccs811.kdl` | **the external driver**: `dependency "ccs811-driver" { path "../ccs811-driver" }`, no `codegen.feature` |
| `ccs811-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/ccs811/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/ccs811.wasm` | the built virtual part, from `just ccs811-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/ccs811/drivers/ccs811.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just ccs811-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: fresh air (400 ppm eCO2, 8 ppb TVOC)
PASS: set:eco2=800 -> 800 ppm
PASS: set:tvoc=50 -> 50 ppb
ccs811-sensor PASS
```

## The steps one at a time

```sh
just ccs811-part-test      # the status bits, the result bytes and the inputs, on the host
just ccs811-part           # -> parts/ccs811.wasm
just ccs811-driver-test    # the id check, boot mode and the decode, on the host
just ccs811-check          # manifest discovered, lowers, `path` dependency resolves
just ccs811-image          # the generated project compiles -> build/ccs811/ccs811.bin
just ccs811-run            # run with the control channel on 127.0.0.1:5575
```

In a second terminal, while `ccs811-run` is going:

```sh
just ccs811-part-input "part list"
just ccs811-part-input "part air state"
just ccs811-part-input "part air input set:eco2=800"
just ccs811-part-input "part air input set:tvoc=50"
just ccs811-part-input "part air input rest"
```

After editing `drivers/ccs811.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just ccs811-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x5A (ADDR low; 0x5B with ADDR high). Every transaction is
a register address byte followed by the read or write, as the datasheet's I²C
protocol describes. A write of several bytes fills the registers that follow.

| Register | Meaning |
|---|---|
| `0x00` | status: bit 7 firmware mode, bit 4 application valid, bit 3 data ready, bit 0 error |
| `0x01` | measurement mode: drive mode in bits 6:4, interrupt configuration below |
| `0x02..=0x05` | result: eCO2 and TVOC, big-endian, ppm and ppb |
| `0x11..=0x12` | algorithm baseline |
| `0x20` | hardware id, `0x81` on every CCS811 |
| `0x21` | hardware version |
| `0x23..=0x24`, `0x25..=0x26` | boot and application firmware versions |
| `0xE0` | error id |
| `0xF4` | `APP_START`: writing the register address alone runs the application |
| `0xFF` | soft reset: write `0x11 0xE5 0x72 0x8A` |

`new()` reads the hardware id and requires `0x81`. It then reads the status
byte and requires `APP_VALID` with the error bit clear; a part still in boot
mode (`FW_MODE` clear) gets `APP_START`, after which the status must say the
firmware runs. Finally it writes drive mode 1 to `MEAS_MODE` (`0x10`), one
measurement a second.

`read()` checks the status again — `DeviceError` on the error bit, `NotReady`
while the data-ready bit is clear — and takes four bytes from 0x02. The part
sets `DATA_READY` as soon as a non-idle drive mode is written, so a measurement
is always waiting; a real chip needs up to a second.

The part holds 400 ppm and 8 ppb, fresh air, until a test changes it. The
algorithm's own ranges are enforced: eCO2 stops at 400 ppm below and 8192 ppm
above, TVOC at 1187 ppb.

The attach line is in `build/ccs811/emu.log` (the firmware console is
`build/ccs811/sensor.stdout`):

```text
co-simulation: part air (ccs811) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x5A on GPIO 6/5
```

## Limits

- Happy path only: no raw data registers, no environmental compensation
  (`ENV_DATA`/`NTC`), no baseline save or restore, no threshold interrupts, no
  sleep/wake, no `init="deferred"`, no negative boards.
- No timing: a real part takes up to a second for its first measurement and
  needs 20 minutes of burn-in before its readings mean anything. The part
  model has no conversion time and answers at once, and its eCO2/TVOC are the
  values a test sets, not an algorithm's output.
- The error id is always 0: the part never reports an error, so the driver's
  `DeviceError` path is covered by its host tests, not by the emulator.
- The boot and application firmware versions are fixed at 0x1000 and 0x2000;
  the driver never reads them.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just ccs811-clean
```
