//! An AOSONG AM2320 temperature and relative humidity sensor, over any
//! `embedded-hal` I2C bus.
//!
//! The chip speaks a Modbus-style read: the master writes the function code
//! and the register window (`0x03, 0x00, 0x04` for humidity and temperature),
//! waits, then reads eight bytes — the function code, the byte count, the two
//! 16-bit registers and their CRC-16. Each register is a tenth of a unit, and
//! the temperature register's top bit is its sign.
//!
//! The write and the read are separate transactions: the part needs a stop
//! before it prepares the answer, so `write_read` with a repeated start does
//! not work on real hardware.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: the frame is decoded but its CRC is not
//! checked, and the wake-up and conversion delays are the caller's. See the
//! fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the pending
// answer in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The AM2320's only I2C address.
pub const ADDRESS: u8 = 0x5C;

/// Read four register bytes from 0x0000: humidity, then temperature.
const READ_REGISTERS: [u8; 3] = [0x03, 0x00, 0x04];

/// Function code for a register read, and the count of register bytes.
const FUNCTION_READ: u8 = 0x03;
const REGISTER_BYTES: u8 = 0x04;

/// Function code, byte count, humidity, temperature, CRC-16.
const FRAME_LEN: usize = 8;

/// One sample.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    pub temperature_c: f32,
    pub humidity_percent: f32,
}

/// Why a transfer or the answer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Am2320Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The answer was not the read frame: another function code, another byte
    /// count, or not enough bytes.
    BadFrame,
}

/// An AM2320 on an I2C bus.
pub struct Am2320<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Am2320<I2C> {
    /// Take the bus and the address, without touching the part.
    ///
    /// The AM2320 has no identity register in the read frame, and every
    /// transaction wakes it from its sleep, so construction stays silent;
    /// [`Am2320::read`] both wakes the part and measures.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Am2320Error<I2C::Error>> {
        Ok(Self { i2c, address })
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Read the two registers and decode them.
    ///
    /// The read command is one write and the sample one read, as the
    /// datasheet describes. Its 1.5 ms conversion time is not waited for: a
    /// caller that reads immediately relies on the bus being slower than the
    /// part, and the emulator answers at once.
    pub fn read(&mut self) -> Result<Reading, Am2320Error<I2C::Error>> {
        self.i2c
            .write(self.address, &READ_REGISTERS)
            .map_err(Am2320Error::Bus)?;
        let mut raw = [0u8; FRAME_LEN];
        self.i2c
            .read(self.address, &mut raw)
            .map_err(Am2320Error::Bus)?;
        if raw[0] != FUNCTION_READ || raw[1] != REGISTER_BYTES {
            return Err(Am2320Error::BadFrame);
        }
        Ok(decode(&raw))
    }
}

/// The eight answer bytes as °C and %RH.
///
/// The registers are big-endian tenths: humidity is unsigned, temperature is
/// signed with the sign in its top bit.
fn decode(raw: &[u8; FRAME_LEN]) -> Reading {
    let humidity = u16::from_be_bytes([raw[2], raw[3]]);
    let temperature = u16::from_be_bytes([raw[4], raw[5]]);
    let temperature_c = if temperature & 0x8000 != 0 {
        -((temperature & 0x7FFF) as f32) / 10.0
    } else {
        temperature as f32 / 10.0
    };
    Reading {
        temperature_c,
        humidity_percent: humidity as f32 / 10.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The chip reduced to: a write prepares the answer, a read clocks it out.
    struct Fake {
        answer: [u8; FRAME_LEN],
        pending: Vec<u8>,
        writes: Vec<Vec<u8>>,
        reads: Vec<usize>,
    }

    impl Fake {
        /// The part's default: 22.5 °C, 45 %RH.
        fn new() -> Self {
            Self {
                answer: [0x03, 0x04, 0x01, 0xC2, 0x00, 0xE1, 0x00, 0x00],
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
                        if &bytes[..] == &READ_REGISTERS[..] {
                            self.pending = self.answer.to_vec();
                        } else {
                            self.pending.clear();
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

    fn sensor() -> Am2320<Fake> {
        Am2320::new(Fake::new(), ADDRESS).expect("an AM2320")
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.01
    }

    #[test]
    fn construction_leaves_the_part_alone() {
        let sensor = sensor();
        assert!(sensor.i2c.writes.is_empty(), "no transaction at boot");
        assert_eq!(sensor.address(), 0x5C);
    }

    #[test]
    fn a_reading_decodes_to_celsius_and_percent() {
        let mut sensor = sensor();
        let reading = sensor.read().unwrap();
        assert!(close(reading.temperature_c, 22.5), "{reading:?}");
        assert!(close(reading.humidity_percent, 45.0), "{reading:?}");
        assert_eq!(
            sensor.i2c.writes,
            [Vec::from(READ_REGISTERS)],
            "one read command"
        );
        assert_eq!(sensor.i2c.reads, [FRAME_LEN], "eight bytes back");
    }

    #[test]
    fn a_negative_temperature_uses_the_top_bit() {
        let mut fake = Fake::new();
        // -5.0 °C: the sign bit plus 50 tenths.
        fake.answer[4..6].copy_from_slice(&[0x80, 0x32]);
        let mut sensor = Am2320::new(fake, ADDRESS).unwrap();
        assert!(close(sensor.read().unwrap().temperature_c, -5.0));
    }

    #[test]
    fn a_frame_with_the_wrong_byte_count_is_refused() {
        let mut fake = Fake::new();
        fake.answer[1] = 0x02;
        let mut sensor = Am2320::new(fake, ADDRESS).unwrap();
        assert_eq!(sensor.read().err(), Some(Am2320Error::BadFrame));
    }

    #[test]
    fn a_frame_with_the_wrong_function_code_is_refused() {
        let mut fake = Fake::new();
        fake.answer[0] = 0x83; // an error answer
        let mut sensor = Am2320::new(fake, ADDRESS).unwrap();
        assert_eq!(sensor.read().err(), Some(Am2320Error::BadFrame));
    }
}
