# ADS1115 on I²C with a project-local driver

An espforge project whose `<ads1x15>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/ads1x15.kdl`, a project-local
manifest, and the Rust half is the `ads1x15-driver` crate beside it. It runs
against the emulator's virtual ADS1115 I²C part, so the same generated firmware
talks to `parts/ads1x15.wasm` with no ADC crate glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `i2cdevices.md` lists `ads1x15` first; this is that entry.

## What is here

| File | What it is |
|---|---|
| `ads1x15-test.espf.txt` | the espforge project: I²C0 on SDA 5 / SCL 6, `<ads1x15 id="adc" bus="$i2c0" address="0x48" />` |
| `src/app.rs` | reads all four channels and AIN0−AIN1 once a second and logs `TEST:` lines |
| `drivers/ads1x15.kdl` | **the external driver**: `dependency "ads1x15-driver" { path "../ads1x15-driver" }`, no `codegen.feature` |
| `ads1x15-driver/` | the driver crate, generic over `embedded_hal::i2c::I2c` |
| `parts/ads1x15/` | the virtual part's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/ads1x15.wasm` | the built virtual part, from `just ads1x15-part` |
| `board.json` | wires the part's `SCL` to GPIO 6 and `SDA` to GPIO 5; ADDR low = 0x48 |
| `Cargo.extra.toml` | the esp-hal `esp-emu` I²C overlay, see `i2c-drivers-workaround.md` |

The spec sits directly in this folder, so the manifest tier is
`drivers/ads1x15/drivers/ads1x15.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's source lives here rather than in the vendored `esp-emulator/` tree,
so it is versioned with the fixture. Its `esp-part` dependency points into
`esp-emulator/esp-part`, which the root `.gitignore` excludes.

The part is an **ADS1115** (16-bit). The driver decodes either chip; the
manifest's constructor names `Chip::Ads1115`, and switching it to
`Chip::Ads1015` selects the 12-bit scaling.

## Try it in one command

```sh
just ads1x15-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's SDA/SCL
PASS: the four channels at their defaults
PASS: AIN0 - AIN1 reads -0.512 V
PASS: set:ain2=1 -> 1.000 V
PASS: set:ain0=1.25 -> 1.250 V
ads1x15-sensor PASS
```

## The steps one at a time

```sh
just ads1x15-part-test     # the register file, the conversions, the gain and the inputs, on the host
just ads1x15-part          # -> parts/ads1x15.wasm
just ads1x15-driver-test   # the configuration write and the decode, on the host
just ads1x15-check         # manifest discovered, lowers, `path` dependency resolves
just ads1x15-image         # the generated project compiles -> build/ads1x15/ads1x15.bin
just ads1x15-run           # run with the control channel on 127.0.0.1:5567
```

In a second terminal, while `ads1x15-run` is going:

```sh
just ads1x15-part-input "part list"
just ads1x15-part-input "part adc state"
just ads1x15-part-input "part adc input set:ain0=1.25"
just ads1x15-part-input "part adc input set:ain2=1"
just ads1x15-part-input "part adc input rest"
```

After editing `drivers/ads1x15.kdl` — even a comment — the lock is stale and
the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just ads1x15-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The ADS111x has no identity register, so `new()` reads `config` (`0x01`) to
prove the part answers, then leaves it at its power-on value (`0x8583`:
single-shot, AIN0−AIN1, ±2.048 V, 128 SPS, comparator off).

Each `read(input, gain)` is two transactions. First a write of the pointer and
the 16-bit big-endian configuration with `OS` (bit 15) set, which starts one
single-shot conversion:

| Bits | Field | Value written |
|---|---|---|
| 15 | `OS` | 1, start a conversion |
| 14:12 | `MUX` | `100`..`111` single-ended AIN0..AIN3, `000`..`011` the four pairs |
| 11:9 | `PGA` | the `Gain` range, `010` ±2.048 V by default |
| 8 | `MODE` | 1, single-shot |
| 7:5 | `DR` | `100`, 128 SPS |
| 4:0 | comparator | `00011`, disabled |

Then a read of `conversion` (`0x00`), two big-endian bytes, decoded as a signed
count and scaled by the range: 62.5 µV per count at ±2.048 V on the ADS1115,
1 mV on the ADS1015 (whose 12-bit code sits in the top bits of the same word).

The defaults the part holds are 0.512 V, 1.024 V, 1.500 V and 0.128 V on
AIN0..AIN3, so AIN0−AIN1 is −0.512 V (count 0xE000).

The attach line is in `build/ads1x15/emu.log` (the firmware console is
`build/ads1x15/sensor.stdout`):

```text
co-simulation: part adc (ads1x15) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x48 on GPIO 6/5
```

## Limits

- Happy path only: no negative boards, no `init="deferred"`, no 0x49..0x4B
  variant (the manifest and the part support it; only the checked-in board and
  `.espf.txt` are at 0x48).
- The part is 16-bit only: it models an ADS1115, so the driver's ADS1015
  scaling is covered by its own host tests, not by the emulator.
- No conversion timing, no data-rate effect, no comparator: a conversion is
  ready as soon as the configuration write lands. The driver does not poll
  `OS`, so on real hardware the first read after the start can still be the
  previous sample; the emulator converts immediately.
- The two comparator thresholds (`0x02`, `0x03`) are plain storage; nothing
  raises an alert pin.
- The host's I²C overlay is required — see `i2c-drivers-workaround.md`. Never
  flash an `esp-emu` build to a board.
- Blocking only: no async variant.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just ads1x15-clean
```
