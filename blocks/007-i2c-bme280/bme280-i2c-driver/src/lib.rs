//! A Bosch BME280 temperature, humidity and pressure sensor, over any
//! `embedded-hal` I2C bus.
//!
//! The chip's I2C protocol is a register pointer write and, for a read, a
//! repeated start: [`Bme280I2c::read`] does the combined `write_read` in one
//! transaction. `ID` (0xD0) is 0x60 on a BME280. The eight data bytes at 0xF7
//! are pressure (three bytes), temperature (three) and humidity (two), which
//! [`Bme280I2c::read`] turns into °C, hPa and %RH with Bosch's compensation
//! formulas (datasheet §4.2.3).
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.

#![no_std]

use embedded_hal::i2c::I2c;

/// `ID`: the fixed identity register.
const ID: u8 = 0xD0;
/// The value a healthy BME280 answers with. A BMP280 answers 0x58.
pub const CHIP_ID: u8 = 0x60;
/// `ctrl_hum`, `config` and `ctrl_meas`: the three registers that wake the
/// chip up.
const CTRL_HUM: u8 = 0xF2;
const CONFIG: u8 = 0xF5;
const CTRL_MEAS: u8 = 0xF4;
/// `press_msb`: the first of the eight data bytes.
const PRESS_MSB: u8 = 0xF7;

/// `ctrl_hum`: humidity oversampling ×1.
const CTRL_HUM_X1: u8 = 0x01;
/// `config`: standby 0.5 ms, IIR filter off.
const CONFIG_DEFAULT: u8 = 0x00;
/// `ctrl_meas`: temperature and pressure oversampling ×1, normal mode.
const CTRL_MEAS_NORMAL: u8 = 0x27;

/// Why a transfer or identity check failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bme280Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The part answered, but not with the identity a BME280 has.
    WrongDevice(u8),
}

/// One sample. The pressure is in hectopascals because that is what weather
/// reports use; the chip itself reports pascals.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    pub temperature_c: f32,
    pub pressure_hpa: f32,
    pub humidity_percent: f32,
}

/// A BME280 on an I2C bus.
pub struct Bme280I2c<I2C> {
    i2c: I2C,
    address: u8,
    calibration: Calibration,
}

impl<I2C: I2c> Bme280I2c<I2C> {
    /// Check the identity register, read the calibration block, and put the
    /// chip in normal mode.
    ///
    /// A part that answers with another id is `Bme280Error::WrongDevice`, so
    /// the boot log names the device instead of reporting bogus readings.
    /// Nothing here waits for the first conversion: the caller's first
    /// `read()` does, by taking seconds to come round in the app.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Bme280Error<I2C::Error>> {
        let mut this = Self {
            i2c,
            address,
            calibration: Calibration::default(),
        };
        let id = this.device_id()?;
        if id != CHIP_ID {
            return Err(Bme280Error::WrongDevice(id));
        }
        let mut low = [0u8; 26];
        this.read_registers(0x88, &mut low)?;
        let mut high = [0u8; 7];
        this.read_registers(0xE1, &mut high)?;
        this.calibration = Calibration::from_bytes(&low, &high);

        // ctrl_hum only takes effect when ctrl_meas is written after it.
        this.write_register(CTRL_HUM, CTRL_HUM_X1)?;
        this.write_register(CONFIG, CONFIG_DEFAULT)?;
        this.write_register(CTRL_MEAS, CTRL_MEAS_NORMAL)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// The identity register: `0x60` on a BME280.
    pub fn device_id(&mut self) -> Result<u8, Bme280Error<I2C::Error>> {
        let mut id = [0u8; 1];
        self.read_registers(ID, &mut id)?;
        Ok(id[0])
    }

    /// One sample: eight data bytes, then the compensation formulas.
    pub fn read(&mut self) -> Result<Reading, Bme280Error<I2C::Error>> {
        let mut raw = [0u8; 8];
        self.read_registers(PRESS_MSB, &mut raw)?;
        Ok(self.calibration.compensate(&raw))
    }

    /// A register read: the pointer byte, then the answer, in one combined
    /// transaction. The pointer auto-increments across the whole read.
    fn read_registers(&mut self, start: u8, buf: &mut [u8]) -> Result<(), Bme280Error<I2C::Error>> {
        self.i2c
            .write_read(self.address, &[start], buf)
            .map_err(Bme280Error::Bus)
    }

    /// A register write: the pointer, then one value.
    fn write_register(&mut self, register: u8, value: u8) -> Result<(), Bme280Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[register, value])
            .map_err(Bme280Error::Bus)
    }
}

/// The chip's one-time-programmed calibration constants.
#[derive(Clone, Copy, Default)]
struct Calibration {
    t1: f64,
    t2: f64,
    t3: f64,
    p1: f64,
    p2: f64,
    p3: f64,
    p4: f64,
    p5: f64,
    p6: f64,
    p7: f64,
    p8: f64,
    p9: f64,
    h1: f64,
    h2: f64,
    h3: f64,
    h4: f64,
    h5: f64,
    h6: f64,
}

impl Calibration {
    /// `low` is 0x88..=0xA1 (T1..T3, P1..P9, H1) and `high` is 0xE1..=0xE7
    /// (H2..H6). The H4/H5 nibbles are 12-bit two's complement.
    fn from_bytes(low: &[u8; 26], high: &[u8; 7]) -> Self {
        let h4 = ((high[3] as i16) << 4) | ((high[4] & 0x0F) as i16);
        let h5 = ((high[5] as i16) << 4) | ((high[4] >> 4) as i16);
        Self {
            t1: u16::from_le_bytes([low[0], low[1]]) as f64,
            t2: i16::from_le_bytes([low[2], low[3]]) as f64,
            t3: i16::from_le_bytes([low[4], low[5]]) as f64,
            p1: u16::from_le_bytes([low[6], low[7]]) as f64,
            p2: i16::from_le_bytes([low[8], low[9]]) as f64,
            p3: i16::from_le_bytes([low[10], low[11]]) as f64,
            p4: i16::from_le_bytes([low[12], low[13]]) as f64,
            p5: i16::from_le_bytes([low[14], low[15]]) as f64,
            p6: i16::from_le_bytes([low[16], low[17]]) as f64,
            p7: i16::from_le_bytes([low[18], low[19]]) as f64,
            p8: i16::from_le_bytes([low[20], low[21]]) as f64,
            p9: i16::from_le_bytes([low[22], low[23]]) as f64,
            h1: low[25] as f64,
            h2: i16::from_le_bytes([high[0], high[1]]) as f64,
            h3: high[2] as f64,
            h4: ((h4 << 4) >> 4) as f64,
            h5: ((h5 << 4) >> 4) as f64,
            h6: high[6] as i8 as f64,
        }
    }

    /// The eight data bytes as °C, hPa and %RH.
    fn compensate(&self, raw: &[u8; 8]) -> Reading {
        let adc_p = ((raw[0] as u32) << 12 | (raw[1] as u32) << 4 | (raw[2] as u32) >> 4) as f64;
        let adc_t = ((raw[3] as u32) << 12 | (raw[4] as u32) << 4 | (raw[5] as u32) >> 4) as f64;
        let adc_h = (((raw[6] as u32) << 8) | raw[7] as u32) as f64;

        let (temperature_c, t_fine) = self.temperature(adc_t);
        Reading {
            temperature_c: temperature_c as f32,
            pressure_hpa: (self.pressure_pa(adc_p, t_fine) / 100.0) as f32,
            humidity_percent: self.humidity_percent(adc_h, t_fine) as f32,
        }
    }

    /// Bosch's temperature compensation (BME280 datasheet §4.2.3), returning
    /// °C and the `t_fine` the other two formulas need.
    fn temperature(&self, adc: f64) -> (f64, f64) {
        let var1 = (adc / 16_384.0 - self.t1 / 1_024.0) * self.t2;
        let centered = adc / 131_072.0 - self.t1 / 8_192.0;
        let var2 = centered * centered * self.t3;
        let t_fine = var1 + var2;
        (t_fine / 5_120.0, t_fine)
    }

    /// Bosch's pressure compensation, in pascals.
    fn pressure_pa(&self, adc: f64, t_fine: f64) -> f64 {
        let mut var1 = t_fine / 2.0 - 64_000.0;
        let mut var2 = var1 * var1 * self.p6 / 32_768.0;
        var2 += var1 * self.p5 * 2.0;
        var2 = var2 / 4.0 + self.p4 * 65_536.0;
        var1 = (self.p3 * var1 * var1 / 524_288.0 + self.p2 * var1) / 524_288.0;
        var1 = (1.0 + var1 / 32_768.0) * self.p1;
        if var1 == 0.0 {
            return 0.0;
        }
        let mut p = 1_048_576.0 - adc;
        p = (p - var2 / 4_096.0) * 6_250.0 / var1;
        let var1 = self.p9 * p * p / 2_147_483_648.0;
        let var2 = p * self.p8 / 32_768.0;
        p + (var1 + var2 + self.p7) / 16.0
    }

    /// Bosch's humidity compensation, in %RH, held to 0..=100.
    fn humidity_percent(&self, adc: f64, t_fine: f64) -> f64 {
        let temp = t_fine - 76_800.0;
        let mut h = adc - (self.h4 * 64.0 + self.h5 / 16_384.0 * temp);
        h *= self.h2 / 65_536.0
            * (1.0
                + self.h6 / 67_108_864.0
                    * temp
                    * (1.0 + self.h3 / 67_108_864.0 * temp));
        h *= 1.0 - self.h1 * h / 524_288.0;
        h.clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};

    /// The emulator's part, reduced to a register file: a write sets the
    /// pointer, a combined read answers from it and auto-increments.
    struct Fake {
        regs: [u8; 256],
        id: u8,
        pointer: u8,
    }

    impl Fake {
        /// The part's calibration and the room conditions the virtual part
        /// reports: 22.5 °C, 1013.25 hPa, 45 %RH.
        fn room(id: u8) -> Self {
            let mut regs = [0u8; 256];
            put_u16(&mut regs, 0x88, 27_504);
            put_u16(&mut regs, 0x8E, 36_477);
            for (reg, value) in [
                (0x8A, 26_435i16),
                (0x8C, -1_000),
                (0x90, -10_685),
                (0x92, 3_024),
                (0x94, 2_855),
                (0x96, 140),
                (0x98, -7),
                (0x9A, 15_500),
                (0x9C, -14_600),
                (0x9E, 6_000),
                (0xE1, 370),
            ] {
                put_i16(&mut regs, reg, value);
            }
            regs[0xA1] = 75;
            regs[0xE3] = 0;
            regs[0xE4] = 0x13;
            regs[0xE5] = 0x29;
            regs[0xE6] = 0x03;
            regs[0xE7] = 30;
            regs[0xF7..=0xFE].copy_from_slice(&[
                0x63, 0xD6, 0x10, 0x7C, 0xEA, 0x00, 0x6D, 0x84,
            ]);
            Self { regs, id, pointer: 0 }
        }
    }

    fn put_u16(regs: &mut [u8; 256], reg: usize, value: u16) {
        regs[reg..reg + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn put_i16(regs: &mut [u8; 256], reg: usize, value: i16) {
        regs[reg..reg + 2].copy_from_slice(&value.to_le_bytes());
    }

    impl ErrorType for Fake {
        type Error = Infallible;
    }

    impl Fake {
        /// A pointer byte with an optional value: the chip's register write.
        fn write_bytes(&mut self, bytes: &[u8]) {
            if let Some(&register) = bytes.first() {
                self.pointer = register;
                if let Some(&value) = bytes.get(1) {
                    self.regs[register as usize] = value;
                }
            }
        }

        /// Answer from the pointer, auto-incrementing it.
        fn read_bytes(&mut self, read: &mut [u8]) {
            for byte in read.iter_mut() {
                *byte = if self.pointer == ID {
                    self.id
                } else {
                    self.regs[self.pointer as usize]
                };
                self.pointer = self.pointer.wrapping_add(1);
            }
        }
    }

    impl I2c for Fake {
        fn transaction(
            &mut self,
            _address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            for operation in operations {
                match operation {
                    Operation::Write(bytes) => self.write_bytes(bytes),
                    Operation::Read(read) => self.read_bytes(read),
                }
            }
            Ok(())
        }
    }

    fn sensor(id: u8) -> Result<Bme280I2c<Fake>, Bme280Error<Infallible>> {
        Bme280I2c::new(Fake::room(id), 0x76)
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.05
    }

    #[test]
    fn it_accepts_a_bme280_and_configures_normal_mode() {
        let sensor = sensor(CHIP_ID).expect("a BME280");
        assert_eq!(sensor.i2c.regs[0xF2 as usize], 0x01, "ctrl_hum");
        assert_eq!(sensor.i2c.regs[0xF5 as usize], 0x00, "config");
        assert_eq!(sensor.i2c.regs[0xF4 as usize], 0x27, "ctrl_meas");
        assert_eq!(sensor.address(), 0x76);
    }

    #[test]
    fn it_refuses_another_identity() {
        assert_eq!(
            sensor(0x58).err(),
            Some(Bme280Error::WrongDevice(0x58)),
            "a BMP280"
        );
    }

    #[test]
    fn room_conditions_decode_as_the_part_reports_them() {
        let mut sensor = sensor(CHIP_ID).unwrap();
        let reading = sensor.read().unwrap();
        assert!(close(reading.temperature_c, 22.5), "{:?}", reading);
        assert!(close(reading.pressure_hpa, 1013.25), "{:?}", reading);
        assert!(close(reading.humidity_percent, 45.0), "{:?}", reading);
    }

    #[test]
    fn the_data_bytes_are_pressure_then_temperature_then_humidity() {
        // 0x63D61 / 0x7CEA0 / 0x6D84 are the counts the part encodes for room
        // conditions; reading the raw bytes directly must give the same.
        let mut fake = Fake::room(CHIP_ID);
        let mut raw = [0u8; 8];
        let calibration = Calibration::from_bytes(
            fake.regs[0x88..=0xA1].try_into().unwrap(),
            fake.regs[0xE1..=0xE7].try_into().unwrap(),
        );
        fake.write_bytes(&[PRESS_MSB]);
        fake.read_bytes(&mut raw);
        assert_eq!(raw, [0x63, 0xD6, 0x10, 0x7C, 0xEA, 0x00, 0x6D, 0x84]);
        let reading = calibration.compensate(&raw);
        assert!(close(reading.temperature_c, 22.5), "{reading:?}");
    }
}
