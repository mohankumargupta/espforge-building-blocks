# TMP102 on I²C with a project-local driver

An espforge project whose `<tmp102>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/tmp102.kdl`, a project-local
manifest, and the Rust half is the `tmp102-driver` crate beside it. It runs
against the emulator's virtual TMP102 I²C part, so the same generated firmware
talks to `parts/tmp102.wasm` with no sensor crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `tmp102` is the last entry in `i2cdevices.md` and the one after
`tlv493d`, so it is the next that was not implemented. Only the main happy
path is implemented: read the temperature register once a second and report it
in degrees Celsius.

## What is here

| File | What it is |
|---|---|
| `tmp102-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<tmp102 id="temp" bus="$i2c0" address="0x48" />` |
| `src/app.rs` | reads the temperature a second and logs `TEST:` lines |
| `drivers/tmp102.kdl` | **the external driver**: `dependency "tmp102-driver" { path "../tmp102-driver" }`, no `codegen.feature` |
| `tmp102-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/tmp102/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/tmp102.wasm` | the built virtual part, from `just tmp102-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/tmp102/drivers/tmp102.kdl` — the nested `drivers/` is the tier, not a
typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

## Try it in one command

```sh
just tmp102-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: a fresh part reads 25.00 C
PASS: set:temperature=26.5 -> 26.50 C
PASS: set:temperature=-5 -> -5.00 C
tmp102-sensor PASS
```

## The steps one at a time

```sh
just tmp102-part-test      # the register file and the left-justified bytes, on the host
just tmp102-part           # -> parts/tmp102.wasm
just tmp102-driver-test    # the pointer write and the decode, on the host
just tmp102-check          # manifest discovered, lowers, `path` dependency resolves
just tmp102-image          # the generated project compiles -> build/tmp102/tmp102.bin
just tmp102-run            # run with the control channel on 127.0.0.1:5588
```

In a second terminal, while `tmp102-run` is going:

```sh
just tmp102-part-input "part list"
just tmp102-part-input "part temp state"
just tmp102-part-input "part temp input set:temperature=26.5"
just tmp102-part-input "part temp input set:temperature=-5"
```

After editing `drivers/tmp102.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just tmp102-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The part answers at 0x48, its address with ADD0 tied to ground (0x49/0x4A/0x4B
need the strap on V+/SDA/SCL, which neither the driver nor the part models).
Every access is a write of the register pointer followed by a read of two
bytes, most significant first:

| Pointer | Register | Power-up value |
|---|---|---|
| `0x00` | temperature, read-only | the part's measurement |
| `0x01` | configuration | `0x60A0`: 12-bit, continuous conversion |
| `0x02` | T_LOW | `0x4B00`, -75 °C |
| `0x03` | T_HIGH | `0x5000`, 80 °C |

`new()` writes the pointer `0x00` and reads the two bytes once, which is the
check that the sensor answers and its value is dropped. `read()` does the same
thing again and converts:

```text
temperature = signed 12-bit count * 0.0625 °C
```

The register is left justified: the first byte is the count's top eight bits,
the second holds the low four in bits 7:4, and bits 3:0 read as zero. At 25 °C
the count is 400 (`0x190`), so the two bytes are `0x19 0x00`. The part starts
at 25 °C until a test moves it.

A read that goes on past a register walks the register file and wraps at its
end, so reading eight bytes from `0x00` returns the temperature, the
configuration, T_LOW and T_HIGH, and a ninth byte is the temperature again. A
write to a pointer the part lacks is NACKed.

The attach line is in `build/tmp102/emu.log` (the firmware console is
`build/tmp102/sensor.stdout`):

```text
co-simulation: part temp (tmp102) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x48 on GPIO 6/5
```

## Limits

- Happy path only: the part converts continuously at its power-up
  configuration, so the driver writes no configuration — no shutdown
  (one-shot), no extended 13-bit mode, no conversion rate, no alert polarity
  or fault queue — and never touches T_LOW/T_HIGH.
- No ALERT line: the thermostat output and its interrupts are not modelled.
- The address strap is fixed at 0x48; no other strap is reachable.
- The part models the register file, the pointer walk and the left-justified
  format, nothing else: no conversion time, no ADC noise, no self-heating,
  and the temperature is held to the -40…125 °C range.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just tmp102-clean
```
