# Repro: esp-hal I²C transactions never complete under esp-emu

**Summary.** On ESP32-C3, `esp-hal` 1.2.2's blocking I²C master cannot complete a
single transaction against `esp-emu` 1.0.0. Every write, read and combined
transaction fails `AcknowledgeCheckFailed`, even to the emulator's own built-in
EEPROM at 0x50 with no co-simulation board attached. Nothing above the HAL can
work until one of the two sides is fixed (or `esp-hal` is pinned to a version
the emulator handles).

This blocks `testsht31.md` at T0/T1 and makes `just sht31-sensor`,
`just sht31-scan` and `just i2cscanner-test` fail. The same failure is what
`testsht31.md` §1a records; this document is the standalone, minimal repro.

| | |
|---|---|
| Chip | ESP32-C3 (`riscv32imc-unknown-none-elf`) |
| HAL | `esp-hal` 1.2.2 (via `espforge-runtime`) |
| Emulator | `esp-emu` 1.0.0 |
| Bus | blocking `I2c`, 100 kHz and 400 kHz both affected |
| Board | none needed to reproduce; same failure with `--cosim-board` |
| Toolchain entry | `just` recipes only, per `AGENTS.md` |

---

## 1. Symptom

`just i2cscanner-test` builds the stock espforge `i2c_scanner` example
(`drivers/i2cscanner/`, SDA 5 / SCL 6, 100 kHz), runs it with **no**
`--cosim-board`, and asserts that the emulator's built-in EEPROM at 0x50 is
found. Actual result:

```text
$ just i2cscanner-test
...
FAIL: the built-in EEPROM at 0x50 was not found; see build/i2cscanner/scan.stdout
error: recipe `i2cscanner-test` failed with exit code 1
```

The captured firmware console contains a complete, error-free scan:

```text
INFO - TEST: i2c scanner begin
INFO - Scan complete
```

No `Found device at address 0x50`. The scan is not stuck, panicking, or timing
out: all 126 addresses were probed and every probe was rejected.

With the SHT31 fixture the same gate shows up one layer up:

```text
$ just sht31-sensor
...
ERROR - device `air`: initialization failed
```

plus `FAIL: T0 firmware boots (no 'TEST: sht31 boot')` from the recipe. The
co-simulation log shows the part attached and answering; an address-only
request to 0x44 (`build/sht31/scan-bus.jsonl`, fields elided):

```text
co-simulation: part sensor (sht31) attached: wired SCL=GPIO6 SDA=GPIO5; drives no pin; answers I2C 0x44 on GPIO 6/5
{"dir":"to_part","msg":{"BusRequest":{"op":{"I2c":{"addr":68,"write":[0],"read_len":0}}}}}
{"dir":"from_part","msg":{"BusReply":{"ack":true,"correlation_id":1,"data":[],"op":1}}}
```

---

## 2. Repro A: address probe, no board

This is the checked-in case and the fastest signal.

```sh
just i2cscanner-test
```

What the recipe does:

1. `drivers/i2cscanner/i2cscanner.espf.txt` describes a bare ESP32-C3 project
   with one blocking bus, `sda=5 scl=6 frequency=100kHz`, and no devices.
2. `src/app.rs` is the stock example's scan loop:

   ```rust
   for address in 1..127 {
       match i2c.write(address, &[]) {
           Ok(_) => logger.info(format_args!("Found device at address 0x{:02x}", address)),
           Err(_) => {}
       }
       delay.delay_ms(10);
   }
   logger.info("Scan complete");
   ```

3. The recipe builds a merged image and runs `esp-emu --chip esp32c3` with no
   `--cosim-board`, so I²C port 0 is the emulator's own. Its built-in 24Cxx
   EEPROM answers at 0x50.
4. It asserts exactly one `Found device at address 0x50`.

**Expected:** `Found device at address 0x50`, `Scan complete`, exit 0.

**Actual:** only `Scan complete`; exit 1. A zero-byte write is a real
address-only transaction here: at 0x44, where nothing exists, the same call
fails with a different error than at 0x50 (next section), so the scan is
actually touching the bus.

---

## 3. Repro B: every transaction shape, with a bare `<i2c>` project

This separates "nothing is there" from "the transaction breaks after the
address". It uses a disposable project under `build/` (gitignored) so it does
not touch the checked-in fixtures.

`build/repro/i2c_probe.espf.txt`:

```text
# espforge
  <project name="i2c_probe" />
  <chip type="esp32c3" />
  <runtime type="blocking" />

# peripherals
  <i2c id="i2c0" sda="5" scl="6" frequency="100kHz" />

# devices

# services
```

`build/repro/src/app.rs`:

```rust
use crate::Context;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: probe start");
    let bus = device!(ctx, i2c0);
    let mut byte = [0u8; 1];
    let mut six = [0u8; 6];

    // 0x50 is the emulator's built-in EEPROM when no cosim board is given.
    match bus.write(0x50, &[]) {
        Ok(()) => ctx.logger.info("TEST: 0x50 empty write ok"),
        Err(e) => ctx.logger.error(format_args!("TEST: 0x50 empty write err {e:?}")),
    }
    match bus.write(0x50, &[0x00]) {
        Ok(()) => ctx.logger.info("TEST: 0x50 1-byte write ok"),
        Err(e) => ctx.logger.error(format_args!("TEST: 0x50 1-byte write err {e:?}")),
    }
    match bus.read(0x50, &mut byte) {
        Ok(()) => ctx.logger.info(format_args!("TEST: 0x50 read ok 0x{:02x}", byte[0])),
        Err(e) => ctx.logger.error(format_args!("TEST: 0x50 read err {e:?}")),
    }
    match bus.write_read(0x50, &[0x00], &mut byte) {
        Ok(()) => ctx.logger.info(format_args!("TEST: 0x50 write_read ok 0x{:02x}", byte[0])),
        Err(e) => ctx.logger.error(format_args!("TEST: 0x50 write_read err {e:?}")),
    }

    // 0x44 with no board: nothing should answer at all.
    match bus.write(0x44, &[]) {
        Ok(()) => ctx.logger.info("TEST: 0x44 empty write ok"),
        Err(e) => ctx.logger.error(format_args!("TEST: 0x44 empty write err {e:?}")),
    }
    match bus.read(0x44, &mut six) {
        Ok(()) => ctx.logger.info("TEST: 0x44 read ok"),
        Err(e) => ctx.logger.error(format_args!("TEST: 0x44 read err {e:?}")),
    }
}

pub fn forever(ctx: &mut Context) {
    ctx.delay.delay_ms(1000);
}
```

Build and run it through the same recipes the scanner uses:

```sh
mkdir -p build/repro/src   # then write the two files above
just check build/repro
just build build/repro
(cd build/build && cargo espflash save-image --merge --chip esp32c3 --flash-size 4mb ../repro/probe.bin)
just esp-emu "--chip esp32c3 --firmware build/repro/probe.bin --timeout 15s --log-file build/repro/emu.log" \
    > build/repro/probe-100k.stdout 2>&1
grep 'TEST:' build/repro/probe-100k.stdout
```

Observed at 100 kHz (2026-11-16, `esp-hal` 1.2.2 + `esp-emu` 1.0.0):

| Address | Operation | Result |
|---|---|---|
| 0x50 | `write(&[])` (address only) | `Bus(AcknowledgeCheckFailed(Data))` |
| 0x50 | `write(&[0x00])` | `Bus(AcknowledgeCheckFailed(Data))` |
| 0x50 | `read(&mut [0u8; 1])` | `Bus(AcknowledgeCheckFailed(Data))` |
| 0x50 | `write_read(&[0x00], &mut [0u8; 1])` | `Bus(AcknowledgeCheckFailed(Data))` |
| 0x44 | `write(&[])` | `Bus(AcknowledgeCheckFailed(Unknown))` |
| 0x44 | `read(&mut [0u8; 6])` | `Bus(AcknowledgeCheckFailed(Unknown))` |

The two errors are different in a meaningful way. At 0x44 nothing drives the
bus, and the controller reports `Unknown` (no acknowledge at all). At 0x50 the
error moves to `Data`, i.e. the address phase got an acknowledge and the
failure happens later in the transfer. Every shape fails, so this is not about
an empty buffer being optimised away: the scanner's zero-byte writes do reach
the bus, and so do real data bytes and reads.

The same probe at 400 kHz fails identically (`build/sht31/probe-noboard.stdout`),
so the result does not depend on bus speed. `testsht31.md` §1a records the same
result for both speeds against the SHT31 part.

---

## 4. Repro C: with the co-simulated SHT31 (for contrast)

```sh
just sht31-sensor
```

This runs the real `<sht31 id="air" bus="$i2c0" address="0x44" />` firmware
against `drivers/001-sht31/board.json` and the virtual part. The part attaches
and ACKs (`ack:true` in the bus log), but `Devices::new` cannot complete its
first transaction (`0x30A2`, the crate's soft reset; the error is the same
`AcknowledgeCheckFailed(Data)` as §3, see `testsht31.md` §1a), so the app never
runs:

```text
$ just sht31-sensor
error: recipe `sht31-sensor` failed with exit code 1
FAIL: T0 firmware boots (no 'TEST: sht31 boot'); see build/sht31/sensor.stdout
```

and the captured console is just the boot followed by the device error:

```text
ERROR - device `air`: initialization failed
```

This case proves the part is not silently broken, and the no-board cases prove
the board is not the cause.

---

## 5. What is ruled out

| Candidate | Evidence |
|---|---|
| The SHT31 driver / `embedded-sht3x` | A bare `<i2c>` project with no driver fails identically |
| The virtual part (`sht31.wasm`) | It attaches and replies `ack:true` to a request at 0x44; the built-in EEPROM (no part at all) fails too |
| The board file / wiring | Same failure with no `--cosim-board` and on the emulator's internal EEPROM at 0x50 |
| Address or pins | 0x44 with the part and 0x50 internally both fail; SDA 5 / SCL 6 match `board.json` |
| Bus speed | 100 kHz and 400 kHz both fail |
| Empty writes being no-ops | A zero-byte write at 0x44 fails `Unknown`, at 0x50 fails `Data`, so it does drive the bus |
| Rust application code | The failure is returned by `esp-hal`; the app only logs it and carries on (the scan still completes) |
| Merged image / boot | The image boots, runs `setup`, prints, and exits its scan normally |

What remains is the pair `esp-hal` 1.2.2 ↔ `esp-emu` 1.0.0: either the HAL's
acknowledge check does not understand what the emulated I²C controller reports,
or the controller model does not update the status the HAL reads after the
data phase. The `Unknown` vs `Data` split at two addresses is consistent with
the controller seeing the address ACK and mishandling (or misreporting) the
data-phase ACK.

The emulator's own ESP-IDF `i2c_sensor-esp32c3.bin` (from the
`esp-emu-<version>-examples.tar.gz` kit) is the documented way to exercise the
part through the emulator's own I²C driver instead of `esp-hal`
(`testsht31.md` §1a). Running it against the same `board.json` would confirm
that the remaining fault is on the HAL side of the pairing; it was not run for
this report.

---

## 6. Impact

- `testsht31.md` T0/T1 cannot pass, so every SHT31 driver test is unverified on
  this toolchain. `just sht31-check` / `just sht31-image` still pass.
- `just i2cscanner-test` fails by design as the gate's canary.
- Any espforge project using `<i2c>` (SHT31, or anything else on the bus) is
  affected under this emulator; this is not specific to the sensor fixture.

## 7. Ways out

1. **Fix the pairing.** Make `esp-emu` complete esp-hal 1.2.2's I²C sequence
   (or fix the HAL's acknowledge check against the emulator's controller
   model). This is the real fix: the runtime crate is in this workspace
   (`espforge/`); the emulator binary is installed at `~/.local/bin/esp-emu`.
2. **Pin a different `esp-hal`.** Find the newest version whose I²C master
   works under `esp-emu` 1.0.0 and pin it in `espforge-runtime`, then rerun the
   gate.

Until one of those lands, the virtual part can still be validated with the
emulator's ESP-IDF example, but not through espforge's `<sht31>` driver.

## 8. Done when

- `just i2cscanner-test` prints `Found device at address 0x50` and exits 0.
- The probe in §3 reports `ok` for every 0x50 row and `Unknown` for 0x44.
- `just sht31-sensor`, `just sht31-scan` and `just sht31-negative` get past T0
  (the SHT31 results are then tracked in `testsht31.md`).

## 9. Evidence on disk

| Path | What |
|---|---|
| `build/i2cscanner/scan.stdout` | scanner console: `begin` + `Scan complete`, no devices |
| `build/i2cscanner/emu.log` | emulator log for the no-board run |
| `build/repro/probe-100k.stdout` | the §3 probe output used for the table (100 kHz) |
| `build/sht31/probe-noboard.stdout` | the same probe at 400 kHz, identical errors |
| `build/sht31/probe-noboard.log` | its emulator log |
| `build/sht31/sensor.stdout`, `build/sht31/sensor-rerun.stdout` | `just sht31-sensor`: device init error and the T0 failure |
| `build/sht31/scan-bus.jsonl` | co-sim bus traffic: the part replies `ack:true` at 0x44 |
| `testsht31.md` §1a | the same gate from the SHT31 fixture's point of view |

See also `testsht31.md` §1a and §7 for how this gate interacts with the
planned test cases.
