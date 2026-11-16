# `planbme280_spi.md`: a BME280 SPI virtual part and the happy path

**Goal:** `drivers/bme280_spi/` holds a virtual Bosch BME280 part, a
project-local espforge `<bme280_spi>` driver, and a blocking ESP32-C3 fixture.
The fixture boots, the driver accepts `ID` = 0x60, room conditions read back as
22.50 °C / 1013.25 hPa / 45.00 %RH, and interactions change them. No hardware,
no changes to `espforge/`.

**Scope:** the happy path only. 4-wire SPI, one sensor on one bus, blocking
runtime, one-second readings. No I2C, no negative boards, no forced-mode
timing, no IIR filter or interrupts, no real-hardware validation.

**Current state:** `drivers/bme280_spi/` was created empty. It now holds the
part, the driver, the espforge project and the recipes below. The built
`parts/bme280_spi.wasm` comes from `just bme280-spi-part`.

## 0. Baseline measured against the code, not assumed

| Fact | Where it comes from |
|---|---|
| espforge has **no bundled BME280 driver**: the manifests are `adxl345`, `ili9341`, `scd4x`, `ssd1306`, `ads111x`, `bme680`, `sht31`, `sht4x`, `mhz19b` | `espforge/espforge-drivers/manifests/` |
| The bundled `bme680` is **I2C only**, address fixed `0x76`, manifest only over `bosch-bme680` | `espforge-drivers/manifests/bme680.kdl` |
| A project-local driver is a manifest under `<project>/drivers/*.kdl` plus a crate beside it, named by `dependency "…" { path "../…" }`, with no `codegen.feature` | `drivers/tmp123/drivers/tmp123.kdl`, `drivers/tmp123/tmp123-driver/` |
| A part's source can live in the fixture folder, with `esp-part` at `../../../../esp-emulator/esp-part` | `drivers/adxl345/parts/adxl345/Cargo.toml` |
| Parts are `cdylib` crates over `esp-part`, built for `wasm32-unknown-unknown`, exported with `export_part!` | `esp-emulator/esp-part/README.md`, `examples/parts/sht31` |
| A SPI part gets one `part_on_bus` per transfer: `tx` is the data phase clocked out, `rx_len` what the master reads | `esp-emulator/proto/PART_ABI.md`, "On SPI" |
| A part that claims a GPIO-driven CS must also name its clock/data pads so the emulator finds the bus | `PART_ABI.md`, "On SPI"; `drivers/adxl345/parts/adxl345/src/lib.rs` |
| The BME280's `ID` register is `0xD0`, value `0x60` (`0x58` is a BMP280) | BME280 datasheet §5.4.1, register map |
| SPI addressing: the wire address is 7 bits (`datasheet address & 0x7F`); a write sends that, a read sends it with bit 7 set, followed by **one dummy byte**; reads auto-increment, writes take one register | datasheet §6.2, "SPI interface"; ESPHome's `bme280_spi` comment on 0xF7/0x77 |
| Data bytes `0xF7..=0xFE`: pressure MSB/LSB/XLSB, temperature MSB/LSB/XLSB, humidity MSB/LSB | datasheet §5.4, register map |
| Calibration: `0x88..=0xA1` T1..T3, P1..P9, H1; `0xE1..=0xE7` H2..H6; H4/H5 are 12-bit nibble pairs | datasheet §5.4.3, §4.2.3 |
| Compensation formulas and `t_fine` | BME280 datasheet §4.2.3 |
| ESP32-C3 pads used by the existing SPI fixtures: SCK 6, MOSI 3, MISO 2, CS 7 | `drivers/adxl345/adxl345-test.espf.txt`, `drivers/tmp123/board.json` |
| espforge drives CS as a plain GPIO around each transaction, not the controller's CS signal | `espforge/espforge-runtime/src/hw/spi.rs`, `ConfiguredSpiDevice` |
| Every espforge call goes through a `justfile` recipe | root `AGENTS.md` |
| All blocks are ESP32-C3 only | root `AGENTS.md` |

## 1. Why the driver is project-local

There is no bundled `<bme280>` element and no published BME280 SPI crate whose
constructor already fits espforge's manifest slots. Copying `tmp123`'s shape
keeps the whole protocol in this repository:

- `drivers/bme280_spi/drivers/bme280_spi.kdl` names the crate by `path`;
- `drivers/bme280_spi/bme280-spi-driver/` is generic over
  `embedded_hal::spi::SpiDevice`, so espforge's `ConfiguredSpiDevice`, the
  virtual part's host tests and a future real board all take the same code;
- the manifest's `codegen.runtime` instantiates
  `bme280_spi_driver::Bme280Spi<espforge_runtime::ConfiguredSpiDevice>`.

When a published crate fits, only the `dependency` block and the `type` /
`constructor` lines change; the part and the firmware stay as they are.

## 2. Layout

```text
drivers/bme280_spi/
  bme280_spi-test.espf.txt          # the espforge project
  src/app.rs                        # firmware application half
  board.json                        # CS 7, SCK 6, MOSI 3, MISO 2
  drivers/bme280_spi.kdl            # the project-local driver manifest
  bme280-spi-driver/
    Cargo.toml                      # generic embedded-hal driver crate
    src/lib.rs
  parts/bme280_spi/
    Cargo.toml                      # the virtual part
    src/lib.rs
  parts/bme280_spi.wasm             # built by `just bme280-spi-part`
  (git-ignored) build output lives in build/bme280_spi, not here
```

The part's `esp-part` dependency points into the vendored `esp-emulator/`
checkout, which the root `.gitignore` excludes, exactly like
`drivers/adxl345/parts/adxl345`.

## 3. The virtual part

`parts/bme280_spi/src/lib.rs` models the chip's register file and protocol:

| Register | Behaviour |
|---|---|
| `0xD0` `id` | read-only `0x60` |
| `0xE0` `reset` | write `0xB6` at SPI address `0x60` clears `ctrl_hum`, `ctrl_meas` and `config`; reads 0 |
| `0xF2` `ctrl_hum` | writable storage |
| `0xF3` `status` | 0: the part always has a reading ready |
| `0xF4` `ctrl_meas` | writable storage |
| `0xF5` `config` | writable storage |
| `0xF7..=0xFE` | live data: pressure, temperature, humidity |
| everything else | stored register file; the calibration block is programmed at power-on |

The part keeps the **air**, not the raw counts: `temp_c`, `pressure_hpa` and
`humidity`. The emulator delivers a `write_read` as a two-byte command phase
and then an answer phase whose `tx` is the controller's buffer, all zeros; the
part keeps the command while that read is pending, so a long answer phase is
served from it rather than taken for a write. On a data read the part encodes
the count a real chip would have converted:

- temperature: Bosch's quadratic solved for the ADC value (the exact inverse);
- pressure: a binary search over the 20-bit count. Pressure **falls** as the
  count rises, so the search direction is the opposite of temperature's;
- humidity: a binary search over the 16-bit count.

The calibration constants are a BME280 part's values, programmed into
`0x88..=0xA1` and `0xE1..=0xE7`:

| Constant | Value | Constant | Value |
|---|---|---|---|
| T1/T2/T3 | 27504 / 26435 / -1000 | P1..P3 | 36477 / -10685 / 3024 |
| P4..P6 | 2855 / 140 / -7 | P7..P9 | 15500 / -14600 / 6000 |
| H1/H2/H3 | 75 / 370 / 0 | H4/H5/H6 | 313 / 50 / 30 |

It starts at 22.5 °C, 1013.25 hPa and 45 %RH, and stays within -40..85 °C,
300..1100 hPa and 0..100 %RH. Inputs:

| Name | What it does |
|---|---|
| `warmer`, `cooler` | temperature ±1 °C |
| `higher`, `lower` | pressure ±1 hPa |
| `wetter`, `drier` | humidity ±5 %RH |
| `set:temperature=<°C>`, `set:pressure=<hPa>`, `set:humidity=<%RH>` | an exact value |
| `rest` | back to the defaults |

The part's own tests (`just bme280-spi-part-test`) run on the host and check
the ID, the calibration block, auto-incrementing reads, the exact default data
bytes, the input steps and clamps, a control write and reset, the SPI claim and
the state view.

## 4. The driver

`bme280-spi-driver/src/lib.rs`:

- `Bme280Spi::new(spi)` reads `ID`, refuses anything but `0x60` with
  `WrongDevice(id)`, reads the two calibration blocks, then writes
  `ctrl_hum = 0x01`, `config = 0x00`, `ctrl_meas = 0x27` (normal mode,
  ×1 oversampling);
- `read()` reads eight bytes from `0xF7` and applies the three compensation
  formulas, returning `temperature_c`, `pressure_hpa` and
  `humidity_percent`;
- every transfer is `SpiDevice::transaction`; a read frame is
  `[address | 0x80, 0x00]` plus `Operation::Read`, so the dummy byte is on the
  wire, and a write frame is `[address & 0x7F, value]`, because the chip's
  SPI writes use the 7-bit address;
- the error type is `Bme280Error<E>`: `Bus(E)`, `WrongDevice(u8)`, `Short`.
  espforge maps an external constructor's error to `DeviceErrorKind::Init`, so
  a wrong part fails at boot with the device named.

The driver's tests (`just bme280-spi-driver-test`) use a register-file
`SpiDevice` double: the ID check, the three configuration writes, the room
compensation and the byte layout.

## 5. The espforge project

`bme280_spi-test.espf.txt`:

```text
# espforge
  <project name="bme280_spi_test" />
  <chip type="esp32c3" />
  <runtime type="blocking" />

# peripherals
  <spi id="spi2" sclk="6" mosi="3" miso="2" mode="0" frequency="1000kHz" />

# devices
  <bme280_spi id="climate" bus="$spi2" cs="7" />
```

`src/app.rs` logs `TEST: bme280_spi boot` once, then once a second:

```text
TEST: climate t=22.50 c p=1013.25 hpa rh=45.00 %
```

`board.json` wires the part's own pin names:

```json
{ "parts": {
    "climate": { "module": "parts/bme280_spi.wasm", "pins": { "CS": 7, "SCK": 6, "MOSI": 3, "MISO": 2 } }
} }
```

The four names must agree: `cs="7"` and the `<spi>` pads in the spec,
`board.json`, and the part's `CS`/`SCK`/`MOSI`/`MISO` constants.

## 6. `just` recipes

Everything goes through the root `justfile`:

| Recipe | What it proves |
|---|---|
| `just bme280-spi-part-test` | the part's ID, registers, data bytes and inputs, on the host |
| `just bme280-spi-part` | builds `parts/bme280_spi.wasm` |
| `just bme280-spi-driver-test` | the driver's ID check, writes and compensation, on the host |
| `just bme280-spi-check` | the manifest is discovered, lowers, and its `path` dependency resolves |
| `just bme280-spi-image` | the generated project compiles → `build/bme280_spi/bme280_spi.bin` |
| `just bme280-spi-run` | boots with the control channel on `127.0.0.1:5563` |
| `just bme280-spi-part-input "part climate input warmer"` | one control line |
| `just bme280-spi-sensor` | the whole happy path |
| `just bme280-spi-driver-record` | re-records the manifest lock after editing the KDL |
| `just bme280-spi-clean` | removes `build/bme280_spi` and the part's `target/` |

Editing the manifest, even a comment, makes the next `check` report
`DRIVER_LOCK_STALE`; `bme280-spi-driver-record` fixes it. Editing the crate,
`src/app.rs`, the spec or `board.json` needs no re-record.

## 7. Milestones and expected output

1. `just bme280-spi-part-test` — the part passes on the host.
2. `just bme280-spi-part` — `parts/bme280_spi.wasm` exists.
3. `just bme280-spi-driver-test` — the driver passes on the host.
4. `just bme280-spi-check` — the element and bus reference lower.
5. `just bme280-spi-image` — the generated project compiles.
6. `just bme280-spi-sensor` — boot, attach, read, move:

```text
PASS: firmware boots
PASS: the part attached on the firmware's CS
PASS: room conditions
PASS: warmer -> 23.50 C
PASS: set:pressure=1000 -> 1000.00 hPa
PASS: set:humidity=62 -> 62.00 %RH
bme280-spi-sensor PASS
```

The attach line is in `build/bme280_spi/emu.log`; the firmware console is
`build/bme280_spi/sensor.stdout`:

```text
co-simulation: part climate (bme280_spi) attached: wired CS=GPIO7 MISO=GPIO2 MOSI=GPIO3 SCK=GPIO6; drives no pin; is the SPI device on chip select GPIO 7
```

## 8. Reading a failure

| Symptom | Cause | Next step |
|---|---|---|
| `initialization failed`, no `TEST` lines | the driver's `ID` check failed; the part answered something other than 0x60, or nothing | confirm `parts/bme280_spi.wasm` was copied, then check `bme280-spi-part-test` |
| Boot prints, then `TEST: climate read failed` | the transfer itself failed in esp-hal | compare with `just adxl345-sensor`, which uses the same SPI path |
| `unexpected element` or `unknown attribute` from `check` | the `<bme280_spi>` element syntax differs, or the manifest was not discovered | the tag is the manifest's `driver "bme280_spi"`; the nested `drivers/` folder is the tier |
| `DRIVER_LOCK_STALE` | the KDL text changed | `just bme280-spi-driver-record` |
| Readings are all zero or wild | the calibration block or the data byte order is wrong | `just bme280-spi-part-test` checks both |
| The pressure moves the wrong way | the binary search must treat higher counts as lower pressure | the `adc_for_pressure` direction, and `just bme280-spi-driver-test` |
| Emulator refuses the board | a pin name the part does not have, or a pad two parts drive | names are `CS`, `SCK`, `MOSI`, `MISO` in upper case |
| End-of-run warning: the part never saw a chip select | the spec's `cs="7"` differs from the board's `CS: 7`, or the bus pads are wrong | make all four agree |

## 9. What this does not prove

1. **SPI timing.** Mode 0 at 1 MHz is declared, but the emulator does not model
   polarity, phase or clock speed. A real BME280 accepts mode 0 and mode 3;
   the dummy byte is on the wire, but its timing is not.
2. **Conversion timing.** The part always has a reading ready. The real chip
   needs up to ~10 ms in normal mode, and the driver writes normal mode
   `0x27`; no busy polling or forced-mode wait is modeled.
3. **Filters, oversampling and interrupts.** `config` and `ctrl_*` are storage
   only; IIR, standby time, `status` bits and the DRDY pin are absent.
4. **The whole register map.** SPI 3-wire, soft reset timing, `0xE0` read
   behaviour beyond the write and the reserved registers are simplified.
5. **I2C.** Only SPI is modeled; the BME280's I2C address and timing are out
   of scope.
6. **Real hardware.** The virtual part validates the protocol and the
   compensation arithmetic, not a particular sensor's accuracy.

## 10. Ordered checklist

- [x] 1. `just bme280-spi-part-test` — the part passes on the host
- [x] 2. `just bme280-spi-part` — `drivers/bme280_spi/parts/bme280_spi.wasm` exists
- [x] 3. `just bme280-spi-driver-test` — the driver passes on the host
- [x] 4. `just bme280-spi-check` — the element and bus reference lower
- [x] 5. `just bme280-spi-image` — the generated project compiles
- [x] 6. `just bme280-spi-sensor` — all `PASS` lines above

## 11. Verification matrix

| Check | What it proves |
|---|---|
| `just bme280-spi-part-test` | ID 0x60, calibration bytes, auto-increment, the exact room data bytes, inputs and clamps, control write and reset, the SPI claim and the state view |
| `just bme280-spi-driver-test` | the ID check, the three configuration writes, pressure/temperature/humidity compensation and the byte layout |
| `just bme280-spi-check` | the project-local `<bme280_spi>` element, its `cs` and bus reference, and the resource graph |
| `just bme280-spi-image` | the generated project names the local crate by path and compiles `Bme280Spi<ConfiguredSpiDevice>` with `device!(ctx, climate)` |
| `just bme280-spi-sensor` | the whole path: boot, attach on GPIO 7, 22.50 °C / 1013.25 hPa / 45.00 %RH, and two interactions |
