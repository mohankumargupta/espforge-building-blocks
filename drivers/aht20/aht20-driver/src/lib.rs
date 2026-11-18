//! An ASAIR AHT20 temperature and relative humidity sensor, over any
//! `embedded-hal` I2C bus.
//!
//! The chip has no register pointer: a command is a plain write of its bytes,
//! and a read answers from whatever the last command prepared. The status
//! byte (command `0x71`, and the first byte of a measurement) carries the
//! calibrated bit at 3 and the busy bit at 7. [`Aht20::new`] reads it once and
//! sends the initialization command only if the part says it is not
//! calibrated; [`Aht20::read`] triggers one measurement (`0xAC, 0x33, 0x00`)
//! and decodes the seven bytes that follow: status, 20 bits of humidity,
//! 20 bits of temperature, then the CRC.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the command
// log and the pending answer in vectors.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The AHT20's only I2C address.
pub const ADDRESS: u8 = 0x38;

/// Status read: `0x71` is the part's status "sub address".
const READ_STATUS: [u8; 1] = [0x71];
/// Initialization: `0xBE`, then the two parameter bytes.
const INITIALIZE: [u8; 3] = [0xBE, 0x08, 0x00];
/// Trigger one measurement: `0xAC`, then the two parameter bytes.
const MEASURE: [u8; 3] = [0xAC, 0x33, 0x00];

/// Status bit 7: a measurement is still running.
const BUSY: u8 = 0x80;
/// Status bit 3: the part is calibrated.
const CALIBRATED: u8 = 0x08;

/// Status, 20 bits of humidity, 20 bits of temperature, CRC.
const MEASUREMENT_LEN: usize = 7;
/// A 20-bit count spans 2^20.
const COUNTS: f32 = 1_048_576.0;

/// One sample.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    pub temperature_c: f32,
    pub humidity_percent: f32,
}

/// Why a transfer, the status check or a measurement failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aht20Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The part answered, but says it is not calibrated even after the
    /// initialization command.
    NotCalibrated,
    /// The part says a measurement is still running.
    Busy,
}

/// An AHT20 on an I2C bus.
pub struct Aht20<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Aht20<I2C> {
    /// Read the status byte, initialize the part if it is not calibrated, and
    /// leave it ready for [`Aht20::read`].
    ///
    /// The initialization's 10 ms is not waited for: the status is read once
    /// more, which the emulator answers at once. A part that still says it is
    /// not calibrated is `Aht20Error::NotCalibrated`.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Aht20Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        if !this.is_calibrated()? {
            this.i2c
                .write(this.address, &INITIALIZE)
                .map_err(Aht20Error::Bus)?;
            if !this.is_calibrated()? {
                return Err(Aht20Error::NotCalibrated);
            }
        }
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// The status byte: bit 7 busy, bit 3 calibrated.
    pub fn status(&mut self) -> Result<u8, Aht20Error<I2C::Error>> {
        let mut status = [0u8; 1];
        self.i2c
            .write_read(self.address, &READ_STATUS, &mut status)
            .map_err(Aht20Error::Bus)?;
        Ok(status[0])
    }

    /// Trigger one measurement and decode it.
    ///
    /// The trigger is one write and the sample one read, as the datasheet
    /// describes. The measurement's 80 ms is not waited for: a part that is
    /// still busy is `Aht20Error::Busy`, and the emulator completes at once.
    pub fn read(&mut self) -> Result<Reading, Aht20Error<I2C::Error>> {
        self.i2c
            .write(self.address, &MEASURE)
            .map_err(Aht20Error::Bus)?;
        let mut raw = [0u8; MEASUREMENT_LEN];
        self.i2c
            .read(self.address, &mut raw)
            .map_err(Aht20Error::Bus)?;
        if raw[0] & BUSY != 0 {
            return Err(Aht20Error::Busy);
        }
        Ok(decode(&raw))
    }

    /// Whether the status byte says the part is calibrated.
    fn is_calibrated(&mut self) -> Result<bool, Aht20Error<I2C::Error>> {
        Ok(self.status()? & CALIBRATED != 0)
    }
}

/// The seven measurement bytes as °C and %RH.
///
/// Bits 39:20 are the humidity count and bits 19:0 the temperature count, both
/// 20-bit big-endian; the first byte is the status and the last the CRC.
fn decode(raw: &[u8; MEASUREMENT_LEN]) -> Reading {
    let humidity = ((raw[1] as u32) << 12) | ((raw[2] as u32) << 4) | ((raw[3] as u32) >> 4);
    let temperature =
        (((raw[3] & 0x0F) as u32) << 16) | ((raw[4] as u32) << 8) | raw[5] as u32;
    Reading {
        temperature_c: temperature as f32 * 200.0 / COUNTS - 50.0,
        humidity_percent: humidity as f32 * 100.0 / COUNTS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The chip reduced to its three commands: a write prepares the answer,
    /// a read clocks it out.
    struct Fake {
        calibrated: bool,
        init_works: bool,
        payload: [u8; MEASUREMENT_LEN],
        pending: Vec<u8>,
        writes: Vec<Vec<u8>>,
        reads: Vec<usize>,
    }

    impl Fake {
        fn new(calibrated: bool) -> Self {
            Self {
                calibrated,
                init_works: true,
                // The part's default: 22.5 °C, 45 %RH.
                payload: [0x18, 0x73, 0x33, 0x35, 0xCC, 0xCD, 0x00],
                pending: Vec::new(),
                writes: Vec::new(),
                reads: Vec::new(),
            }
        }

        fn write_bytes(&mut self, bytes: &[u8]) {
            self.writes.push(bytes.to_vec());
            match bytes {
                [0x71] => {
                    let status = if self.calibrated { 0x18 } else { 0x10 };
                    self.pending = Vec::from([status]);
                }
                [0xBE, _, _] => {
                    if self.init_works {
                        self.calibrated = true;
                    }
                }
                [0xAC, _, _] => self.pending = self.payload.to_vec(),
                _ => {}
            }
        }

        fn read_bytes(&mut self, read: &mut [u8]) {
            self.reads.push(read.len());
            let n = read.len().min(self.pending.len());
            let data: Vec<u8> = self.pending.drain(..n).collect();
            read[..n].copy_from_slice(&data);
            for byte in &mut read[n..] {
                *byte = 0xFF;
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
                    Operation::Write(bytes) => self.write_bytes(bytes),
                    Operation::Read(read) => self.read_bytes(read),
                }
            }
            Ok(())
        }
    }

    fn sensor(calibrated: bool) -> Result<Aht20<Fake>, Aht20Error<Infallible>> {
        Aht20::new(Fake::new(calibrated), ADDRESS)
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.01
    }

    #[test]
    fn it_leaves_a_calibrated_part_alone() {
        let sensor = sensor(true).expect("a calibrated AHT20");
        assert_eq!(
            sensor.i2c.writes,
            [Vec::from([0x71])],
            "only the status read, no initialization"
        );
        assert_eq!(sensor.address(), 0x38);
    }

    #[test]
    fn it_initializes_a_part_that_is_not_calibrated() {
        let sensor = sensor(false).expect("an AHT20 that initializes");
        assert_eq!(
            sensor.i2c.writes,
            [
                Vec::from([0x71]),
                Vec::from([0xBE, 0x08, 0x00]),
                Vec::from([0x71])
            ],
            "status, initialization, status again"
        );
    }

    #[test]
    fn it_refuses_a_part_that_never_calibrates() {
        let mut fake = Fake::new(false);
        fake.init_works = false;
        assert_eq!(
            Aht20::new(fake, ADDRESS).err(),
            Some(Aht20Error::NotCalibrated)
        );
    }

    #[test]
    fn a_measurement_decodes_to_celsius_and_percent() {
        let mut sensor = sensor(true).unwrap();
        let reading = sensor.read().unwrap();
        assert!(close(reading.temperature_c, 22.5), "{reading:?}");
        assert!(close(reading.humidity_percent, 45.0), "{reading:?}");
        assert_eq!(
            sensor.i2c.writes.last(),
            Some(&Vec::from([0xAC, 0x33, 0x00])),
            "one trigger write"
        );
        assert_eq!(sensor.i2c.reads.last(), Some(&7), "seven bytes back");
    }

    #[test]
    fn a_busy_part_is_reported_as_busy() {
        let mut fake = Fake::new(true);
        fake.payload[0] = 0x98; // busy and calibrated
        let mut sensor = Aht20::new(fake, ADDRESS).unwrap();
        assert_eq!(sensor.read().err(), Some(Aht20Error::Busy));
    }

    #[test]
    fn the_status_command_answers_the_status_byte() {
        let mut sensor = sensor(true).unwrap();
        assert_eq!(sensor.status().unwrap(), 0x18);
        assert_eq!(sensor.i2c.writes.last(), Some(&Vec::from([0x71])));
    }
}
