//! A TI HDC302x temperature and relative humidity sensor, over any
//! `embedded-hal` I2C bus.
//!
//! The part is command-driven: it has no register pointer. [`Hdc302x::new`]
//! sends the two-byte soft reset, and [`Hdc302x::read`] sends the two-byte
//! trigger-on-demand command and then reads the four measurement bytes, which
//! the datasheet converts as
//!
//! - temperature in °C: `-45 + 175 * raw / 65535`,
//! - relative humidity in %: `100 * raw / 65535`.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: no CRC variant of the commands, no heater, no
//! auto-measurement mode, no alerts or thresholds, no low-power/mode
//! selection. See the fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the command
// log in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with both address pins low; 0x45 to 0x47 are the other
/// straps.
pub const ADDRESS: u8 = 0x44;

/// Trigger one on-demand measurement, low-power mode, no CRC.
const TRIGGER_ON_DEMAND: [u8; 2] = [0x24, 0x00];
/// Soft reset: the control registers go back to their power-on values.
const SOFT_RESET: [u8; 2] = [0x30, 0xA2];

/// Temperature and humidity, four bytes, most significant byte first.
const MEASUREMENT_LEN: usize = 4;

/// The conversion spans the whole 16-bit range.
const FULL_SCALE: f32 = 65_535.0;
/// The datasheet's two ends of the temperature scale.
const MIN_TEMPERATURE_C: f32 = -45.0;
const TEMPERATURE_SPAN_C: f32 = 175.0;

/// One sample.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    /// Degrees Celsius.
    pub temperature_c: f32,
    /// Percent relative humidity.
    pub humidity_percent: f32,
}

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hdc302xError<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// An HDC302x on an I2C bus.
pub struct Hdc302x<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Hdc302x<I2C> {
    /// Reset the part, which puts its heater and alert settings back to their
    /// power-on values.
    ///
    /// The reset's 200 µs is not waited for: the emulator completes at once,
    /// and a real caller's next transaction is slow enough on its own.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Hdc302xError<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.i2c
            .write(address, &SOFT_RESET)
            .map_err(Hdc302xError::Bus)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Trigger one measurement and read it.
    ///
    /// The trigger is one write and the sample one read, as the datasheet
    /// describes. The conversion's 8.5 ms is not waited for: the emulator
    /// completes at once, and a real caller waits before reading.
    pub fn read(&mut self) -> Result<Reading, Hdc302xError<I2C::Error>> {
        self.i2c
            .write(self.address, &TRIGGER_ON_DEMAND)
            .map_err(Hdc302xError::Bus)?;
        let mut raw = [0u8; MEASUREMENT_LEN];
        self.i2c
            .read(self.address, &mut raw)
            .map_err(Hdc302xError::Bus)?;
        Ok(decode(&raw))
    }
}

/// The four measurement bytes as °C and %RH.
///
/// The temperature pair comes first, then the humidity pair, both 16-bit
/// big-endian.
fn decode(raw: &[u8; MEASUREMENT_LEN]) -> Reading {
    let temperature = u16::from_be_bytes([raw[0], raw[1]]);
    let humidity = u16::from_be_bytes([raw[2], raw[3]]);
    Reading {
        temperature_c: MIN_TEMPERATURE_C + TEMPERATURE_SPAN_C * f32::from(temperature) / FULL_SCALE,
        humidity_percent: 100.0 * f32::from(humidity) / FULL_SCALE,
    }
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
        /// The part's default: 22.5 °C and 45 %RH, which the datasheet's
        /// conversions encode as 25278 and 29491.
        fn new() -> Self {
            Self {
                payload: [0x62, 0xBE, 0x73, 0x33],
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
                        if *bytes == TRIGGER_ON_DEMAND.as_slice() {
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

    fn sensor() -> Hdc302x<Fake> {
        Hdc302x::new(Fake::new(), ADDRESS).expect("an HDC302x")
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.01
    }

    #[test]
    fn it_resets_the_part() {
        let sensor = sensor();
        assert_eq!(sensor.address(), 0x44);
        assert_eq!(
            sensor.i2c.writes,
            [Vec::from(SOFT_RESET)],
            "one soft reset, no other setup"
        );
    }

    #[test]
    fn a_measurement_is_triggered_then_read() {
        let mut sensor = sensor();
        let reading = sensor.read().unwrap();
        assert!(close(reading.temperature_c, 22.5), "{reading:?}");
        assert!(close(reading.humidity_percent, 45.0), "{reading:?}");
        assert_eq!(
            sensor.i2c.writes.last(),
            Some(&Vec::from(TRIGGER_ON_DEMAND)),
            "one trigger write"
        );
        assert_eq!(
            sensor.i2c.reads.last(),
            Some(&MEASUREMENT_LEN),
            "four bytes back"
        );
    }

    #[test]
    fn the_conversion_covers_the_datasheet_range() {
        let mut sensor = sensor();
        sensor.i2c.payload = [0x00, 0x00, 0x00, 0x00];
        let reading = sensor.read().unwrap();
        assert_eq!(reading.temperature_c, MIN_TEMPERATURE_C);
        assert_eq!(reading.humidity_percent, 0.0);

        sensor.i2c.payload = [0xFF, 0xFF, 0xFF, 0xFF];
        let reading = sensor.read().unwrap();
        assert!(close(reading.temperature_c, 130.0), "{reading:?}");
        assert!(close(reading.humidity_percent, 100.0), "{reading:?}");
    }

    #[test]
    fn the_two_pairs_are_big_endian() {
        let mut sensor = sensor();
        // 0x8000 is half scale: 42.5 °C and 50 %RH.
        sensor.i2c.payload = [0x80, 0x00, 0x80, 0x00];
        let reading = sensor.read().unwrap();
        assert!(close(reading.temperature_c, 42.5), "{reading:?}");
        assert!(close(reading.humidity_percent, 50.0), "{reading:?}");
    }
}
