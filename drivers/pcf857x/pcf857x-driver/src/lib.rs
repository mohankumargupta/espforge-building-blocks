//! A NXP PCF8574 8-bit I2C I/O expander, over any `embedded-hal` I2C bus.
//!
//! The part has no register file. One byte written is the port's output latch,
//! and one byte read is the level on its eight pins, so there is nothing to
//! initialise: a fresh part already answers with every pin pulled high.
//!
//! The pins are *quasi-bidirectional*: writing a 0 drives the pin low, and
//! writing a 1 only releases it, leaving a weak current source to pull it high.
//! A read always returns the level on the pin, so an externally driven low wins
//! over a written 1, and a written 0 reads back as 0. With nothing else on the
//! pins, `write` then `read` gives the byte back, which is what makes a
//! read-modify-write on a single pin work.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the emulator's
//! virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: no interrupt output, no PCF8575's sixteen pins,
//! no PCF8574A address range, no pin-level API. See the fixture's `README.md`
//! for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the write log
// in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with every address pin low; 0x20 to 0x27 are the straps.
pub const ADDRESS: u8 = 0x20;

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pcf857xError<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// A PCF8574 on an I2C bus.
pub struct Pcf857x<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Pcf857x<I2C> {
    /// Take the bus and the address.
    ///
    /// The part has no registers, so there is nothing to write and the
    /// constructor cannot fail: the pins come up as inputs pulled high.
    pub fn new(i2c: I2C, address: u8) -> Self {
        Self { i2c, address }
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Drive the pins: a 0 pulls a pin low, a 1 releases it to its pull-up.
    pub fn write(&mut self, outputs: u8) -> Result<(), Pcf857xError<I2C::Error>> {
        self.i2c
            .write(self.address, &[outputs])
            .map_err(Pcf857xError::Bus)
    }

    /// The level on the eight pins.
    ///
    /// A pin written low reads 0; a pin released to its pull-up reads 1 unless
    /// something else on the board holds it low.
    pub fn read(&mut self) -> Result<u8, Pcf857xError<I2C::Error>> {
        let mut pins = [0u8; 1];
        self.i2c
            .read(self.address, &mut pins)
            .map_err(Pcf857xError::Bus)?;
        Ok(pins[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to a write log and the levels its pins read.
    struct Fake {
        pins: u8,
        writes: Vec<Vec<u8>>,
    }

    impl Fake {
        fn new(pins: u8) -> Self {
            Self {
                pins,
                writes: Vec::new(),
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
                        // The last byte written is the port.
                        if let Some(&last) = bytes.last() {
                            self.pins = last;
                        }
                    }
                    Operation::Read(read) => read.fill(self.pins),
                }
            }
            Ok(())
        }
    }

    fn expander(pins: u8) -> Pcf857x<Fake> {
        Pcf857x::new(Fake::new(pins), ADDRESS)
    }

    #[test]
    fn it_takes_the_address_and_writes_nothing() {
        let expander = expander(0xFF);
        assert_eq!(expander.address(), 0x20);
        assert!(
            expander.i2c.writes.is_empty(),
            "the part has no registers, so the constructor has no transaction"
        );
    }

    #[test]
    fn a_write_is_the_port_byte() {
        let mut expander = expander(0xFF);
        expander.write(0xAA).unwrap();
        assert_eq!(
            expander.i2c.writes,
            [Vec::from([0xAA])],
            "one byte, and it is the latch itself"
        );
    }

    #[test]
    fn a_read_is_the_pin_byte() {
        let mut expander = expander(0x3C);
        assert_eq!(expander.read().unwrap(), 0x3C);
        assert!(
            expander.i2c.writes.is_empty(),
            "a read is a bare read: there is no register to point at first"
        );
    }

    #[test]
    fn a_pin_written_low_reads_low_and_a_released_one_reads_high() {
        let mut expander = expander(0xFF);
        expander.write(0xF0).unwrap();
        assert_eq!(
            expander.read().unwrap(),
            0xF0,
            "the fake models the pull-ups: released pins read high"
        );
    }

    #[test]
    fn a_bus_failure_is_reported() {
        /// A bus that rejects everything.
        struct Broken;

        impl ErrorType for Broken {
            type Error = ErrorKind;
        }

        impl I2c for Broken {
            fn transaction(
                &mut self,
                _address: u8,
                _operations: &mut [Operation<'_>],
            ) -> Result<(), Self::Error> {
                Err(ErrorKind::Other)
            }
        }

        let mut expander = Pcf857x::new(Broken, ADDRESS);
        assert_eq!(
            expander.write(0x00).err(),
            Some(Pcf857xError::Bus(ErrorKind::Other))
        );
        assert_eq!(
            expander.read().err(),
            Some(Pcf857xError::Bus(ErrorKind::Other))
        );
    }
}
