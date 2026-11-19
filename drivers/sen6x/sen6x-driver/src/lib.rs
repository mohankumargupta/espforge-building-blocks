//! A Sensirion SEN6x air-quality sensor, over any `embedded-hal` I2C bus.
//!
//! The part is command-driven: it has no register pointer. [`Sen6x::new`]
//! sends the two-byte start-continuous-measurement command, and
//! [`Sen6x::read`] sends the two-byte read-measured-values command and then
//! reads 27 bytes. The 27 bytes are nine 16-bit big-endian words, each
//! followed by its CRC-8, in the SEN66 order:
//!
//! - PM1.0, PM2.5, PM4.0 and PM10.0, scaled by 10 (µg/m³),
//! - ambient temperature, scaled by 200 (°C),
//! - ambient relative humidity, scaled by 100 (%),
//! - VOC index and NOx index, scaled by 10,
//! - CO2 in ppm.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: the data-ready status is never polled, and
//! there is no stop, reset, product name, serial number, device status, fan
//! cleaning, temperature offset or CO2 recalibration. The first measurement's
//! ~1.1 s is not waited for: the emulator answers at once. See the fixture's
//! `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the command
// log in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's only address.
pub const ADDRESS: u8 = 0x6B;

/// Start continuous measurement in the SEN66 layout.
const START_CONTINUOUS_MEASUREMENT: [u8; 2] = [0x00, 0x21];
/// Read the measured values: nine words, each with its CRC byte.
const READ_MEASURED_VALUES: [u8; 2] = [0x03, 0x00];

/// Nine words of two bytes and a CRC each.
const MEASUREMENT_LEN: usize = 27;

/// PM words are tenths of a microgram per cubic metre.
const PM_LSB_UG_M3: f32 = 0.1;
/// Temperature words are two-hundredths of a degree Celsius.
const TEMPERATURE_LSB_C: f32 = 1.0 / 200.0;
/// Humidity words are hundredths of a percent.
const HUMIDITY_LSB_PERCENT: f32 = 0.01;
/// VOC and NOx index words are tenths.
const INDEX_LSB: f32 = 0.1;

/// One sample, in physical units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    /// PM1.0 mass concentration, µg/m³.
    pub pm1_0_ug_m3: f32,
    /// PM2.5 mass concentration, µg/m³.
    pub pm2_5_ug_m3: f32,
    /// PM4.0 mass concentration, µg/m³.
    pub pm4_0_ug_m3: f32,
    /// PM10.0 mass concentration, µg/m³.
    pub pm10_ug_m3: f32,
    /// Degrees Celsius.
    pub temperature_c: f32,
    /// Percent relative humidity.
    pub humidity_percent: f32,
    /// VOC index, 1 to 500.
    pub voc_index: f32,
    /// NOx index, 1 to 500.
    pub nox_index: f32,
    /// Parts per million of CO2.
    pub co2_ppm: u16,
}

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sen6xError<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// A data word failed its CRC check.
    Crc,
}

/// An SEN6x on an I2C bus.
pub struct Sen6x<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Sen6x<I2C> {
    /// Start continuous measurement.
    ///
    /// The command has no answer; a dead sensor surfaces on the first read.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Sen6xError<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.i2c
            .write(address, &START_CONTINUOUS_MEASUREMENT)
            .map_err(Sen6xError::Bus)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Read the measured values.
    ///
    /// The first measurement's ~1.1 s and the 20 ms execution time are not
    /// waited for: the emulator answers at once, and a real caller's next read
    /// is slow enough on its own.
    pub fn read(&mut self) -> Result<Reading, Sen6xError<I2C::Error>> {
        self.i2c
            .write(self.address, &READ_MEASURED_VALUES)
            .map_err(Sen6xError::Bus)?;
        let mut raw = [0u8; MEASUREMENT_LEN];
        self.i2c
            .read(self.address, &mut raw)
            .map_err(Sen6xError::Bus)?;
        decode(&raw).ok_or(Sen6xError::Crc)
    }
}

/// The nine measured words, each checked against its CRC byte.
fn decode(raw: &[u8; MEASUREMENT_LEN]) -> Option<Reading> {
    let pm1 = f32::from(word(raw, 0)?) * PM_LSB_UG_M3;
    let pm2_5 = f32::from(word(raw, 3)?) * PM_LSB_UG_M3;
    let pm4 = f32::from(word(raw, 6)?) * PM_LSB_UG_M3;
    let pm10 = f32::from(word(raw, 9)?) * PM_LSB_UG_M3;
    let temperature = i16::from_be_bytes([raw[12], raw[13]]) as f32 * TEMPERATURE_LSB_C;
    let humidity = i16::from_be_bytes([raw[15], raw[16]]) as f32 * HUMIDITY_LSB_PERCENT;
    let voc = i16::from_be_bytes([raw[18], raw[19]]) as f32 * INDEX_LSB;
    let nox = i16::from_be_bytes([raw[21], raw[22]]) as f32 * INDEX_LSB;
    let co2 = word(raw, 24)?;
    for offset in [12, 15, 18, 21] {
        if crc8([raw[offset], raw[offset + 1]]) != raw[offset + 2] {
            return None;
        }
    }
    Some(Reading {
        pm1_0_ug_m3: pm1,
        pm2_5_ug_m3: pm2_5,
        pm4_0_ug_m3: pm4,
        pm10_ug_m3: pm10,
        temperature_c: temperature,
        humidity_percent: humidity,
        voc_index: voc,
        nox_index: nox,
        co2_ppm: co2,
    })
}

/// One two-byte word and its CRC byte.
fn word(raw: &[u8], offset: usize) -> Option<u16> {
    if crc8([raw[offset], raw[offset + 1]]) != raw[offset + 2] {
        return None;
    }
    Some(u16::from_be_bytes([raw[offset], raw[offset + 1]]))
}

/// The Sensirion CRC-8: polynomial 0x31, initial value 0xFF, no reflection.
fn crc8(bytes: [u8; 2]) -> u8 {
    let mut crc = 0xFFu8;
    for byte in bytes {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to its two commands: a write selects what the next
    /// read answers.
    struct Fake {
        /// The measurement the part holds, already encoded.
        payload: [u8; MEASUREMENT_LEN],
        pending: Vec<u8>,
        writes: Vec<Vec<u8>>,
        reads: Vec<usize>,
    }

    impl Fake {
        /// PM1 8.0, PM2.5 12.5, PM4 15.0, PM10 20.0 µg/m³, 22.5 °C,
        /// 45 %RH, VOC 100, NOx 1 and 400 ppm of CO2.
        fn new() -> Self {
            Self {
                payload: [
                    0x00, 0x50, 0xFF, 0x00, 0x7D, 0x35, 0x00, 0x96, 0x1E, 0x00, 0xC8, 0x7F,
                    0x11, 0x94, 0xE6, 0x11, 0x94, 0xE6, 0x03, 0xE8, 0xD4, 0x00, 0x0A, 0x5A,
                    0x01, 0x90, 0x4C,
                ],
                pending: Vec::new(),
                writes: Vec::new(),
                reads: Vec::new(),
            }
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
                        self.writes.push(bytes.to_vec());
                        if *bytes == READ_MEASURED_VALUES.as_slice() {
                            self.pending = self.payload.to_vec();
                        }
                    }
                    Operation::Read(read) => {
                        self.reads.push(read.len());
                        let n = read.len().min(self.pending.len());
                        let data: Vec<u8> = self.pending.drain(..n).collect();
                        read[..n].copy_from_slice(&data);
                        for byte in &mut read[n..] {
                            *byte = 0xFF;
                        }
                    }
                }
            }
            Ok(())
        }
    }

    fn sensor() -> Sen6x<Fake> {
        Sen6x::new(Fake::new(), ADDRESS).expect("an SEN6x")
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.01
    }

    #[test]
    fn it_starts_continuous_measurement() {
        let sensor = sensor();
        assert_eq!(sensor.address(), 0x6B);
        assert_eq!(
            sensor.i2c.writes,
            [Vec::from(START_CONTINUOUS_MEASUREMENT)],
            "one start command, no other setup"
        );
    }

    #[test]
    fn a_read_returns_all_nine_fields() {
        let mut sensor = sensor();
        let reading = sensor.read().unwrap();
        assert!(close(reading.pm1_0_ug_m3, 8.0), "{reading:?}");
        assert!(close(reading.pm2_5_ug_m3, 12.5), "{reading:?}");
        assert!(close(reading.pm4_0_ug_m3, 15.0), "{reading:?}");
        assert!(close(reading.pm10_ug_m3, 20.0), "{reading:?}");
        assert!(close(reading.temperature_c, 22.5), "{reading:?}");
        assert!(close(reading.humidity_percent, 45.0), "{reading:?}");
        assert!(close(reading.voc_index, 100.0), "{reading:?}");
        assert!(close(reading.nox_index, 1.0), "{reading:?}");
        assert_eq!(reading.co2_ppm, 400);
        assert_eq!(
            sensor.i2c.writes.last(),
            Some(&Vec::from(READ_MEASURED_VALUES)),
            "one read-measured-values command"
        );
        assert_eq!(
            sensor.i2c.reads.last(),
            Some(&MEASUREMENT_LEN),
            "27 bytes back, nine words and their CRCs"
        );
    }

    #[test]
    fn a_bad_crc_is_refused() {
        let mut sensor = sensor();
        sensor.i2c.payload[2] ^= 0xFF;
        assert_eq!(sensor.read().err(), Some(Sen6xError::Crc));
    }

    #[test]
    fn the_temperature_is_signed() {
        let mut sensor = sensor();
        let word = (-1000i16).to_be_bytes();
        sensor.i2c.payload[12..15].copy_from_slice(&[word[0], word[1], crc8(word)]);
        assert!(close(sensor.read().unwrap().temperature_c, -5.0));
    }

    #[test]
    fn the_crc_matches_the_datasheet_example() {
        assert_eq!(crc8([0xBE, 0xEF]), 0x92);
    }

    #[test]
    fn the_scale_is_the_datasheets() {
        assert!(close(PM_LSB_UG_M3, 0.1));
        assert!(close(TEMPERATURE_LSB_C, 0.005));
        assert!(close(HUMIDITY_LSB_PERCENT, 0.01));
        assert!(close(INDEX_LSB, 0.1));
    }
}
