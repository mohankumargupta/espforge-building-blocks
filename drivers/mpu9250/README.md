# MPU-9250 with an external (project-local) driver

An espforge project whose `<mpu9250>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/mpu9250/drivers/mpu9250.kdl`,
a project-local manifest, and the Rust half is the `mpu9250-driver` crate
beside it. It runs against the virtual MPU-9250 part in `parts/mpu9250/`, so the
same generated firmware talks to `mpu9250.wasm` with no IMU code glued in by
hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `planmpu9250.md` is the plan and the reasoning.

## What is here

| File | What it is |
|---|---|
| `mpu9250-test.espf.txt` | the espforge project: SPI2 on SCK 6 / MOSI 3 / MISO 2, `<mpu9250 id="imu" bus="$spi2" cs="7" />` |
| `src/app.rs` | reads once a second and logs `TEST: imu ax=… ay=… az=… g gx=… gy=… gz=… dps t=… c` |
| `drivers/mpu9250.kdl` | **the external driver**: `dependency "mpu9250-driver" { path "../mpu9250-driver" }`, no `codegen.feature` |
| `mpu9250-driver/` | the driver crate, generic over `embedded_hal::spi::SpiDevice` |
| `board.json` | wires the part's `CS` to GPIO 7, `SCK` to 6, `MOSI` to 3, `MISO` to 2 |
| `parts/mpu9250/` | the virtual IMU's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/mpu9250.wasm` | the built virtual IMU, from `just mpu9250-part` |
| `.espforge/drivers.lock` | written by the first `check`/`build`; re-record with `just mpu9250-driver-record` |

The spec sits directly in this folder, so the manifest tier is
`drivers/mpu9250/drivers/mpu9250.kdl` — the nested `drivers/` is the tier, not
a typo. Nothing in `espforge/` was changed.

The part's `esp-part` dependency points into the vendored `esp-emulator/` tree,
which the root `.gitignore` excludes, exactly like
`drivers/bme280_spi/parts/bme280_spi`.

## Try it in one command

```sh
just mpu9250-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's CS
PASS: flat part reads 1 g on Z at 21.00 C
PASS: set:ax=0.5 -> 0.50 g
PASS: set:gz=90 -> 90.00 dps
mpu9250-sensor PASS
```

## The steps one at a time

```sh
just mpu9250-part-test     # identity, power-on registers, the 14 data bytes, the reset, the inputs
just mpu9250-part          # -> parts/mpu9250.wasm
just mpu9250-driver-test   # the identity check, the reset poll, the configuration writes, the decode
just mpu9250-check         # the element and its bus reference lower
just mpu9250-image         # the generated project compiles -> build/mpu9250/mpu9250.bin
just mpu9250-run           # run with the control channel on 127.0.0.1:5565
```

In a second terminal, while `mpu9250-run` is going:

```sh
just mpu9250-part-input "part imu state"
just mpu9250-part-input "part imu input set:ax=0.5"
just mpu9250-part-input "part imu input set:az=0"
just mpu9250-part-input "part imu input set:gz=90"
just mpu9250-part-input "part imu input set:temperature=25"
just mpu9250-part-input "part imu input rest"
```

After editing `drivers/mpu9250/drivers/mpu9250.kdl` — even a comment — the lock
is stale and the next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just mpu9250-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The MPU-9250's SPI protocol is one address byte per transfer: bit 7 set for a
read and bits 6:0 the register, and **no dummy byte**. The address increments
across a read while CS stays low, which is why one transaction can take all
fourteen measurement bytes; the generated `ConfiguredSpiDevice` drives CS as a
plain GPIO output around each transaction.

The driver resets through `PWR_MGMT_1` (0x6B = 0x80) and polls that register
until `DEVICE_RESET` clears, checks `WHO_AM_I` (0x75) for 0x71, then writes
`SMPLRT_DIV` 0x04, `CONFIG` 0x03, `ACCEL_CONFIG2` 0x03, `GYRO_CONFIG` 0x00 and
`ACCEL_CONFIG` 0x00, and finally `PWR_MGMT_1` = 0x01 (awake, X-gyro PLL).

`read()` is one transaction: address `0xBB` (0x3B with the read bit) and
fourteen bytes:

| Bytes | What |
|---|---|
| `0x3B..=0x40` | acceleration X, Y, Z, big-endian at 16384 LSB/g (±2 g) |
| `0x41`/`0x42` | temperature, big-endian at 333.87 LSB/°C with +21 °C at zero |
| `0x43..=0x48` | rotation X, Y, Z, big-endian at 131 LSB/dps (±250 dps) |

A flat, still part at 21 °C is `00 00 00 00 40 00 00 00 00 00 00 00 00 00`.

The attach line is in `build/mpu9250/emu.log` (the firmware console is
`build/mpu9250/sensor.stdout`):

```text
co-simulation: part imu (mpu9250) attached: wired CS=GPIO7 MISO=GPIO2 MOSI=GPIO3 SCK=GPIO6; drives no pin; is the SPI device on chip select GPIO 7
```

## Limits

- Happy path only: 4-wire SPI, one IMU on one bus, blocking runtime, one
  reading a second.
- **No magnetometer.** The AK8963 is a separate die on the auxiliary I2C bus;
  reaching it over SPI needs the chip's internal I2C master or a bypassed
  second bus, so no 9-axis sample is read and none of the `I2C_SLV*` or
  `EXT_SENS_DATA_*` registers are modelled.
- Range and filter changes are storage only: the part always encodes at ±2 g
  and ±250 dps and the driver always decodes with those scales, so a ±16 g
  configuration would read wrong against this part.
- The DMP, the FIFO, `INT_ENABLE`/`INT_STATUS` and the data-ready line are
  absent or storage; no self-test or offset calibration of any kind.
- `SMPLRT_DIV` does not change how often the part updates; the measurement is
  constant until an input changes it.
- SPI mode 0 at 1 MHz is declared; the emulator does not model polarity, phase
  or speed.
- No real hardware: the virtual part validates the protocol, the byte order and
  the scale arithmetic, not the sensor or a board. `AD0` and the SPI/I2C
  strapping are not modelled; the identity is always 0x71.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just mpu9250-clean
```
