# AHT20 on I²C with a project-local driver

An espforge project whose `<aht20>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/aht20.kdl`, a project-local
manifest, and the Rust half is the `aht20-driver` crate beside it. It runs
against the emulator's virtual AHT20 I²C part, so the same generated firmware
talks to `parts/aht20.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `aht20` after `ads1x15` and `adxl345`,
which are already here. Only the main happy path is implemented: initialize,
take one measurement a second, read °C and %RH.

## What is here

| File | What it is |
|---|---|
| `aht20-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<aht20 id="climate" bus="$i2c0" address="0x38" />` |
| `src/app.rs` | reads temperature and humidity once a second and logs `TEST:` lines |
| `drivers/aht20.kdl` | **the external driver**: `dependency "aht20-driver" { path "../aht20-driver" }`, no `codegen.feature` |
| `aht20-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/aht20/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/aht20.wasm` | the built virtual part, from `just aht20-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/aht20/drivers/aht20.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just aht20-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the part reads 22.50 C and 45.00 %RH
PASS: set:temperature=30 -> 30.00 C
PASS: set:humidity=60 -> 60.00 %RH
aht20-sensor PASS
```

## The steps one at a time

```sh
just aht20-part-test     # the status, the counts and the CRC, on the host
just aht20-part          # -> parts/aht20.wasm
just aht20-driver-test   # the status check and the decode, on the host
just aht20-check         # manifest discovered, lowers, `path` dependency resolves
just aht20-image         # the generated project compiles -> build/aht20/aht20.bin
just aht20-run           # run with the control channel on 127.0.0.1:5568
```

In a second terminal, while `aht20-run` is going:

```sh
just aht20-part-input "part list"
just aht20-part-input "part climate state"
just aht20-part-input "part climate input set:temperature=30"
just aht20-part-input "part climate input set:humidity=60"
just aht20-part-input "part climate input rest"
```

After editing `drivers/aht20.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just aht20-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The AHT20 has no register pointer. A command is one write and the answer is
the next read; there is no address byte inside the transaction beyond the
usual I²C address, which is always `0x38`.

| Write | Meaning |
|---|---|
| `0x71` | prepare the status byte |
| `0xBE 0x08 0x00` | initialize (calibrate) the part |
| `0xAC 0x33 0x00` | trigger one measurement |

`Aht20::new` reads the status and only sends the initialization command if
bit 3 (calibrated) is clear; a real AHT20 is calibrated at power-on, so the
usual boot is a single status read. The status the part reports when it is
ready is `0x18` (calibrated and idle); bit 7 would say a measurement is still
running.

One measurement is two transactions: the trigger write, then a read of seven
bytes. The first byte repeats the status, the next five carry 20 bits of
humidity and 20 bits of temperature big-endian, and the last is the CRC-8
(polynomial `0x31`, initial value `0xFF`) over the first six:

```text
RH  = count / 2^20 * 100        %RH
T   = count / 2^20 * 200 - 50   °C
```

The part holds 22.5 °C and 45 %RH by default, which encodes as humidity count
471 859 and temperature count 380 109.

The attach line is in `build/aht20/emu.log` (the firmware console is
`build/aht20/sensor.stdout`):

```text
co-simulation: part climate (aht20) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x38 on GPIO 6/5
```

## Limits

- Happy path only: no negative boards, no `init="deferred"`, no uncalibrated
  part in the fixture (the driver's `NotCalibrated` and `Busy` paths are
  covered by its own host tests, not by the emulator).
- No timing: a real part needs 10 ms after initialization and 80 ms for a
  measurement. The driver does not wait, and the emulator completes both at
  once; a part that is still busy is reported as `Busy`, which the fixture
  never sees.
- The CRC byte is generated by the part and ignored by the driver.
- No `0xBA` soft reset and no command/parameter validation beyond the three
  the driver sends; the address is fixed at `0x38`.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just aht20-clean
```
