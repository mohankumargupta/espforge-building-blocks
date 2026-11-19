//! An NXP PCA9685 16-channel PWM controller, over any `embedded-hal` I2C bus.
//!
//! The part is register-based, and every channel has four duty registers that
//! hold the 12-bit on and off counts of one cycle. [`Pca9685::new`] puts the
//! part to sleep (the prescaler may only be written then), programs the
//! prescaler for 50 Hz from the internal 25 MHz oscillator, and wakes it with
//! auto-increment, so a channel's four registers go out as one write.
//!
//! [`Pca9685::set_duty`] then writes one channel's counts: on = 0, off the
//! duty in 1/4096 steps, and the full-on bit for 100 %.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: a fixed 50 Hz frame rate, so no
//! `set_frequency`, no inverted output, no totem-pole/open-drain selection, no
//! external clock, no ALLCALL or SUBADR addressing, no restart or sleep
//! control after construction, and no read-back. See the fixture's `README.md`
//! for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the write log
// in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with every address pin low; 0x40 to 0x7F are the straps.
pub const ADDRESS: u8 = 0x40;

/// The mode register: sleep and auto-increment live here.
const MODE1: u8 = 0x00;
/// The first of the channel registers; each channel is four bytes after it.
const LED0_ON_L: u8 = 0x06;
/// The prescaler, writable only while the part sleeps.
const PRESCALE: u8 = 0xFE;

/// The sleep bit: the oscillator stops and the prescaler may be written.
const SLEEP: u8 = 0x10;
/// The auto-increment bit: a multi-byte write walks the registers.
const AUTO_INCREMENT: u8 = 0x20;

/// The part's internal oscillator.
const OSCILLATOR_HZ: u32 = 25_000_000;
/// 12-bit duty resolution.
const STEPS: u32 = 4096;
/// A 12-bit count of 4096 is out of range, so one more value means full on.
const FULL_ON: u16 = 4096;
/// How many channels the part drives.
const CHANNELS: u8 = 16;
/// The frame rate `new` programs, the usual servo rate.
const FREQUENCY_HZ: f32 = 50.0;

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pca9685Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// There is no channel with this index; the part has 0 to 15.
    Channel,
}

/// A PCA9685 on an I2C bus.
pub struct Pca9685<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Pca9685<I2C> {
    /// Sleep, program the prescaler for 50 Hz, and wake with auto-increment.
    ///
    /// The oscillator's 500 µs start-up is not waited for: the emulator
    /// completes at once, and a real caller's next transaction is slow enough
    /// on its own.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Pca9685Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.write(&[MODE1, SLEEP])?;
        this.write(&[PRESCALE, prescale(FREQUENCY_HZ)])?;
        this.write(&[MODE1, AUTO_INCREMENT])?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// The frame rate `new` programmed.
    pub fn frequency_hz(&self) -> f32 {
        FREQUENCY_HZ
    }

    /// Set one channel's duty cycle.
    ///
    /// `percent` runs 0 to 100 and is clamped; 100 % uses the full-on bit,
    /// which the counts themselves cannot express.
    pub fn set_duty(
        &mut self,
        channel: u8,
        percent: f32,
    ) -> Result<(), Pca9685Error<I2C::Error>> {
        if channel >= CHANNELS {
            return Err(Pca9685Error::Channel);
        }
        let off = counts(percent);
        let [high, low] = off.to_be_bytes();
        // On = 0, so the channel is low for the first `off` counts.
        let register = LED0_ON_L + 4 * channel;
        self.write(&[register, 0x00, 0x00, low, high])
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), Pca9685Error<I2C::Error>> {
        self.i2c
            .write(self.address, bytes)
            .map_err(Pca9685Error::Bus)
    }
}

/// The prescale value for a frame rate, from the datasheet's formula:
/// `round(25 MHz / (4096 * rate)) - 1`.
fn prescale(hz: f32) -> u8 {
    // Not `f32::round`: this crate is `no_std` and keeps no float library.
    // Adding half a step before the cast rounds half up, and both operands
    // are positive.
    let divisor = (OSCILLATOR_HZ as f32 / (STEPS as f32 * hz) + 0.5) as u32;
    divisor.saturating_sub(1).min(255) as u8
}

/// A duty cycle in 1/4096 steps, with 4096 for full on.
fn counts(percent: f32) -> u16 {
    if percent >= 100.0 {
        FULL_ON
    } else {
        (percent.max(0.0) / 100.0 * STEPS as f32 + 0.5) as u16
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

    fn controller() -> Pca9685<Fake> {
        Pca9685::new(Fake::new(), ADDRESS).expect("a PCA9685")
    }

    #[test]
    fn it_sleeps_programs_the_prescaler_and_wakes() {
        let controller = controller();
        assert_eq!(controller.address(), 0x40);
        assert_eq!(controller.frequency_hz(), 50.0);
        assert_eq!(
            controller.i2c.writes,
            [
                Vec::from([MODE1, SLEEP]),
                Vec::from([PRESCALE, 121]),
                Vec::from([MODE1, AUTO_INCREMENT]),
            ],
            "the prescaler is written while the part sleeps"
        );
    }

    #[test]
    fn a_duty_cycle_is_one_channel_write() {
        let mut controller = controller();
        controller.set_duty(0, 25.0).unwrap();
        assert_eq!(
            controller.i2c.writes.last(),
            Some(&Vec::from([LED0_ON_L, 0x00, 0x00, 0x00, 0x04])),
            "25 % of 4096 is 1024: on 0, off 0x0400"
        );
    }

    #[test]
    fn every_channel_has_its_own_four_registers() {
        let mut controller = controller();
        controller.set_duty(3, 50.0).unwrap();
        assert_eq!(
            controller.i2c.writes.last(),
            Some(&Vec::from([LED0_ON_L + 12, 0x00, 0x00, 0x00, 0x08])),
            "2048 counts half a cycle"
        );
        controller.set_duty(15, 25.0).unwrap();
        assert_eq!(
            controller.i2c.writes.last(),
            Some(&Vec::from([LED0_ON_L + 60, 0x00, 0x00, 0x00, 0x04])),
            "the last channel sits at 0x42"
        );
    }

    #[test]
    fn full_on_uses_the_bit_the_counts_cannot_reach() {
        let mut controller = controller();
        controller.set_duty(1, 100.0).unwrap();
        assert_eq!(
            controller.i2c.writes.last(),
            Some(&Vec::from([LED0_ON_L + 4, 0x00, 0x00, 0x00, 0x10])),
            "off 0x1000 is the full-on bit, not a count"
        );
        controller.set_duty(1, 1000.0).unwrap();
        assert_eq!(
            controller.i2c.writes.last(),
            Some(&Vec::from([LED0_ON_L + 4, 0x00, 0x00, 0x00, 0x10])),
            "over 100 % is clamped to full on"
        );
    }

    #[test]
    fn zero_and_below_are_off() {
        let mut controller = controller();
        controller.set_duty(0, 0.0).unwrap();
        assert_eq!(
            controller.i2c.writes.last(),
            Some(&Vec::from([LED0_ON_L, 0x00, 0x00, 0x00, 0x00]))
        );
        controller.set_duty(0, -5.0).unwrap();
        assert_eq!(
            controller.i2c.writes.last(),
            Some(&Vec::from([LED0_ON_L, 0x00, 0x00, 0x00, 0x00])),
            "below zero is clamped"
        );
    }

    #[test]
    fn a_channel_that_does_not_exist_is_refused() {
        let mut controller = controller();
        let writes = controller.i2c.writes.len();
        assert_eq!(controller.set_duty(16, 50.0), Err(Pca9685Error::Channel));
        assert_eq!(
            controller.i2c.writes.len(),
            writes,
            "nothing goes on the bus"
        );
    }

    #[test]
    fn the_prescaler_follows_the_datasheet_formula() {
        assert_eq!(prescale(50.0), 121);
        assert_eq!(prescale(60.0), 101);
        assert_eq!(prescale(1000.0), 5);
    }
}
