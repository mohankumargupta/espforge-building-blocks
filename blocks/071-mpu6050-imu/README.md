# 071 — 6-Axis Inertial Measurement Unit (MPU-6050)

**Status:** 🟡 partly possible — the I²C bus and the register burst are real; the scaling and filtering are in `app.rs`
**ESPHome lego:** `sensor: platform: mpu6050`

## ESPHome

```yaml
sensor:
  - platform: mpu6050
    address: 0x68
    i2c_id: i2c0
    accel_x:
      name: "Acceleration X"
    accel_y:
      name: "Acceleration Y"
    accel_z:
      name: "Acceleration Z"
    gyro_x:
      name: "Angular Velocity X"
    update_interval: 100ms
```

The MPU-6050 lays its six measurements out contiguously from register 0x3B: 14
bytes of big-endian signed 16-bit acceleration, temperature and rotation. One
burst read keeps the axes consistent with each other; reading them separately
mixes samples from different moments.

## espforge

`mpu6050-imu.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0`, `address: 0x68` | `<i2c id="i2c0" sda="4" scl="5" frequency="400kHz" />`, `ADDRESS` |
| 14-byte burst | one `write_read(0x3B, &mut raw[14])` |
| `accel_x/y/z`, `gyro_x` | big-endian `i16` pairs, `accel / 16384 g`, `gyro / 131 dps` |
| `update_interval: 100ms` | `delay_ms(100)` |
| `platform: mpu6050` | none: no driver |
| DLPF configuration | register 0x1A, written once in `setup` |
| `name:` | none; the readings are logged |

The raw registers are real, so the block is 🟡: the conversion factors are the
datasheet's, and a 14-byte read at 400 kHz fits the 100 ms period comfortably.
The missing driver would add the self-test, the DLPF and the axis-remap
handling.

### What is missing

An `<mpu6050>` device that owns the configuration registers and publishes six
sensors, plus the filter pipeline those readings want.
