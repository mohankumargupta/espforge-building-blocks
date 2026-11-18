//! A ROHM BH1750 ambient light sensor, over any `embedded-hal` I2C bus.
//!
//! The part takes one-byte commands and answers a measurement with two
//! big-endian bytes of raw counts. [`Bh1750::new`] powers it on;
//! [`Bh1750::read`] triggers a one-shot high-resolution measurement and returns
//! the illuminance in lux, converting with the datasheet's 1.2 counts per lux.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: one resolution, one-shot mode, and the
//! measurement time is the caller's. See the fixture's `README.md` for what is
//! left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and record the
// transactions they saw.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The BH1750's address with its ADDR pin low; the other strapping is 0x5C.
pub const ADDRESS: u8 = 0x23;

/// The commands this driver sends.
const POWER_ON: u8 = 0x01;
const ONE_SHOT_HIGH_RES: u8 = 0x20;

/// Raw counts per lux in high-resolution mode, from the datasheet.
const COUNTS_PER_LUX: f32 = 1.2;

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bh1750Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// A BH1750 on an I2C bus.
pub struct Bh1750<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Bh1750<I2C> {
    /// Power the part on.
    ///
    /// The measurement's 120 ms are not waited for: the emulator completes it
    /// at once.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Bh1750Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.command(POWER_ON)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Trigger a one-shot high-resolution measurement and read the lux.
    ///
    /// On hardware the conversion takes up to 180 ms after the command; the
    /// driver does not wait, so a real part needs the caller's delay.
    pub fn read(&mut self) -> Result<f32, Bh1750Error<I2C::Error>> {
        self.command(ONE_SHOT_HIGH_RES)?;
        let mut raw = [0u8; 2];
        self.i2c
            .read(self.address, &mut raw)
            .map_err(Bh1750Error::Bus)?;
        Ok(f32::from(u16::from_be_bytes(raw)) / COUNTS_PER_LUX)
    }

    /// Send one command byte.
    fn command(&mut self, command: u8) -> Result<(), Bh1750Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[command])
            .map_err(Bh1750Error::Bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// A part that records commands and answers with a fixed count.
    struct Fake {
        raw: u16,
        writes: Vec<Vec<u8>>,
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
                    Operation::Write(bytes) => self.writes.push(bytes.to_vec()),
                    Operation::Read(read) => {
                        let bytes = self.raw.to_be_bytes();
                        for (slot, byte) in read.iter_mut().zip(bytes.iter().cycle()) {
                            *slot = *byte;
                        }
                    }
                }
            }
            Ok(())
        }
    }

    fn sensor(raw: u16) -> Bh1750<Fake> {
        Bh1750::new(Fake { raw, writes: Vec::new() }, ADDRESS).expect("a BH1750")
    }

    #[test]
    fn it_powers_the_part_on() {
        let sensor = sensor(360);
        assert_eq!(sensor.i2c.writes, [Vec::from([POWER_ON])]);
        assert_eq!(sensor.address(), ADDRESS);
        assert_eq!(sensor.address(), 0x23);
    }

    #[test]
    fn a_measurement_asks_for_one_shot_high_res_then_reads() {
        let mut sensor = sensor(360);
        let lux = sensor.read().unwrap();
        assert_eq!(
            sensor.i2c.writes,
            [Vec::from([POWER_ON]), Vec::from([ONE_SHOT_HIGH_RES])],
            "the power-on, then the one-shot command"
        );
        assert!((lux - 300.0).abs() < 0.01, "360 counts is 300 lux, got {lux}");
    }

    #[test]
    fn the_counts_are_big_endian() {
        // 1200 counts is 0x04B0; a little-endian read would give 0xB004.
        let mut sensor = sensor(1200);
        let lux = sensor.read().unwrap();
        assert!((lux - 1000.0).abs() < 0.01, "1200 counts is 1000 lux, got {lux}");
    }

    #[test]
    fn a_dark_part_reads_zero_lux() {
        let mut sensor = sensor(0);
        assert_eq!(sensor.read().unwrap(), 0.0);
    }
}
