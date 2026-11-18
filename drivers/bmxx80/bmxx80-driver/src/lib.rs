//! A Bosch BME280 temperature, pressure and humidity sensor, over any
//! `embedded-hal` I2C bus.
//!
//! [`Bme280::new`] checks the chip id, soft-resets the part, reads its factory
//! calibration and starts normal-mode measurements with 1x oversampling on all
//! three channels. [`Bme280::read`] then reads the eight data registers and
//! applies the datasheet's integer compensation:
//!
//! - temperature in degrees Celsius,
//! - pressure in hectopascals (the compensation returns pascals),
//! - humidity in percent relative humidity.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: I2C only (no SPI), one oversampling setting,
//! normal mode, and the 1.5 ms measurement time is the caller's. See the
//! fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep a register
// file in an array.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The BME280's address with SDO low; SDO high is 0x77.
pub const ADDRESS: u8 = 0x76;

/// The value the BME280 answers at the chip id register.
pub const CHIP_ID_BME280: u8 = 0x60;

const CHIP_ID: u8 = 0xD0;
const RESET: u8 = 0xE0;
const RESET_COMMAND: u8 = 0xB6;
const CTRL_HUM: u8 = 0xF2;
const CTRL_MEAS: u8 = 0xF4;
const CALIBRATION_START: u8 = 0x88;
const CALIBRATION_LEN: usize = 26;
const CALIBRATION2_START: u8 = 0xE1;
const CALIBRATION2_LEN: usize = 7;
const DATA_START: u8 = 0xF7;

/// `0b001` in the low bits: humidity oversampling x1, then temperature and
/// pressure x1 with normal mode in `0b11`.
const OVERSAMPLING_X1: u8 = 0b001;
const NORMAL_MODE: u8 = 0b11;

/// The BME280's factory calibration, as the compensation formulas use it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Calibration {
    t1: i32,
    t2: i32,
    t3: i32,
    p1: i32,
    p2: i32,
    p3: i32,
    p4: i32,
    p5: i32,
    p6: i32,
    p7: i32,
    p8: i32,
    p9: i32,
    h1: i32,
    h2: i32,
    h3: i32,
    h4: i32,
    h5: i32,
    h6: i32,
}

/// One compensated reading.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Measurement {
    /// Degrees Celsius.
    pub temperature: f32,
    /// Hectopascals.
    pub pressure: f32,
    /// Percent relative humidity.
    pub humidity: f32,
}

/// Why the part could not be used, or a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bmxx80Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The chip id was not a BME280's.
    WrongChipId(u8),
}

/// A BME280 on an I2C bus.
pub struct Bme280<I2C> {
    i2c: I2C,
    address: u8,
    calibration: Calibration,
}

impl<I2C: I2c> Bme280<I2C> {
    /// Check the part, read its calibration and start measuring.
    ///
    /// The reset and the first measurement are not waited for: the emulator
    /// completes both at once.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Bmxx80Error<I2C::Error>> {
        let mut this = Self {
            i2c,
            address,
            calibration: Calibration::default(),
        };

        let mut chip_id = [0u8; 1];
        this.i2c
            .write_read(address, &[CHIP_ID], &mut chip_id)
            .map_err(Bmxx80Error::Bus)?;
        if chip_id[0] != CHIP_ID_BME280 {
            return Err(Bmxx80Error::WrongChipId(chip_id[0]));
        }

        this.write_register(RESET, RESET_COMMAND)?;
        this.calibration = this.read_calibration()?;

        // Humidity must be set before the measurement control, for its
        // oversampling to take effect.
        this.write_register(CTRL_HUM, OVERSAMPLING_X1)?;
        this.write_register(
            CTRL_MEAS,
            (OVERSAMPLING_X1 << 5) | (OVERSAMPLING_X1 << 2) | NORMAL_MODE,
        )?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Read the data registers and compensate them.
    pub fn read(&mut self) -> Result<Measurement, Bmxx80Error<I2C::Error>> {
        let mut data = [0u8; 8];
        self.i2c
            .write_read(self.address, &[DATA_START], &mut data)
            .map_err(Bmxx80Error::Bus)?;

        // The 20-bit ADC values are stored left-aligned in three bytes.
        let adc_p = ((u32::from(data[0]) << 12)
            | (u32::from(data[1]) << 4)
            | (u32::from(data[2]) >> 4)) as i32;
        let adc_t = ((u32::from(data[3]) << 12)
            | (u32::from(data[4]) << 4)
            | (u32::from(data[5]) >> 4)) as i32;
        let adc_h = (u16::from(data[6]) << 8 | u16::from(data[7])) as i32;

        let (t_fine, temperature) = self.compensate_temperature(adc_t);
        let pressure = self.compensate_pressure(adc_p, t_fine);
        let humidity = self.compensate_humidity(adc_h, t_fine);

        Ok(Measurement {
            // The compensation yields 0.01 degrees.
            temperature: temperature as f32 / 100.0,
            // ... pascals, and the datasheet's users want hectopascals.
            pressure: pressure as f32 / 100.0,
            // ... %RH * 1024.
            humidity: humidity as f32 / 1024.0,
        })
    }

    /// One register write: the register address, then its byte.
    fn write_register(&mut self, register: u8, value: u8) -> Result<(), Bmxx80Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[register, value])
            .map_err(Bmxx80Error::Bus)
    }

    /// Read both halves of the factory calibration.
    fn read_calibration(&mut self) -> Result<Calibration, Bmxx80Error<I2C::Error>> {
        let mut raw = [0u8; CALIBRATION_LEN];
        self.i2c
            .write_read(self.address, &[CALIBRATION_START], &mut raw)
            .map_err(Bmxx80Error::Bus)?;
        let mut raw2 = [0u8; CALIBRATION2_LEN];
        self.i2c
            .write_read(self.address, &[CALIBRATION2_START], &mut raw2)
            .map_err(Bmxx80Error::Bus)?;

        Ok(Calibration {
            t1: i32::from(u16::from_le_bytes([raw[0], raw[1]])),
            t2: i32::from(i16::from_le_bytes([raw[2], raw[3]])),
            t3: i32::from(i16::from_le_bytes([raw[4], raw[5]])),
            p1: i32::from(u16::from_le_bytes([raw[6], raw[7]])),
            p2: i32::from(i16::from_le_bytes([raw[8], raw[9]])),
            p3: i32::from(i16::from_le_bytes([raw[10], raw[11]])),
            p4: i32::from(i16::from_le_bytes([raw[12], raw[13]])),
            p5: i32::from(i16::from_le_bytes([raw[14], raw[15]])),
            p6: i32::from(i16::from_le_bytes([raw[16], raw[17]])),
            p7: i32::from(i16::from_le_bytes([raw[18], raw[19]])),
            p8: i32::from(i16::from_le_bytes([raw[20], raw[21]])),
            p9: i32::from(i16::from_le_bytes([raw[22], raw[23]])),
            h1: i32::from(raw[24]),
            h2: i32::from(i16::from_le_bytes([raw2[0], raw2[1]])),
            h3: i32::from(raw2[2]),
            // H4 and H5 share the nibbles of 0xE5.
            h4: sign_extend_12((i32::from(raw2[3]) << 4) | i32::from(raw2[4] & 0x0F)),
            h5: sign_extend_12((i32::from(raw2[5]) << 4) | i32::from(raw2[4] >> 4)),
            h6: i32::from(raw2[6] as i8),
        })
    }

    /// The datasheet's temperature compensation; also yields `t_fine`, which
    /// pressure and humidity need.
    fn compensate_temperature(&self, adc_t: i32) -> (i32, i32) {
        let c = &self.calibration;
        let var1 = (((adc_t >> 3) - (c.t1 << 1)) * c.t2) >> 11;
        let centred = (adc_t >> 4) - c.t1;
        let var2 = (((centred * centred) >> 12) * c.t3) >> 14;
        let t_fine = var1 + var2;
        (t_fine, (t_fine * 5 + 128) >> 8)
    }

    /// The datasheet's pressure compensation, in pascals.
    fn compensate_pressure(&self, adc_p: i32, t_fine: i32) -> u32 {
        let c = &self.calibration;
        let mut var1 = (t_fine >> 1) - 64_000;
        let mut var2 = (((var1 >> 2) * (var1 >> 2)) >> 11) * c.p6;
        var2 += (var1 * c.p5) << 1;
        var2 = (var2 >> 2) + (c.p4 << 16);
        var1 = (((c.p3 * (((var1 >> 2) * (var1 >> 2)) >> 13)) >> 3) + ((c.p2 * var1) >> 1)) >> 18;
        var1 = ((32_768 + var1) * c.p1) >> 15;
        if var1 == 0 {
            return 0;
        }

        // The datasheet widens this product to unsigned 32 bits; 64 bits keep
        // it exact without relying on wraparound.
        let p = (i64::from(1_048_576 - adc_p) - i64::from(var2 >> 12)) * 3_125;
        let p = if p < 0x8000_0000 {
            (p * 2) / i64::from(var1)
        } else {
            (p / i64::from(var1)) * 2
        } as i32;

        let var1 = (c.p9 * (((p >> 3) * (p >> 3)) >> 13)) >> 12;
        let var2 = ((p >> 2) * c.p8) >> 13;
        let p = p + ((var1 + var2 + c.p7) >> 4);
        p.max(0) as u32
    }

    /// The datasheet's humidity compensation, in %RH * 1024.
    ///
    /// The datasheet's C keeps these intermediates in 32 bits; 64 bits keep
    /// them exact for any calibration.
    fn compensate_humidity(&self, adc_h: i32, t_fine: i32) -> u32 {
        let c = &self.calibration;
        let mut v = i64::from(t_fine) - 76_800;
        // The temperature-dependent factor, all from the original `v`.
        let fine = ((v * i64::from(c.h6)) >> 10) * (((v * i64::from(c.h3)) >> 11) + 32_768);
        v = ((((i64::from(adc_h) << 14) - (i64::from(c.h4) << 20) - i64::from(c.h5) * v)
            + 16_384)
            >> 15)
            * ((((fine >> 10) + 2_097_152) * i64::from(c.h2) + 8_192) >> 14);
        v -= ((((v >> 15) * (v >> 15)) >> 7) * i64::from(c.h1)) >> 4;
        (v.clamp(0, 419_430_400) >> 12) as u32
    }
}

/// Sign-extend the 12-bit humidity calibration values.
fn sign_extend_12(value: i32) -> i32 {
    if value & 0x800 != 0 {
        value - 0x1000
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to a 256-byte register file.
    struct Fake {
        registers: [u8; 256],
        pointer: u8,
        writes: Vec<Vec<u8>>,
    }

    impl Fake {
        fn new() -> Self {
            let mut fake = Self {
                registers: [0u8; 256],
                pointer: 0,
                writes: Vec::new(),
            };
            fake.registers[usize::from(CHIP_ID)] = CHIP_ID_BME280;
            // The same simplified calibration the fixture's virtual part
            // publishes: temperature x1 gain, pressure in pascals as
            // 1048576 - adc, and humidity in %RH * 1024.
            let calibration: [(usize, u8); 10] = [
                (0x88, 0),
                (0x89, 0), // T1 = 0
                (0x8A, 0x00),
                (0x8B, 0x10), // T2 = 4096
                (0x8E, 0x6A),
                (0x8F, 0x18), // P1 = 6250
                (0xE1, 0x00),
                (0xE2, 0x40), // H2 = 16384
                (0xE3, 0x00),
                (0xE7, 0x00),
            ];
            for (register, value) in calibration {
                fake.registers[register] = value;
            }
            fake.set_measurement(512_000, 947_251, 179);
            fake
        }

        /// Store one raw ADC value and its derived data registers.
        fn set_measurement(&mut self, adc_t: u32, adc_p: u32, adc_h: u16) {
            let p = adc_p << 4;
            let t = adc_t << 4;
            self.registers[usize::from(DATA_START)] = (p >> 16) as u8;
            self.registers[usize::from(DATA_START) + 1] = (p >> 8) as u8;
            self.registers[usize::from(DATA_START) + 2] = p as u8;
            self.registers[usize::from(DATA_START) + 3] = (t >> 16) as u8;
            self.registers[usize::from(DATA_START) + 4] = (t >> 8) as u8;
            self.registers[usize::from(DATA_START) + 5] = t as u8;
            self.registers[usize::from(DATA_START) + 6] = (adc_h >> 8) as u8;
            self.registers[usize::from(DATA_START) + 7] = adc_h as u8;
        }
    }

    impl ErrorType for Fake {
        type Error = Infallible;
    }

    impl I2c for Fake {
        fn transaction(
            &mut self,
            _address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            for operation in operations {
                match operation {
                    Operation::Write(bytes) => {
                        if let Some(&first) = bytes.first() {
                            self.pointer = first;
                            self.writes.push(bytes.to_vec());
                            if bytes.len() > 1 {
                                self.registers[usize::from(first)] = bytes[1];
                            }
                        }
                    }
                    Operation::Read(read) => {
                        for (offset, byte) in read.iter_mut().enumerate() {
                            let register = self.pointer.wrapping_add(offset as u8);
                            *byte = self.registers[usize::from(register)];
                        }
                    }
                }
            }
            Ok(())
        }
    }

    fn sensor() -> Bme280<Fake> {
        Bme280::new(Fake::new(), ADDRESS).expect("a BME280")
    }

    #[test]
    fn it_checks_the_chip_id_resets_and_configures() {
        let sensor = sensor();
        assert_eq!(sensor.address(), ADDRESS);
        assert_eq!(sensor.address(), 0x76);
        let writes: Vec<Vec<u8>> = sensor
            .i2c
            .writes
            .iter()
            .filter(|write| write.len() > 1)
            .cloned()
            .collect();
        assert_eq!(
            writes,
            [
                Vec::from([RESET, RESET_COMMAND]),
                Vec::from([CTRL_HUM, OVERSAMPLING_X1]),
                Vec::from([CTRL_MEAS, 0x27]),
            ],
            "reset, humidity oversampling, then the measurement control"
        );
    }

    #[test]
    fn a_measurement_is_compensated_to_physical_units() {
        let mut sensor = sensor();
        assert_eq!(
            sensor.read().unwrap(),
            Measurement {
                temperature: 25.0,
                pressure: 1013.25,
                humidity: 45.0,
            }
        );
    }

    #[test]
    fn the_temperature_and_humidity_follow_the_raw_values() {
        let mut sensor = sensor();
        // 30.00 degrees is t_fine 153600, so adc 614400; 60 %RH is adc 239.
        sensor.i2c.set_measurement(614_400, 947_251, 239);
        let reading = sensor.read().unwrap();
        assert_eq!(reading.temperature, 30.0);
        assert_eq!(reading.humidity, 60.0);
        assert_eq!(reading.pressure, 1013.25);
    }

    #[test]
    fn a_wrong_chip_id_is_reported() {
        let mut fake = Fake::new();
        fake.registers[usize::from(CHIP_ID)] = 0x58; // a BMP280
        assert_eq!(
            Bme280::new(fake, ADDRESS).err(),
            Some(Bmxx80Error::WrongChipId(0x58))
        );
    }

    #[test]
    fn the_compensation_ignores_a_partial_calibration() {
        // With T2 = 0 the temperature compensation returns the datasheet's
        // 0.01-degree offset only, so a broken calibration cannot panic.
        let mut fake = Fake::new();
        fake.registers[0x8A] = 0;
        fake.registers[0x8B] = 0;
        let mut sensor = Bme280::new(fake, ADDRESS).unwrap();
        let reading = sensor.read().unwrap();
        assert!(reading.temperature.is_finite());
        assert!(reading.pressure.is_finite());
        assert!(reading.humidity.is_finite());
    }
}
