//! An Infineon TLV493D-A1B6 3D magnetic sensor, over any `embedded-hal` I2C
//! bus.
//!
//! Unlike a register-addressed part, the sensor answers a read with its whole
//! ten-byte frame, starting at the register its address pointer holds (0x00
//! after power-up) and walking the pointer as the master clocks:
//!
//! - `0x00`, `0x01`, `0x02`: the top eight bits of Bx, By and Bz,
//! - `0x03`: the temperature's top four bits, then the frame counter and the
//!   channel,
//! - `0x04`: Bx's low nibble in bits 7:4, By's in bits 3:0,
//! - `0x05`: Bz's low nibble, and the power-down flag in bit 4,
//! - `0x06`: the temperature's low eight bits,
//! - `0x07` to `0x09`: the part's factory values.
//!
//! [`Tlv493d::read`] therefore clocks out the ten bytes in one transaction and
//! converts them with the datasheet's scale: 0.098 mT per signed 12-bit count
//! on each axis, and 1.1 °C per unsigned count with 25 °C at count 340.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: the part measures continuously, so the driver
//! writes no configuration (no MOD1/MOD2, no parity bit, no low-power or
//! master-controlled mode), reads no interrupt line and does not check the
//! frame counter or channel. See the fixture's `README.md` for what is left
//! out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep a read log in
// a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with the ADDR strap low; the high strap is 0x1F.
pub const ADDRESS: u8 = 0x5E;

/// The frame's ten read registers.
const FRAME_LEN: usize = 10;

/// Bx's low nibble and By's, in register 0x04.
const BX2: usize = 0x04;
/// Bz's low nibble and the power-down flag, in register 0x05.
const BZ2: usize = 0x05;
/// The temperature's top four bits, in register 0x03.
const TEMP1: usize = 0x03;
/// The temperature's low eight bits, in register 0x06.
const TEMP2: usize = 0x06;

/// The field one signed 12-bit count stands for, in milliteslas.
pub const RESOLUTION_MT: f32 = 0.098;
/// The temperature one count stands for, in degrees Celsius.
pub const TEMPERATURE_LSB_C: f32 = 1.1;
/// The count that reads 25 °C.
const TEMPERATURE_ZERO_RAW: i32 = 340;
/// What that count reads.
pub const TEMPERATURE_ZERO_C: f32 = 25.0;

/// One sample.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    /// The field along X, in milliteslas.
    pub x_mt: f32,
    /// The field along Y, in milliteslas.
    pub y_mt: f32,
    /// The field along Z, in milliteslas.
    pub z_mt: f32,
    /// The part's temperature, in degrees Celsius.
    pub temperature_c: f32,
}

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tlv493dError<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// A TLV493D-A1B6 on an I2C bus.
pub struct Tlv493d<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Tlv493d<I2C> {
    /// Take the sensor and read one frame from it.
    ///
    /// The first frame after power-up can still be the part's power-on frame,
    /// so it is read and dropped: the read is what proves the sensor answers.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Tlv493dError<I2C::Error>> {
        let mut this = Self { i2c, address };
        let _ = this.read()?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Clock out the ten-byte frame and convert it.
    ///
    /// The sensor's address pointer still holds 0x00, so the frame starts at
    /// Bx. No register write precedes the read: the part answers with its
    /// frame as it is.
    pub fn read(&mut self) -> Result<Reading, Tlv493dError<I2C::Error>> {
        let mut frame = [0u8; FRAME_LEN];
        self.i2c
            .read(self.address, &mut frame)
            .map_err(Tlv493dError::Bus)?;
        Ok(decode(&frame))
    }
}

/// The ten frame bytes as physical units.
pub fn decode(frame: &[u8; FRAME_LEN]) -> Reading {
    Reading {
        x_mt: axis(frame[0], frame[BX2] >> 4) * RESOLUTION_MT,
        y_mt: axis(frame[1], frame[BX2]) * RESOLUTION_MT,
        z_mt: axis(frame[2], frame[BZ2]) * RESOLUTION_MT,
        temperature_c: TEMPERATURE_ZERO_C
            + (i32::from(temperature_raw(frame)) - TEMPERATURE_ZERO_RAW) as f32 * TEMPERATURE_LSB_C,
    }
}

/// One axis from its top eight bits and the nibble holding its low four: a
/// signed 12-bit value.
fn axis(upper: u8, lower: u8) -> f32 {
    let raw = (u16::from(upper) << 4) | u16::from(lower & 0x0F);
    // `raw` is 12 bits, so shifting it into the top of an `i16` and back
    // arithmetically sign-extends bit 11.
    let signed = ((raw << 4) as i16) >> 4;
    f32::from(signed)
}

/// The temperature's unsigned 12 bits: four in register 0x03, eight after.
fn temperature_raw(frame: &[u8; FRAME_LEN]) -> u16 {
    (u16::from(frame[TEMP1] & 0xF0) << 4) | u16::from(frame[TEMP2])
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to the frame it answers reads with.
    struct Fake {
        frame: [u8; FRAME_LEN],
        reads: Vec<(u8, usize)>,
        writes: Vec<Vec<u8>>,
    }

    impl Fake {
        fn new(frame: [u8; FRAME_LEN]) -> Self {
            Self {
                frame,
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
                        let n = read.len().min(FRAME_LEN);
                        read[..n].copy_from_slice(&self.frame[..n]);
                    }
                    Operation::Write(bytes) => self.writes.push(bytes.to_vec()),
                }
            }
            Ok(())
        }
    }

    /// A frame as the sensor sends it: the axes' signed 12-bit counts and the
    /// temperature's unsigned one.
    fn frame(bx: i16, by: i16, bz: i16, temperature: u16) -> [u8; FRAME_LEN] {
        let bx = bx as u16 & 0x0FFF;
        let by = by as u16 & 0x0FFF;
        let bz = bz as u16 & 0x0FFF;
        [
            (bx >> 4) as u8,
            (by >> 4) as u8,
            (bz >> 4) as u8,
            ((temperature >> 8) as u8 & 0x0F) << 4,
            ((bx & 0x0F) as u8) << 4 | (by & 0x0F) as u8,
            (bz & 0x0F) as u8,
            temperature as u8,
            0x18,
            0xFF,
            0x1F,
        ]
    }

    fn sensor(frame: [u8; FRAME_LEN]) -> Tlv493d<Fake> {
        Tlv493d::new(Fake::new(frame), ADDRESS).expect("a TLV493D")
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.01
    }

    #[test]
    fn it_reads_one_frame_to_find_the_sensor() {
        let sensor = sensor(frame(0, 0, 0, 340));
        assert_eq!(sensor.address(), 0x5E);
        assert_eq!(
            sensor.i2c.reads,
            [(ADDRESS, FRAME_LEN)],
            "one ten-byte read, and nothing else"
        );
        assert!(sensor.i2c.writes.is_empty(), "the driver never writes");
    }

    #[test]
    fn a_read_decodes_the_three_axes() {
        let mut sensor = sensor(frame(10, 0, -5, 340));
        let reading = sensor.read().unwrap();
        assert!(close(reading.x_mt, 0.98), "{reading:?}");
        assert!(close(reading.y_mt, 0.0), "{reading:?}");
        assert!(close(reading.z_mt, -0.49), "{reading:?}");
    }

    #[test]
    fn the_axes_are_signed_twelve_bit_values() {
        let mut sensor = sensor(frame(-2048, 2047, -1, 340));
        let reading = sensor.read().unwrap();
        assert!(close(reading.x_mt, -200.70), "{reading:?}");
        assert!(close(reading.y_mt, 200.61), "{reading:?}");
        assert!(close(reading.z_mt, -0.10), "{reading:?}");
    }

    #[test]
    fn the_temperature_is_the_datasheets_counts() {
        for (raw, expected) in [(340u16, 25.0), (341, 26.1), (339, 23.9)] {
            let mut sensor = sensor(frame(0, 0, 0, raw));
            let reading = sensor.read().unwrap();
            assert!(close(reading.temperature_c, expected), "{raw} -> {reading:?}");
        }
    }

    #[test]
    fn the_counter_channel_and_flags_do_not_change_the_reading() {
        let mut raw = frame(0, 0, 0, 340);
        // Frame counter 3, channel 0, and the power-down flag clear.
        raw[TEMP1] |= 0b1100;
        let mut sensor = sensor(raw);
        assert!(close(sensor.read().unwrap().temperature_c, 25.0));
    }

    #[test]
    fn the_scale_is_the_datasheets() {
        assert!(close(RESOLUTION_MT, 0.098));
        assert!(close(TEMPERATURE_LSB_C, 1.1));
        assert_eq!(TEMPERATURE_ZERO_RAW, 340);
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
            Tlv493d::new(Broken, ADDRESS).err(),
            Some(Tlv493dError::Bus(ErrorKind::Other))
        );
    }
}
