# `planmpu9250.md`: an MPU-9250 virtual part and a project-local driver

**Goal:** `drivers/mpu9250/` holds a virtual InvenSense MPU-9250 part, a
project-local espforge `<mpu9250>` driver, and a blocking ESP32-C3 fixture. The
fixture boots, the driver accepts `WHO_AM_I` = 0x71, and a flat part reads
0.00 / 0.00 / 1.00 g at 0 dps and 21.00 °C. Interactions move an accelerometer
axis and turn the gyro. No hardware, no changes to `espforge/`.

**Scope:** the main happy path only. 4-wire SPI, one IMU on one bus, blocking
runtime, one reading a second, the accelerometer/gyro die only. No magnetometer,
no DMP, no FIFO, no interrupts, no self-test or offset calibration, no
`init="deferred"`, no real-hardware validation. `planbme280_spi.md` and
`planmfrc522.md` are the precedents for a project-local driver plus a part under
`drivers/`; this document carries only what the MPU-9250 needs.

**Current state:** `drivers/mpu9250/` does not exist (checked). espforge has no
bundled `<mpu9250>` element, so this plan creates the driver, the part and the
fixture. The built `parts/mpu9250.wasm` will come from `just mpu9250-part`.

## 0. Baseline measured against the code, not assumed

| Fact | Where it comes from |
|---|---|
| espforge has **no bundled MPU-9250 driver**: the manifests are `adxl345`, `ili9341`, `scd4x`, `ssd1306`, `ads111x`, `bme680`, `sht31`, `sht4x`, `mhz19b` | `espforge/espforge-drivers/manifests/` |
| A project-local driver is a manifest under `<project>/drivers/*.kdl` plus a crate beside it, named by `dependency "…" { path "../…" }`, with no `codegen.feature` | `drivers/bme280_spi/drivers/bme280_spi.kdl`, `drivers/bme280_spi/bme280-spi-driver/` |
| A part's source can live in the fixture folder, with `esp-part` at `../../../../esp-emulator/esp-part` | `drivers/bme280_spi/parts/bme280_spi/Cargo.toml` |
| Parts are `cdylib` crates over `esp-part`, built for `wasm32-unknown-unknown`, exported with `export_part!` | `esp-emulator/esp-part/README.md` |
| A SPI part gets one `part_on_bus` per transfer: `tx` is the data phase clocked out, `rx_len` what the master reads | `esp-emulator/proto/PART_ABI.md`, "On SPI" |
| A part that claims a GPIO-driven CS must also name its clock/data pads so the emulator finds the bus | `PART_ABI.md`, "On SPI"; `drivers/bme280_spi/parts/bme280_spi/src/lib.rs` |
| The MPU-9250 wire address is one byte: bit 7 set for a read, bits 6:0 the register; **no dummy byte**, and reads increment the address automatically while CS stays low | MPU-9250 register map, serial interface description |
| No multi-byte bit exists, unlike the ADXL345's `0x40`: `WHO_AM_I` itself is `0x75`, which needs bit 6 as address | register map; `planadxl345.md` for the contrast |
| `WHO_AM_I` is `0x75`, value `0x71`; `0x70` is an MPU-6500 and `0x73` an MPU-9255 | datasheet, `WHO_AM_I` |
| `PWR_MGMT_1` (`0x6B`) powers up `0x40` (sleep, internal 20 MHz clock); bit 7 `0x80` is a self-clearing device reset; `CLKSEL = 1` selects the X-gyro PLL | datasheet, `PWR_MGMT_1` |
| Data are big-endian 16-bit pairs: `0x3B..=0x40` accel X/Y/Z, `0x41`/`0x42` temperature, `0x43..=0x48` gyro X/Y/Z | datasheet, register map |
| Scales at the power-on ranges: 16384 LSB/g (±2 g), 131 LSB/dps (±250 dps), 333.87 LSB/°C with +21 °C at zero | datasheet, sensitivity tables |
| `SMPLRT_DIV` `0x19`, `CONFIG` `0x1A`, `GYRO_CONFIG` `0x1B`, `ACCEL_CONFIG` `0x1C`, `ACCEL_CONFIG2` `0x1D`, `USER_CTRL` `0x6A` are the configuration registers a simple driver writes | datasheet, register map |
| SPI register access is limited to 1 MHz | datasheet, SPI timing |
| The AK8963 magnetometer is a separate die on the auxiliary I2C bus; it is not addressable from the SPI register map | MPU-9250 datasheet, block diagram; AK8963 datasheet |
| espforge drives CS as a plain GPIO around each transaction, not the controller's CS signal | `espforge/espforge-runtime/src/hw/spi.rs`, `ConfiguredSpiDevice` |
| ESP32-C3 pads used by the existing SPI fixtures: SCK 6, MOSI 3, MISO 2, CS 7 | `drivers/bme280_spi/bme280_spi-test.espf.txt`, `drivers/adxl345/board.json` |
| Every espforge call goes through a `justfile` recipe | root `AGENTS.md` |
| All blocks are ESP32-C3 only | root `AGENTS.md` |

## 1. Why the driver is project-local, and why no magnetometer

There is no bundled `<mpu9250>` element, and the published crates (`mpu9250`,
`mpu6050`, `mpu-9250`) are built on `embedded-hal` 0.2 with a `DelayNs`-style
constructor and, for the magnetometer, a **second I2C bus** to the AK8963. That
split does not fit espforge's manifest slots, which give a driver one bus.
Copying `bme280_spi`'s shape keeps the protocol in this repository:

- `drivers/mpu9250/drivers/mpu9250.kdl` names the crate by `path`;
- `drivers/mpu9250/mpu9250-driver/` is generic over
  `embedded_hal::spi::SpiDevice`, so espforge's `ConfiguredSpiDevice`, the
  driver's host tests and a future real board take the same code;
- the manifest's `codegen.runtime` instantiates
  `mpu9250_driver::Mpu9250<espforge_runtime::ConfiguredSpiDevice>`.

The magnetometer is deliberately out of scope. The AK8963 is a second die on
the MPU-9250's auxiliary I2C bus: over SPI it is reachable only by programming
the chip's internal I2C master (`I2C_SLV0_*`, `EXT_SENS_DATA_*`) or by turning
on `INT_PIN_CFG` bypass and wiring a second bus from the MCU. Neither is a
happy path, and the second needs hardware this fixture does not have. The
accelerometer and gyro share one die and are fully readable over SPI, so they
carry the proof that the part is an MPU-9250 (`WHO_AM_I` 0x71) and that the
driver speaks its protocol. `planmfrc522.md`'s "no crypto1" and
`planbme280_spi.md`'s "no I2C" are the same kind of cut.

## 2. Layout

```text
drivers/mpu9250/
  mpu9250-test.espf.txt             # the espforge project
  src/app.rs                        # firmware application half
  board.json                        # CS 7, SCK 6, MOSI 3, MISO 2
  drivers/mpu9250.kdl               # the project-local driver manifest
  mpu9250-driver/
    Cargo.toml                      # generic embedded-hal driver crate
    src/lib.rs
  parts/mpu9250/
    Cargo.toml                      # the virtual part
    src/lib.rs
  parts/mpu9250.wasm                # built by `just mpu9250-part`
  (git-ignored) build output lives in build/mpu9250, not here
```

The part's `esp-part` dependency points into the vendored `esp-emulator/`
checkout, which the root `.gitignore` excludes, exactly like
`drivers/bme280_spi/parts/bme280_spi`.

## 3. The virtual part

`parts/mpu9250/src/lib.rs` models the register file and encodes a live
measurement on every read, like the BME280 part does:

| Register | Behaviour |
|---|---|
| `0x19` `smplrt_div` | storage; powers up `0x00` |
| `0x1A` `config` | storage; powers up `0x00` |
| `0x1B` `gyro_config` | storage; powers up `0x00` (±250 dps) |
| `0x1C` `accel_config` | storage; powers up `0x00` (±2 g) |
| `0x1D` `accel_config2` | storage; powers up `0x00` |
| `0x3B..=0x40` | live acceleration, big-endian pairs X, Y, Z at 16384 LSB/g |
| `0x41`/`0x42` | live temperature, big-endian at 333.87 LSB/°C about 21 °C |
| `0x43..=0x48` | live rotation, big-endian pairs X, Y, Z at 131 LSB/dps |
| `0x6B` `pwr_mgmt_1` | powers up `0x40` (sleep); a write with bit 7 set resets and reads back `0x40`; otherwise stored |
| `0x75` `who_am_i` | read-only `0x71` |
| everything else | stored register file |

The part keeps the **physical** values — `accel_g`, `gyro_dps`, `temperature_c`
— and converts to counts as each register is read, with the exact inverse of
the datasheet scales, clamped to the power-on ranges:

- acceleration: `counts = (g * 16384).round()` clamped to ±32767 (≈ ±2 g);
- rotation: `counts = (dps * 131).round()` clamped to ±32767 (≈ ±250 dps);
- temperature: `counts = ((t - 21) * 333.87).round()`, so 21 °C reads `0x0000`.

Range changes are storage only: a driver that writes `ACCEL_CONFIG` for ±16 g
still gets counts at 16384 LSB/g. That is a modelling limit, stated in §9, not
a protocol bug; it keeps one scale to reason about.

It starts flat on the table (0 g, 0 g, 1 g), still (0 dps) and at 21 °C.
Inputs:

| Name | What it does |
|---|---|
| `set:ax=<g>`, `set:ay=<g>`, `set:az=<g>` | one accelerometer axis, clamped to ±2 g |
| `set:gx=<dps>`, `set:gy=<dps>`, `set:gz=<dps>` | one gyro axis, clamped to ±250 dps |
| `set:temperature=<°C>` | the die temperature, clamped to -40..85 °C |
| `rest` | back to the defaults: flat, still, 21 °C |

A read of `0x3B` with `rx_len` = 14 walks the data block, incrementing the
register for every byte, which is what the driver's single transfer expects;
the emulator gives the part the address byte in `tx` and the length in
`rx_len`. A write is `[address, value]` with `rx_len == 0`.

The part's own tests (`just mpu9250-part-test`) run on the host and check the
identity register, the power-on register values, the exact 14 data bytes, the
big-endian order of each pair, the reset write, the inputs and clamps, the SPI
claim and the state view.

## 4. The driver

`mpu9250-driver/src/lib.rs`:

- `Mpu9250::new(spi)` writes `PWR_MGMT_1 = 0x80` and polls `PWR_MGMT_1` until
  `DEVICE_RESET` clears (bounded retries, so no delay parameter), reads
  `WHO_AM_I` and refuses anything but `0x71` with `WrongDevice(v)`, then writes
  `SMPLRT_DIV = 0x04`, `CONFIG = 0x03`, `ACCEL_CONFIG2 = 0x03`,
  `GYRO_CONFIG = 0x00`, `ACCEL_CONFIG = 0x00` and finally `PWR_MGMT_1 = 0x01`
  (wake, X-gyro PLL);
- `read()` reads fourteen bytes from `0x3B` in one transaction and returns
  `Reading { ax_g, ay_g, az_g, gx_dps, gy_dps, gz_dps, temperature_c }`, all
  `f32`, decoded from big-endian pairs and divided by the same scales the part
  encodes with;
- every transfer is `SpiDevice::transaction`; a register read is the address
  with bit 7 set (`reg | 0x80`) plus `Operation::Read(n)`, so no dummy byte is
  on the wire, and a single register write is `[reg & 0x7F, value]`;
- the error type is `Mpu9250Error<E>`: `Bus(E)`, `WrongDevice(u8)`, `Short`,
  `Stuck`. espforge maps an external constructor's
  error to `DeviceErrorKind::Init`, so a wrong part fails at boot with the
  device named.

The driver's tests (`just mpu9250-driver-test`) use a register-file
`SpiDevice` double: the `WHO_AM_I` check accepts `0x71` and rejects `0x70`, the
reset poll, the six configuration writes and their addresses, the 14-byte read
at `0xBB`, and the decode of a known byte pattern into the expected g, dps and
°C values.

## 5. The espforge project

`mpu9250-test.espf.txt`:

```text
# espforge
  <project name="mpu9250_test" />
  <chip type="esp32c3" />
  <runtime type="blocking" />

# peripherals
  <spi id="spi2" sclk="6" mosi="3" miso="2" mode="0" frequency="1000kHz" />

# devices
  <mpu9250 id="imu" bus="$spi2" cs="7" />
```

The 1 MHz clock is the part's register-access limit, so the busy path runs
inside spec rather than at a made-up speed.

`src/app.rs` logs `TEST: mpu9250 boot` once, then once a second:

```text
TEST: imu ax=0.00 ay=0.00 az=1.00 g gx=0.00 gy=0.00 gz=0.00 dps t=21.00 c
```

`board.json` wires the part's own pin names:

```json
{ "parts": {
    "imu": { "module": "parts/mpu9250.wasm", "pins": { "CS": 7, "SCK": 6, "MOSI": 3, "MISO": 2 } }
} }
```

The four names must agree: `cs="7"` and the `<spi>` pads in the spec,
`board.json`, and the part's `CS`/`SCK`/`MOSI`/`MISO` constants.

## 6. `just` recipes

Everything goes through the root `justfile`, in a section beside
`bme280-spi-*`, with `mpu9250_out := build_dir / "mpu9250"` and the control
channel on `127.0.0.1:5565`:

| Recipe | What it proves |
|---|---|
| `just mpu9250-part-test` | the part's identity, data bytes, reset and inputs, on the host |
| `just mpu9250-part` | builds `parts/mpu9250.wasm` |
| `just mpu9250-driver-test` | the driver's identity check, writes, read and decode, on the host |
| `just mpu9250-check` | the manifest is discovered, lowers, and its `path` dependency resolves |
| `just mpu9250-image` | the generated project compiles → `build/mpu9250/mpu9250.bin` |
| `just mpu9250-run` | boots with the control channel on `127.0.0.1:5565` |
| `just mpu9250-part-input "part imu input set:az=0"` | one control line |
| `just mpu9250-sensor` | the whole happy path |
| `just mpu9250-driver-record` | re-records the manifest lock after editing the KDL |
| `just mpu9250-clean` | removes `build/mpu9250` and the part's `target/` |

Editing the manifest, even a comment, makes the next `check` report
`DRIVER_LOCK_STALE`; `mpu9250-driver-record` fixes it. Editing the crate,
`src/app.rs`, the spec or `board.json` needs no re-record.

## 7. Milestones and expected output

1. `just mpu9250-part-test` — the part passes on the host.
2. `just mpu9250-part` — `parts/mpu9250.wasm` exists.
3. `just mpu9250-driver-test` — the driver passes on the host.
4. `just mpu9250-check` — the element and bus reference lower.
5. `just mpu9250-image` — the generated project compiles.
6. `just mpu9250-sensor` — boot, attach, read, move:

```text
PASS: firmware boots
PASS: the part attached on the firmware's CS
PASS: flat part reads 1 g on Z at 21.00 C
PASS: set:ax=0.5 -> 0.50 g
PASS: set:gz=90 -> 90.00 dps
mpu9250-sensor PASS
```

The attach line is in `build/mpu9250/emu.log`; the firmware console is
`build/mpu9250/sensor.stdout`:

```text
co-simulation: part imu (mpu9250) attached: wired CS=GPIO7 MISO=GPIO2 MOSI=GPIO3 SCK=GPIO6; drives no pin; is the SPI device on chip select GPIO 7
```

## 8. Reading a failure

| Symptom | Cause | Next step |
|---|---|---|
| `initialization failed`, no `TEST` lines | the driver's `WHO_AM_I` check failed; the part answered something other than 0x71, or nothing | confirm `parts/mpu9250.wasm` was copied, then check `mpu9250-part-test` |
| Initialization hangs on `Stuck` | `PWR_MGMT_1` never stops reporting `DEVICE_RESET`, because the reset branch is not modelled as self-clearing | the part must answer `0x40` with bit 7 clear after the reset write |
| Boot prints, then `TEST: imu read failed` | the transfer itself failed in esp-hal | compare with `just bme280-spi-sensor`, which uses the same SPI path and passes |
| Accelerometer axes are swapped or scaled by 256 | the data block is not walked with the register incrementing per byte, or the pairs are read little-endian | the byte order is big-endian; `mpu9250-part-test` matches the exact 14 bytes |
| A fresh part reads `az=1.00` but a moved part snaps back | the part stores counts instead of the physical value, so `set:` updates are overwritten by the conversion | keep `accel_g`/`gyro_dps`/`temperature_c` as the state and encode on read |
| Temperature reads 0 °C on a fresh part | the 21 °C offset is missing from either the encode or the decode | `counts = (t - 21) * 333.87`; the driver adds 21 back |
| `unexpected element` or `unknown attribute` from `check` | the `<mpu9250>` element syntax differs, or the manifest was not discovered | the tag is the manifest's `driver "mpu9250"`; the nested `drivers/` folder is the tier |
| `DRIVER_LOCK_STALE` | the KDL text changed | `just mpu9250-driver-record` |
| End-of-run warning: the part never saw a chip select | the spec's `cs="7"` differs from the board's `CS: 7`, or the bus pads are wrong | make all four agree |
| Emulator refuses the board | a pin name the part does not have, or a pad two parts drive | names are `CS`, `SCK`, `MOSI`, `MISO` in upper case |

## 9. What this does not prove

1. **The magnetometer.** The AK8963 is on the auxiliary I2C bus; no 9-axis
   sample is read, no `I2C_SLV*` master or bypass mode is modelled, and the
   AK8963's own registers, sensitivity adjustment and continuous modes are
   absent.
2. **SPI timing.** Mode 0 at 1 MHz is declared, but the emulator does not model
   polarity, phase or clock speed, so neither the 1 MHz ceiling nor a fast
   burst read is exercised.
3. **Range and filter changes.** `ACCEL_CONFIG`, `GYRO_CONFIG`, `CONFIG` and
   `ACCEL_CONFIG2` are storage. The part always encodes at ±2 g, ±250 dps and
   the drivers always decode with those scales; a ±16 g configuration would
   read wrong against this part and is not covered.
4. **The DMP, FIFO and interrupts.** `FIFO_EN`, `INT_ENABLE`, `INT_STATUS`,
   the data-ready line, `USER_CTRL`'s FIFO/DMP bits and the whole DMP image
   are storage or absent.
5. **Calibration.** No self-test, no factory offsets (`XA_OFFS`…), no bias or
   scale correction; the part's output is exact, with no noise, so a driver's
   averaging or trimming is not exercised.
6. **Sample rate and timing.** `SMPLRT_DIV` does not change how often the part
   updates; it is constant until an input changes it.
7. **Real hardware.** The virtual part validates the protocol, the byte order
   and the scale arithmetic, not the sensor or a particular board. `AD0` and
   the SPI/`I2C` pin strapping are not modelled; the identity is always 0x71.

## 10. Ordered checklist

- [ ] 1. Create `drivers/mpu9250/parts/mpu9250/` (`Cargo.toml`, `src/lib.rs`) — §3
- [ ] 2. Create `drivers/mpu9250/mpu9250-driver/` (`Cargo.toml`, `src/lib.rs`) — §4
- [ ] 3. Create `drivers/mpu9250/drivers/mpu9250.kdl` — §1, §4
- [ ] 4. Create `drivers/mpu9250/mpu9250-test.espf.txt`, `src/app.rs`, `board.json` — §5
- [ ] 5. Add the `mpu9250-*` section to `justfile` — §6
- [ ] 6. `just mpu9250-part-test` — the part passes on the host
- [ ] 7. `just mpu9250-part` — `drivers/mpu9250/parts/mpu9250.wasm` exists
- [ ] 8. `just mpu9250-driver-test` — the driver passes on the host
- [ ] 9. `just mpu9250-driver-record` then `just mpu9250-check` — the element and bus reference lower
- [ ] 10. `just mpu9250-image` — the generated project compiles
- [ ] 11. `just mpu9250-sensor` — all `PASS` lines above
- [ ] 12. `drivers/mpu9250/README.md` records how to run it

## 11. Verification matrix

| Check | What it proves |
|---|---|
| `just mpu9250-part-test` | `WHO_AM_I` 0x71, the power-on register values, the exact 14 big-endian data bytes, the self-clearing reset, the inputs and clamps, the SPI claim and the state view |
| `just mpu9250-driver-test` | the identity check, the reset poll, the six configuration writes at the right addresses, the 14-byte read at `0xBB` and the g/dps/°C decode |
| `just mpu9250-check` | the project-local `<mpu9250>` element, its `cs` and bus reference, and the resource graph |
| `just mpu9250-image` | the generated project names the local crate by path and compiles `Mpu9250<ConfiguredSpiDevice>` with `device!(ctx, imu)` |
| `just mpu9250-sensor` | the whole path: boot, attach on GPIO 7, 1 g on Z at 21.00 °C, 0.50 g on X, 90.00 dps on Z |
