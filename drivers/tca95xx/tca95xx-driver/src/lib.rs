//! A TCA9535 16-bit I2C I/O expander, over any `embedded-hal` I2C bus.
//!
//! The part has two 8-bit ports behind eight registers, and its register
//! pointer auto-increments, so a multi-byte write reaches consecutive
//! registers in one transaction. [`Tca95xx::new`] clears both configuration
//! registers, which makes all sixteen pins outputs, and
//! [`Tca95xx::write_outputs`] writes the whole output latch, low port first,
//! so the two ports change together.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: every pin is an output, so no per-pin
//! direction changes, no polarity inversion, no input reads and no interrupt
//! line. See the fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the write log
// in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with every address pin low; 0x20 to 0x27 are the straps.
pub const ADDRESS: u8 = 0x20;

/// The configuration register of the first port: a set bit is an input.
const CONFIGURATION0: u8 = 0x06;
/// The output latch of the first port; the second port follows it.
const OUTPUT_PORT0: u8 = 0x02;

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tca95xxError<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// A TCA9535 on an I2C bus.
pub struct Tca95xx<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Tca95xx<I2C> {
    /// Configure all sixteen pins as outputs.
    ///
    /// Both configuration registers are written in one transaction, which the
    /// part's auto-incrementing pointer walks.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Tca95xxError<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.write(&[CONFIGURATION0, 0x00, 0x00])?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Drive all sixteen pins at once: bit 0 is P0.0, bit 15 is P1.7.
    ///
    /// The low port is written first and the pointer carries on into the high
    /// port, so no intermediate state is ever driven.
    pub fn write_outputs(&mut self, outputs: u16) -> Result<(), Tca95xxError<I2C::Error>> {
        let [high, low] = outputs.to_be_bytes();
        self.write(&[OUTPUT_PORT0, low, high])
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), Tca95xxError<I2C::Error>> {
        self.i2c
            .write(self.address, bytes)
            .map_err(Tca95xxError::Bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to a write log: the driver never reads.
    struct Fake {
        writes: Vec<Vec<u8>>,
    }

    impl Fake {
        fn new() -> Self {
            Self { writes: Vec::new() }
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
                if let Operation::Write(bytes) = operation {
                    self.writes.push(bytes.to_vec());
                }
            }
            Ok(())
        }
    }

    fn expander() -> Tca95xx<Fake> {
        Tca95xx::new(Fake::new(), ADDRESS).expect("a TCA9535")
    }

    #[test]
    fn it_makes_every_pin_an_output() {
        let expander = expander();
        assert_eq!(expander.address(), 0x20);
        assert_eq!(
            expander.i2c.writes,
            [Vec::from([CONFIGURATION0, 0x00, 0x00])],
            "both configuration registers in one transaction"
        );
    }

    #[test]
    fn a_word_is_one_write() {
        let mut expander = expander();
        expander.write_outputs(0x00A5).unwrap();
        assert_eq!(
            expander.i2c.writes.last(),
            Some(&Vec::from([OUTPUT_PORT0, 0xA5, 0x00])),
            "the low port is the first byte after the register"
        );
    }

    #[test]
    fn the_high_byte_reaches_the_second_port() {
        let mut expander = expander();
        expander.write_outputs(0x1234).unwrap();
        assert_eq!(
            expander.i2c.writes.last(),
            Some(&Vec::from([OUTPUT_PORT0, 0x34, 0x12])),
            "P0.0 is bit 0, P1.7 is bit 15"
        );
    }

    #[test]
    fn all_low_and_all_high_are_whole_words() {
        let mut expander = expander();
        expander.write_outputs(0x0000).unwrap();
        assert_eq!(
            expander.i2c.writes.last(),
            Some(&Vec::from([OUTPUT_PORT0, 0x00, 0x00]))
        );
        expander.write_outputs(0xFFFF).unwrap();
        assert_eq!(
            expander.i2c.writes.last(),
            Some(&Vec::from([OUTPUT_PORT0, 0xFF, 0xFF]))
        );
    }

    #[test]
    fn every_write_goes_to_the_same_address() {
        let mut expander = expander();
        expander.write_outputs(0x8001).unwrap();
        assert_eq!(
            expander.i2c.writes,
            [
                Vec::from([CONFIGURATION0, 0x00, 0x00]),
                Vec::from([OUTPUT_PORT0, 0x01, 0x80]),
            ]
        );
    }

    #[test]
    fn a_bus_failure_is_reported() {
        /// A bus that rejects everything.
        struct Broken;

        impl ErrorType for Broken {
            type Error = embedded_hal::i2c::ErrorKind;
        }

        impl I2c for Broken {
            fn transaction(
                &mut self,
                _address: u8,
                _operations: &mut [Operation<'_>],
            ) -> Result<(), Self::Error> {
                Err(embedded_hal::i2c::ErrorKind::Other)
            }
        }

        assert_eq!(
            Tca95xx::new(Broken, ADDRESS).err(),
            Some(Tca95xxError::Bus(embedded_hal::i2c::ErrorKind::Other))
        );
    }
}
