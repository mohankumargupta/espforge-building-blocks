//! A Sensirion SGP30 air-quality sensor, over any `embedded-hal` I2C bus.
//!
//! The part is command-driven: it has no register pointer. [`Sgp30::new`]
//! sends the two-byte initialise-air-quality command (with its CRC byte), and
//! [`Sgp30::read`] sends the two-byte measure-IAQ command and then reads six
//! bytes: eCO2 in ppm and TVOC in ppb, each 16-bit big-endian followed by its
//! CRC-8.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: no baseline read or write, no humidity
//! compensation, no raw signals, no feature-set version, and no measurement
//! timing. A real sensor wants 15 s between initialisation and the first
//! measurement; the caller spaces the reads, and the emulator answers at once.
//! See the fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the command
// log in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's only address.
pub const ADDRESS: u8 = 0x58;

/// Initialise air quality measurement: 0x2003 and its CRC.
const IAQ_INIT: [u8; 3] = [0x20, 0x03, 0x0E];
/// Select the IAQ measurement: 0x2008 and its CRC.
const MEASURE_IAQ: [u8; 3] = [0x20, 0x08, 0xE4];

/// Two words of two bytes and a CRC each.
const MEASUREMENT_LEN: usize = 6;

/// One sample, in physical units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    /// Equivalent CO2 concentration, parts per million.
    pub eco2_ppm: u16,
    /// Total volatile organic compounds, parts per billion.
    pub tvoc_ppb: u16,
}

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sgp30Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// A data word failed its CRC check.
    Crc,
}

/// An SGP30 on an I2C bus.
pub struct Sgp30<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Sgp30<I2C> {
    /// Initialise air quality measurement.
    ///
    /// The command has no answer; a dead sensor surfaces on the first read.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Sgp30Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.i2c
            .write(address, &IAQ_INIT)
            .map_err(Sgp30Error::Bus)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Read the IAQ measurement.
    ///
    /// The 12 ms execution time and the 15 s start-up are not waited for: the
    /// emulator answers at once, and a real caller's read interval is slow
    /// enough on its own.
    pub fn read(&mut self) -> Result<Reading, Sgp30Error<I2C::Error>> {
        self.i2c
            .write(self.address, &MEASURE_IAQ)
            .map_err(Sgp30Error::Bus)?;
        let mut raw = [0u8; MEASUREMENT_LEN];
        self.i2c
            .read(self.address, &mut raw)
            .map_err(Sgp30Error::Bus)?;
        decode(&raw).ok_or(Sgp30Error::Crc)
    }
}

/// The two measured words, each checked against its CRC byte.
fn decode(raw: &[u8; MEASUREMENT_LEN]) -> Option<Reading> {
    Some(Reading {
        eco2_ppm: word(raw, 0)?,
        tvoc_ppb: word(raw, 3)?,
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
        /// 400 ppm of eCO2 and 0 ppb of TVOC.
        fn new() -> Self {
            Self {
                payload: [0x01, 0x90, 0x4C, 0x00, 0x00, 0x81],
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
                        if *bytes == MEASURE_IAQ.as_slice() {
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

    fn sensor() -> Sgp30<Fake> {
        Sgp30::new(Fake::new(), ADDRESS).expect("an SGP30")
    }

    #[test]
    fn it_initialises_air_quality() {
        let sensor = sensor();
        assert_eq!(sensor.address(), 0x58);
        assert_eq!(
            sensor.i2c.writes,
            [Vec::from(IAQ_INIT)],
            "one initialise command, no other setup"
        );
    }

    #[test]
    fn a_read_returns_both_fields() {
        let mut sensor = sensor();
        let reading = sensor.read().unwrap();
        assert_eq!(
            reading,
            Reading {
                eco2_ppm: 400,
                tvoc_ppb: 0,
            }
        );
        assert_eq!(
            sensor.i2c.writes.last(),
            Some(&Vec::from(MEASURE_IAQ)),
            "one measure-IAQ command"
        );
        assert_eq!(
            sensor.i2c.reads.last(),
            Some(&MEASUREMENT_LEN),
            "six bytes back, two words and their CRCs"
        );
    }

    #[test]
    fn both_words_are_decoded() {
        let mut sensor = sensor();
        // 1000 ppm and 137 ppb.
        sensor.i2c.payload = [0x03, 0xE8, 0xD4, 0x00, 0x89, 0x73];
        assert_eq!(
            sensor.read().unwrap(),
            Reading {
                eco2_ppm: 1000,
                tvoc_ppb: 137,
            }
        );
    }

    #[test]
    fn a_bad_crc_is_refused() {
        let mut sensor = self::sensor();
        sensor.i2c.payload[2] ^= 0xFF;
        assert_eq!(sensor.read().err(), Some(Sgp30Error::Crc));
        let mut sensor = self::sensor();
        sensor.i2c.payload[5] ^= 0xFF;
        assert_eq!(sensor.read().err(), Some(Sgp30Error::Crc));
    }

    #[test]
    fn the_commands_carry_their_crc() {
        assert_eq!(IAQ_INIT, [0x20, 0x03, crc8([0x20, 0x03])]);
        assert_eq!(MEASURE_IAQ, [0x20, 0x08, crc8([0x20, 0x08])]);
    }

    #[test]
    fn the_crc_matches_the_datasheet_example() {
        assert_eq!(crc8([0xBE, 0xEF]), 0x92);
    }
}
