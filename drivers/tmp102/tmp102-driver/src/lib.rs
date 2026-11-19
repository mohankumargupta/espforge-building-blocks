//! A TI TMP102 temperature sensor, over any `embedded-hal` I2C bus.
//!
//! The part is register addressed: a write's byte is the register pointer and
//! the read that follows starts there. Register 0x00 is the temperature, two
//! bytes, most significant first:
//!
//! - the first byte is the top eight bits of a signed 12-bit value,
//! - the second holds the low four bits in bits 7:4, and bits 3:0 read as zero
//!   in the part's 12-bit resolution.
//!
//! [`Tmp102::read`] writes that pointer and clocks out both bytes in one
//! transaction, then converts them with the datasheet's 0.0625 °C per count.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: the part converts continuously at its power-up
//! configuration, so the driver writes no configuration (no shutdown, no
//! one-shot, no extended 13-bit mode, no conversion rate or alert polarity),
//! never touches T_LOW/T_HIGH and reads no ALERT line. See the fixture's
//! `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with ADD0 tied to ground: 0x48.
pub const ADDRESS: u8 = 0x48;

/// The temperature register's pointer.
const REG_TEMPERATURE: u8 = 0x00;

/// The temperature one count stands for, in degrees Celsius.
pub const TEMPERATURE_LSB_C: f32 = 0.0625;

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tmp102Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// A TMP102 on an I2C bus.
pub struct Tmp102<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Tmp102<I2C> {
    /// Take the sensor and read its temperature register once.
    ///
    /// The read is what proves the sensor answers; its value is dropped.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Tmp102Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        let _ = this.read()?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Read the temperature register, in degrees Celsius.
    pub fn read(&mut self) -> Result<f32, Tmp102Error<I2C::Error>> {
        let mut raw = [0u8; 2];
        self.i2c
            .write_read(self.address, &[REG_TEMPERATURE], &mut raw)
            .map_err(Tmp102Error::Bus)?;
        Ok(decode(raw))
    }
}

/// The register's two bytes as degrees Celsius.
pub fn decode(raw: [u8; 2]) -> f32 {
    // Shifting the signed 16-bit value right by four sign-extends the 12-bit
    // value and drops the resolution bits, which read as zero.
    f32::from(i16::from_be_bytes(raw) >> 4) * TEMPERATURE_LSB_C
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to the two bytes register 0x00 answers with.
    struct Fake {
        raw: [u8; 2],
        reads: Vec<(u8, usize)>,
        writes: Vec<(u8, Vec<u8>)>,
    }

    impl Fake {
        fn new(raw: [u8; 2]) -> Self {
            Self {
                raw,
                reads: Vec::new(),
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
            address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            for operation in operations {
                match operation {
                    Operation::Read(read) => {
                        self.reads.push((address, read.len()));
                        let n = read.len().min(2);
                        read[..n].copy_from_slice(&self.raw[..n]);
                    }
                    Operation::Write(bytes) => self.writes.push((address, bytes.to_vec())),
                }
            }
            Ok(())
        }
    }

    /// A register value as the part sends it: a signed 12-bit count, left
    /// justified in two bytes.
    fn register(celsius: f32) -> [u8; 2] {
        let counts = (celsius / TEMPERATURE_LSB_C).round() as i16;
        (counts << 4).to_be_bytes()
    }

    fn sensor(raw: [u8; 2]) -> Tmp102<Fake> {
        Tmp102::new(Fake::new(raw), ADDRESS).expect("a TMP102")
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.001
    }

    #[test]
    fn it_reads_the_temperature_register_to_find_the_sensor() {
        let sensor = sensor(register(25.0));
        assert_eq!(sensor.address(), 0x48);
        assert_eq!(
            sensor.i2c.writes,
            [(ADDRESS, std::vec![REG_TEMPERATURE])],
            "the pointer write, and nothing else"
        );
        assert_eq!(
            sensor.i2c.reads,
            [(ADDRESS, 2)],
            "one two-byte read of the temperature register"
        );
    }

    #[test]
    fn a_read_decodes_the_temperature() {
        for celsius in [25.0f32, 26.5, -5.0, -40.0, 125.0, 0.0] {
            let mut sensor = sensor(register(celsius));
            let reading = sensor.read().unwrap();
            assert!(close(reading, celsius), "{celsius} -> {reading}");
        }
    }

    #[test]
    fn the_register_is_left_justified() {
        // 25 C is count 400 (0x190): the low nibble of the second byte is the
        // resolution bits and reads as zero.
        assert_eq!(register(25.0), [0x19, 0x00]);
        assert!(close(decode([0x19, 0x0F]), 25.0), "bits 3:0 are ignored");
        // -0.0625 C is count -1: all twelve bits set.
        assert_eq!(register(-0.0625), [0xFF, 0xF0]);
    }

    #[test]
    fn the_scale_is_the_datasheets() {
        assert!(close(TEMPERATURE_LSB_C, 0.0625));
        assert!(close(decode(register(1.0)), 1.0));
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

        assert_eq!(
            Tmp102::new(Broken, ADDRESS).err(),
            Some(Tmp102Error::Bus(ErrorKind::Other))
        );
    }
}
