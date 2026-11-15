# Testing `<sht31>` support with `esp-emu`

How to exercise espforge's SHT31 driver against a virtual Sensirion SHT31-D
using the Espressif emulator (`esp-emu` 1.0.0 in this workspace) and the part
files in `drivers/001-sht31/`.

Every firmware is ESP32-C3, and everything espforge does goes through the root
`justfile` (`just check`, `just build`, `just espforge`, `just esp-emu`), per
`AGENTS.md`. The SHT31 recipes are the `sht31-*` recipes added alongside the
`ota-*` ones.

---

## 1. What is under test

Two halves meet over I2C:

| Half | What it is | Where |
|---|---|---|
| Driver | espforge's `<sht31>` element. Manifest only: it names the `embedded-sht3x` crate and `Sht3x::new(bus, address, delay)`, and `Devices::new` builds it at boot | `espforge/espforge-drivers/manifests/sht31.kdl` |
| Device | A virtual SHT31-D part: 16-bit commands, CRC-8 on every word, single-shot measurements, status register, soft reset | `drivers/001-sht31/sht31.wasm` |
| Wiring | The part's `SCL` on GPIO 6, `SDA` on GPIO 5, address 0x44 (ADDR low) | `drivers/001-sht31/board.json` |
| Firmware | A test app using `<sht31 id="air" bus="$i2c0" address="0x44" />` on `sda=5 scl=6`, printing `TEST:` lines | `drivers/001-sht31/firmware/` |
| Scanner | The espforge `i2c_scanner` example, rebuilt on SDA 5 / SCL 6, for the bus-scan case | `just sht31-scan` |

`board.json`:

```json
{ "parts": {
    "sensor": { "module": "sht31.wasm", "pins": { "SCL": 6, "SDA": 5 } }
} }
```

The firmware uses the stock `embedded-sht3x` I2C driver and does not know the
part is virtual. That is the point: the emulator runs the real firmware image
on an emulated GPIO/I2C peripheral, so the driver's transaction shape (16-bit
command, then a six-byte read, CRCs checked in software) is what gets tested.

What the part models (and therefore what can pass or fail here):

| Command | Behaviour |
|---|---|
| `0x2C06`, `0x2C0D`, `0x2C10`, `0x2400`, `0x240B`, `0x2416` | Single-shot measurement, low/medium/high repeatability, with or without clock stretching. Six bytes: T MSB/LSB/CRC, RH MSB/LSB/CRC |
| `0xF32D` | Status register: two bytes plus CRC. `0x8010` after power-on or soft reset |
| `0x3041` | Clear the status flags (bits 15, 11, 10, 4) |
| `0x30A2` | Soft reset: status back to `0x8010`, any unread answer dropped |

Interactions the control channel can send to the part: `warmer` / `cooler`
(±1 °C), `wetter` / `drier` (±5 %RH), and `set:temperature=<°C>` /
`set:humidity=<%RH>` in one step. It starts at 22.5 °C / 45 %RH and clamps to
-45…130 °C and 0…100 %RH.

`Sht3x::new` sends `0x30A2` (soft reset) at boot, so T2 below is checking that
the part and the driver agree on the reset path, not just on a read.

### Address: 0x44, or 0x45 with `addr`

`drivers/001-sht31/board.json` has no `addr` entry, so the part answers at its
ADDR-low default, **0x44**. The 0x45 address is the ADDR-high strap and must be
stated by the board:

```json
{ "parts": {
    "sensor": { "module": "sht31.wasm", "pins": { "SCL": 6, "SDA": 5 }, "addr": "0x45" }
} }
```

The firmware's `<sht31 … address="0x44" />` must be changed to match; the driver
and part do not auto-detect the address. `just sht31-boards` writes that variant
to `build/sht31/board-0x45.json`, and T7 uses it as the mismatch case. If the
intended board really is 0x45, change the checked-in `board.json` and the
fixture's `.espf.txt` together and T7's roles swap (0x44 becomes the mismatch).

---

## 1a. Status today: the I²C gate is not passing

This plan cannot produce green driver results on the current toolchain.
`esp-hal` 1.2.2 does not complete I²C transactions inside `esp-emu` 1.0.0
(the latest release): every write and read returns
`AcknowledgeCheckFailed(Data)` against a part, and the built-in EEPROM at 0x50
and `--i2c-dev 0:0x44` fail the same way. The part is not at fault — the
co-simulation log shows it receiving the transaction and ACKing it:

```text
co-simulation: part sensor (sht31) attached: ... answers I2C 0x44 on GPIO 6/5
{"dir":"to_part","msg":{"BusRequest":{"op":{"I2c":{"addr":68,"write":[48,162],"read_len":0}}}}}
{"dir":"from_part","msg":{"BusReply":{"ack":true, ...}}}}
```

and the firmware reports:

```text
ERROR - device `air`: initialization failed
```

A minimal espforge project with only `<i2c>` (no SHT31 driver) confirms it is
not the driver: `write`, `write_read` and `read` all fail, and the failure is
the same at 100 kHz and 400 kHz and with no board at all. This is a
toolchain/emulator I²C issue, not a `<sht31>` regression.

Consequences for the recipes:

| Recipe | Today |
|---|---|
| `just sht31-check`, `just sht31-image` | pass: schema, generation, compile and merge |
| `just sht31-sensor`, `just sht31-scan`, `just sht31-negative` | fail at T0/T1 |

The rest of this document is the plan that applies unchanged once I²C
completes a transaction on this pair. Until then, the part and board can still
be validated with the emulator's own ESP-IDF `i2c_sensor-esp32c3.bin` from the
`esp-emu-<version>-examples.tar.gz` release kit: it talks to the same
`board.json` and exercises T0/T3-T6 for the **part**, but not for the `<sht31>`
driver. Fixing this gate (an emulator that completes esp-hal's I²C command
sequence, or an esp-hal pin that does) is the first task, not a test failure to
work around.

---

## 2. Fixtures

Disposable output lives under `build/sht31/` (gitignored).

| Artifact | Source | Purpose |
|---|---|---|
| `drivers/001-sht31/board.json` | checked in, from the esp-emulator sensor example | part at 0x44 on SCL 6 / SDA 5 |
| `drivers/001-sht31/sht31.wasm` | checked in, from the esp-emulator sensor example | the virtual sensor |
| `drivers/001-sht31/firmware/` | checked in: `.espf.txt` + `src/app.rs` | test firmware source |
| `build/sht31/sht31.bin` | `just sht31-image`, merged image | what `esp-emu` boots |
| `build/sht31/sht31.elf` | `just sht31-image`, from `build/build/target/riscv32imc-unknown-none-elf/debug/sht31_test` | symbolised panics |
| `build/sht31/scanner.bin` | `just sht31-scan`, the `i2c_scanner` example on SDA 5 / SCL 6 | T1 bus scan |
| `build/sht31/board-0x45.json` | `just sht31-boards` (part at 0x45) | T7 wrong-address case |
| `build/sht31/board-scl7.json` | `just sht31-boards` (SCL on GPIO 7) | T8 wrong-wiring case |
| `build/sht31/*.stdout`, `emu.log` | the recipes | evidence |

`just sht31-boards` also copies `sht31.wasm` beside the variant boards: the
emulator looks for `module` next to the board file.

---

## 3. Build

```sh
just sht31-image
```

That recipe runs `just check` and `just build` on
`drivers/001-sht31/firmware`, then `cargo espflash save-image --merge` for
ESP32-C3 / 4 MB, and copies the ELF out. It prints both paths.

Sanity checks before running:

```sh
xxd -l 1 build/sht31/sht31.bin     # 0xe9: merged flash image starts with the ROM header byte
stat -c%s build/sht31/sht31.bin     # a debug image, well under 4 MB
```

`just sht31-check` alone is the fast gate: a bad `bus=` reference, an
out-of-range address or an unknown attribute is reported there before any
Rust compile.

---

## 4. Run

The default run uses the checked-in board, opens the control channel on port
5559 (`5558` belongs to the OTA recipes), and keeps the emulator log out of the
firmware console:

```sh
just sht31-run 90s
```

In a second terminal, drive the part:

```sh
just sht31-part "part list"                          # ok sensor
just sht31-part "part sensor input warmer"           # ok
just sht31-part "part sensor state"                  # ok temperature=23.5 humidity=45
just sht31-part "part sensor input set:temperature=31.5"
```

From the firmware console, the expected sequence is:

```text
TEST: sht31 boot
TEST: status=0x8010
TEST: reading t=22.50 rh=45.00
...
```

Attach evidence is on stderr / `build/sht31/emu.log`:

```text
co-simulation: part sensor (sht31) attached: ... answers I2C 0x44 on GPIO 6/5
```

A wrong pin name (lower-case `scl`) is not an error: the part attaches with the
pin unwired and the firmware's `air` device fails to initialize. T8 uses a
visible variant of that.

---

## 5. Test cases

### T0 — Part attaches and the image boots

```sh
just sht31-run 30s
```

Pass: `co-simulation: part sensor (sht31) attached`, `TEST: sht31 boot`, and no
never-used-wiring warning at exit for the I2C pads. This gates the rest.

**Current result: fail at the I²C gate** (`device \`air\`: initialization
failed`, with `AcknowledgeCheckFailed(Data)` in a probe). The attach line and
`TEST: sht31 boot` can appear, but `Devices::new` refuses before the app runs,
so the console shows the device error instead. Treat this as the toolchain
blocker in §1a, not as a sensor or wiring fault.

### T1 — Only the board's sensor answers on its port

The generated `Devices` exposes only the `air` handle, not the bus, so the scan
uses espforge's `i2c_scanner` example rebuilt on the board's pins:

```sh
just sht31-scan
```

Pass:

- `Found device at address 0x44`;
- **no** `0x50` — the emulator's built-in EEPROM is suppressed on a port a
  virtual part is wired to;
- no `0x45` — the part defaults to ADDR low.

The recipe patches the generated example's `<i2c sda="6" scl="7" />` to
`sda="5" scl="6"` before building it, runs it against the same board, and
prints `PASS: T1 ...` or fails. This proves the part's claim (address + the two
pins) is what the firmware sees, not a hard-coded scan result.

### T2 — Driver construction and the soft reset

`Devices::new` already ran `Sht3x::new`, which writes `0x30A2`. The app then
asks for the status register.

Pass: `TEST: status=0x8010` (ALERT_PENDING | RESET) and no
`device \`air\`: initialization failed`. A part that ignored commands or NACKed
would fail `Devices::new` and the boot loop would print the device error.

### T3 — A reading matches the part's air

The part starts at 22.5 °C / 45 %RH. The value travels as raw 16-bit words and
comes back through the crate's conversions.

Pass: `TEST: reading t=22.50 rh=45.00` within a couple of seconds of boot, and
further identical lines once a second. The exact two decimals matter: they
exercise the datasheet conversion (`-45 + 175 × raw / 65535` and
`100 × raw / 65535`), not a fixed string.

### T4 — Interactions move the measurement

While the run is up:

```sh
just sht31-part "part sensor input warmer"     # +1 °C
just sht31-part "part sensor input wetter"     # +5 %RH
```

Pass: the next reading is `t=23.50 rh=50.00`. `cooler` and `drier` must reverse
it. This is the case that fails if the firmware is printing a constant or the
part's inputs are not reaching the sensor.

### T5 — Exact values survive the raw encoding

```sh
just sht31-part "part sensor input set:temperature=31.5"
just sht31-part "part sensor input set:humidity=62"
```

Pass: `t=31.50 rh=62.00`. Also run one negative temperature
(`set:temperature=-10`) and confirm the sign and magnitude survive.

### T6 — Range clamping is the part's, not the driver's

```sh
just sht31-part "part sensor input set:temperature=200"
just sht31-part "part sensor input set:temperature=-200"
just sht31-part "part sensor input set:humidity=150"
just sht31-part "part sensor input set:humidity=-5"
```

Pass: `t=130.00`, `t=-45.00`, `rh=100.00`, `rh=0.00`, each held over the next
few readings. The part clamps to what a 16-bit raw value can carry.

### T7 — Wrong address is a named boot failure

`just sht31-negative` runs this unattended; by hand:

```sh
just sht31-boards
just sht31-run 30s build/sht31/board-0x45.json
```

The part is at 0x45, the firmware talks to 0x44.

Pass: `device \`air\`: initialization failed`, **no** `TEST: reading`, and the
firmware does not silently pretend to measure. The refusal is the driver's
`Result` path in `Devices::new`, not a panic.

The positive counterpart (a board at 0x45 with firmware rebuilt at
`address="0x45"`) is a useful extra: change the `.espf.txt`, rebuild with
`just sht31-image`, and confirm the same readings. It is not in the automated
recipe because it requires a second image.

### T8 — Wrong pins fail the same way, and the emulator says so

```sh
just sht31-run 30s build/sht31/board-scl7.json
```

The part claims I2C on GPIO 7/5, the firmware drives 6/5.

Pass: `device \`air\`: initialization failed`, no readings, and at exit the
emulator reports the part's pads never got a bus (the "wiring the firmware
never used" report). This distinguishes "sensor absent" from "sensor on the
wrong pins" — the first is T9, the second is a board mistake.

`just sht31-negative` covers T7 and T8 in one run.

### T9 — No board at all: prove the readings were not baked in

```sh
just esp-emu "--chip esp32c3 --firmware build/sht31/sht31.bin \
  --control-tcp 127.0.0.1:5559 --timeout 20s --log-file build/sht31/emu-noboard.log"
```

With no `--cosim-board`, the I2C port is the emulator's own: a built-in EEPROM
at 0x50 and nothing at 0x44.

Pass: `device \`air\`: initialization failed`, no reading, and the scan (which
never runs, since `Devices::new` fails first) is irrelevant. If this case ever
prints a reading, T3 is not testing the part.

### T10 — A chip reset does not reset the board

During a run, set a value, then reset the chip and confirm the new value is
still there after the reboot:

```sh
just sht31-part "part sensor input set:temperature=31.5"
printf 'reset\n' | nc -q1 127.0.0.1 5559
```

Pass: the firmware reboots, logs `TEST: sht31 boot` and `status=0x8010` again
(the driver's soft reset re-armed the status register), and the first reading
is `t=31.50`, not `22.50`. The part keeps its air across a chip reset, as on a
real board; scheduled inputs do not survive it.

---

## 6. Automated run

```sh
just sht31-scan        # T1, a separate scanner image
just sht31-sensor      # T0/T2-T5 in one process
just sht31-negative    # T7 + T8
```

`sht31-sensor` builds the image, boots it, waits for the boot, status and room
reading, then sends `warmer`, `wetter` and the exact `set:` values through the
control channel, asserting each `TEST: reading` line. It prints `PASS:` per
step and exits non-zero on the first miss, leaving the captured console in
`build/sht31/sensor.stdout`. `sht31-negative` does the same for the two
failure boards and requires the initialization error and the absence of a
reading.

Both currently stop at T0 because of §1a: the sensor never initializes, so
there is no `TEST: sht31 boot` / `TEST: status` / reading to assert. The
recipes are still the regression harness: when the gate is fixed, they are the
whole pass/fail signal.

| ID | Firmware / board | Expected | Driver claim exercised |
|---|---|---|---|
| T0 | `sht31.bin`, `board.json` | part attaches, firmware boots | board wiring and part load |
| T1 | `scanner.bin`, `board.json` | scan finds only 0x44 | I2C claim: address and pins |
| T2 | same | `status=0x8010` | `Sht3x::new` soft reset, status read + CRC |
| T3 | same | 22.50 °C / 45.00 %RH | single-shot transaction, raw conversion, CRC |
| T4 | same + `warmer`/`wetter` | 23.50 °C / 50.00 %RH | readings follow the part, not a constant |
| T5 | same + `set:` | 31.50 °C / 62.00 %RH, negative °C | exact raw encode/decode |
| T6 | same + out-of-range | 130 / -45 °C, 100 / 0 %RH | part clamping observed through the driver |
| T7 | `board-0x45.json` | `air` init fails, no reading | address is really used |
| T8 | `board-scl7.json` | init fails, unused pads reported | pins really used |
| T9 | no board | init fails, no reading | nothing is faked |
| T10 | `board.json` + reset | set value survives reboot | part state vs chip state |

A green gate is T0–T5 plus T7–T9. T6 and T10 are quick manual checks. On the
current toolchain the gate is red at T0 for the reason in §1a; the table is the
acceptance target, not a claim about today's run.

---

## 7. What this does not prove

- **Timing.** A measurement is ready at once in the part; the real sensor takes
  up to 15 ms and NACKs a read until then without clock stretching. The
  driver's `delay_us(1500)` after reset is executed, but the part never tests a
  timeout.
- **Heater, ALERT limits, periodic mode, Fetch Data, Break.** The part does not
  model them, so neither does this plan.
- **CRC failure paths.** The part always sends correct CRCs; injecting a bad
  CRC needs a different part or a `--cosim-tcp` model. The crate's `BadCrc`
  branch is therefore unreachable here.
- **Electrical bus behaviour** (pull-ups, clock stretching timing, noise) and
  multi-master arbitration: the emulated controller and the part hand bytes to
  each other, they do not model a wire.
- **Raw command coverage.** The driver only sends the commands its crate uses
  (`0x2C0D`, `0xF32D`, `0x30A2`). `clear_status`/`enable_heater` are not on the
  generated `Sht3x` surface this fixture uses; the part's own `cargo test`
  covers those command paths.
- **esp-hal ↔ emulator I²C.** On esp-hal 1.2.2 + esp-emu 1.0.0 no transaction
  completes (`AcknowledgeCheckFailed`), even to the built-in EEPROM. Everything
  from T0 down is therefore unverified on this toolchain; see §1a for the
  evidence and the two ways out (fix the emulator's esp-hal I²C support, or pin
  an esp-hal the emulator handles).

The emulator's own errors name the fix when a board is wrong: a pin the chip
lacks, two parts on one pin, or a second part claiming the same address on the
same pads stops the run before the firmware starts.

---

## 8. Cleanup

```sh
# Ctrl-C the run (or let --timeout end it)
just sht31-clean
```

That removes `build/sht31/` and the unused `build/sht31-src/`; the checked-in
`drivers/001-sht31/board.json`, `sht31.wasm` and `firmware/` stay.
