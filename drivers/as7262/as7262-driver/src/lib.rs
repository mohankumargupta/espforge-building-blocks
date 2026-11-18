//! An ams AS7262 six-channel visible light spectral sensor, over any
//! `embedded-hal` I2C bus.
//!
//! Only three physical registers exist: STATUS, WRITE and READ. Every other
//! register is virtual and reached through them:
//!
//! - a write is the virtual address with bit 7 set, then the data byte, both
//!   written to the WRITE register;
//! - a read is the virtual address with bit 7 clear, then one byte read from
//!   the READ register, with STATUS' `RX_VALID` saying when it is there.
//!
//! [`As7262::new`] soft-resets the part and sets its integration time.
//! [`As7262::read`] selects 16x gain and a one-shot conversion of both
//! photodiode banks, then reads the six big-endian raw channels: violet, blue,
//! green, yellow, orange and red.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: the calibrated floating-point registers are
//! ignored, the conversion time is the caller's, and a busy interface is
//! reported rather than polled. See the fixture's `README.md` for what is left
//! out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the virtual
// register file in an array.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The AS7262's only I2C address.
pub const ADDRESS: u8 = 0x49;

/// The three physical registers.
const STATUS_REGISTER: u8 = 0x00;
const WRITE_REGISTER: u8 = 0x01;
const READ_REGISTER: u8 = 0x02;

/// STATUS bit 1: the WRITE register still holds a byte; 1 means busy.
const TX_VALID: u8 = 0x02;
/// STATUS bit 0: a byte is waiting in the READ register.
const RX_VALID: u8 = 0x01;

/// Virtual register addresses the driver uses.
const CONTROL_SETUP: u8 = 0x04;
const INTEGRATION_TIME: u8 = 0x05;
const DATA_START: u8 = 0x08;

/// Bit 7 of a virtual address marks a write; clear means a read.
const WRITE_MARKER: u8 = 0x80;

/// Control_Setup: soft reset, and the gain/bank bits this driver writes.
const SOFT_RESET: u8 = 0x80;
/// Gain `0b10` in bits 5:4 is 16x.
const GAIN_16X: u8 = 0b10 << 4;
/// Bank `0b11` in bits 3:2 is one-shot, both photodiode banks.
const MODE_ONE_SHOT: u8 = 0b11 << 2;
/// Control_Setup bit 1: a conversion's data is ready to read.
const DATA_READY: u8 = 0x02;

/// Integration time in units of 2.8 ms; 50 is 140 ms.
const INTEGRATION_TIME_DEFAULT: u8 = 50;

/// Six channels, two big-endian bytes each, from 0x08 to 0x13.
const CHANNELS: usize = 6;
const CHANNEL_BYTES: usize = CHANNELS * 2;

/// One measurement: the six raw channels, in order V, B, G, Y, O, R.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Spectrum {
    pub violet: u16,
    pub blue: u16,
    pub green: u16,
    pub yellow: u16,
    pub orange: u16,
    pub red: u16,
}

/// Why a transfer failed, or why the part was not ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum As7262Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The interface held a byte, or a read had not arrived.
    Busy,
}

/// An AS7262 on an I2C bus.
pub struct As7262<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> As7262<I2C> {
    /// Soft-reset the part and set its integration time.
    ///
    /// The reset's and the conversion's timings are not waited for: the
    /// emulator completes both at once.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, As7262Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.write_register(CONTROL_SETUP, SOFT_RESET)?;
        this.write_register(INTEGRATION_TIME, INTEGRATION_TIME_DEFAULT)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Trigger a one-shot conversion and read the six raw channels.
    ///
    /// The conversion takes two integration times on hardware; the driver
    /// reports `Busy` instead of waiting for it.
    pub fn read(&mut self) -> Result<Spectrum, As7262Error<I2C::Error>> {
        self.write_register(CONTROL_SETUP, GAIN_16X | MODE_ONE_SHOT)?;
        if self.read_register(CONTROL_SETUP)? & DATA_READY == 0 {
            return Err(As7262Error::Busy);
        }
        let mut raw = [0u8; CHANNEL_BYTES];
        for (offset, byte) in raw.iter_mut().enumerate() {
            *byte = self.read_register(DATA_START + offset as u8)?;
        }
        Ok(Spectrum {
            violet: u16::from_be_bytes([raw[0], raw[1]]),
            blue: u16::from_be_bytes([raw[2], raw[3]]),
            green: u16::from_be_bytes([raw[4], raw[5]]),
            yellow: u16::from_be_bytes([raw[6], raw[7]]),
            orange: u16::from_be_bytes([raw[8], raw[9]]),
            red: u16::from_be_bytes([raw[10], raw[11]]),
        })
    }

    /// Write one virtual register byte, as the datasheet's pseudocode does.
    fn write_register(&mut self, register: u8, value: u8) -> Result<(), As7262Error<I2C::Error>> {
        self.wait_ready_to_write()?;
        self.i2c
            .write(self.address, &[WRITE_REGISTER, register | WRITE_MARKER])
            .map_err(As7262Error::Bus)?;
        self.wait_ready_to_write()?;
        self.i2c
            .write(self.address, &[WRITE_REGISTER, value])
            .map_err(As7262Error::Bus)
    }

    /// Read one virtual register byte: address it, then take the byte.
    fn read_register(&mut self, register: u8) -> Result<u8, As7262Error<I2C::Error>> {
        self.wait_ready_to_write()?;
        self.i2c
            .write(self.address, &[WRITE_REGISTER, register])
            .map_err(As7262Error::Bus)?;
        if self.status()? & RX_VALID == 0 {
            return Err(As7262Error::Busy);
        }
        let mut byte = [0u8; 1];
        self.i2c
            .write_read(self.address, &[READ_REGISTER], &mut byte)
            .map_err(As7262Error::Bus)?;
        Ok(byte[0])
    }

    /// The interface's STATUS byte: `TX_VALID` busy, `RX_VALID` a byte ready.
    fn status(&mut self) -> Result<u8, As7262Error<I2C::Error>> {
        let mut status = [0u8; 1];
        self.i2c
            .write_read(self.address, &[STATUS_REGISTER], &mut status)
            .map_err(As7262Error::Bus)?;
        Ok(status[0])
    }

    /// One STATUS poll: a part that is still busy is `Busy`, not a wait.
    fn wait_ready_to_write(&mut self) -> Result<(), As7262Error<I2C::Error>> {
        if self.status()? & TX_VALID != 0 {
            return Err(As7262Error::Busy);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The chip reduced to its three physical registers over a virtual
    /// register file.
    struct Fake {
        registers: [u8; 32],
        /// A virtual write's address, waiting for its data byte.
        write_address: Option<u8>,
        /// A virtual read's address, whose byte the next READ takes.
        read_address: Option<u8>,
        /// Whether a one-shot write makes the part report `DATA_RDY`.
        ready_after_conversion: bool,
        writes: Vec<Vec<u8>>,
    }

    impl Fake {
        /// A part whose six channels are a ramp: 0x0102, 0x0304, ... 0x0B0C.
        fn new() -> Self {
            let mut registers = [0u8; 32];
            for i in 0..CHANNEL_BYTES {
                registers[DATA_START as usize + i] = (i as u8) + 1;
            }
            Self {
                registers,
                write_address: None,
                read_address: None,
                ready_after_conversion: true,
                writes: Vec::new(),
            }
        }

        fn write_byte(&mut self, byte: u8) {
            if let Some(address) = self.write_address.take() {
                self.registers[address as usize & 0x1F] = byte;
                if address == CONTROL_SETUP {
                    if byte & SOFT_RESET != 0 {
                        self.registers[CONTROL_SETUP as usize] = 0b10 << 2;
                    } else if byte & MODE_ONE_SHOT == MODE_ONE_SHOT && self.ready_after_conversion {
                        self.registers[CONTROL_SETUP as usize] |= DATA_READY;
                    }
                }
            } else if byte & WRITE_MARKER != 0 {
                self.write_address = Some(byte & !WRITE_MARKER);
            } else {
                self.read_address = Some(byte & !WRITE_MARKER);
            }
        }

        fn read_byte(&mut self) -> u8 {
            match self.read_address.take() {
                Some(address) => {
                    let value = self.registers[address as usize & 0x1F];
                    if (DATA_START..=DATA_START + CHANNEL_BYTES as u8).contains(&address) {
                        self.registers[CONTROL_SETUP as usize] &= !DATA_READY;
                    }
                    value
                }
                None => 0xFF,
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
            let mut addressed = None;
            for operation in operations {
                match operation {
                    Operation::Write(bytes) => {
                        if let Some(&first) = bytes.first() {
                            addressed = Some(first);
                            self.writes.push(bytes.to_vec());
                            if first == WRITE_REGISTER {
                                for &byte in &bytes[1..] {
                                    self.write_byte(byte);
                                }
                            }
                        }
                    }
                    Operation::Read(read) => {
                        let byte = match addressed {
                            Some(STATUS_REGISTER) => {
                                if self.read_address.is_some() {
                                    RX_VALID
                                } else {
                                    0
                                }
                            }
                            Some(READ_REGISTER) => self.read_byte(),
                            _ => 0xFF,
                        };
                        read[0] = byte;
                    }
                }
            }
            Ok(())
        }
    }

    fn sensor() -> As7262<Fake> {
        As7262::new(Fake::new(), ADDRESS).expect("an AS7262")
    }

    #[test]
    fn it_soft_resets_and_sets_the_integration_time() {
        let sensor = sensor();
        let writes: Vec<Vec<u8>> = sensor
            .i2c
            .writes
            .iter()
            .filter(|write| write.first() == Some(&WRITE_REGISTER))
            .cloned()
            .collect();
        assert_eq!(
            writes,
            [
                Vec::from([WRITE_REGISTER, CONTROL_SETUP | WRITE_MARKER]),
                Vec::from([WRITE_REGISTER, SOFT_RESET]),
                Vec::from([WRITE_REGISTER, INTEGRATION_TIME | WRITE_MARKER]),
                Vec::from([WRITE_REGISTER, INTEGRATION_TIME_DEFAULT]),
            ],
            "reset, then integration time, each as address and data"
        );
        assert_eq!(sensor.address(), ADDRESS);
        assert_eq!(sensor.address(), 0x49);
    }

    #[test]
    fn a_measurement_reads_the_six_channels() {
        let mut sensor = sensor();
        let spectrum = sensor.read().unwrap();
        assert_eq!(
            spectrum,
            Spectrum {
                violet: 0x0102,
                blue: 0x0304,
                green: 0x0506,
                yellow: 0x0708,
                orange: 0x090A,
                red: 0x0B0C,
            }
        );
        assert_eq!(
            sensor.i2c.registers[CONTROL_SETUP as usize] & (GAIN_16X | MODE_ONE_SHOT),
            GAIN_16X | MODE_ONE_SHOT,
            "16x gain and a one-shot conversion were selected"
        );
        assert_eq!(
            sensor.i2c.registers[CONTROL_SETUP as usize] & DATA_READY,
            0,
            "reading a data register cleared DATA_RDY"
        );
    }

    #[test]
    fn a_part_that_is_not_ready_is_busy() {
        let mut fake = Fake::new();
        fake.ready_after_conversion = false;
        let mut sensor = As7262::new(fake, ADDRESS).unwrap();
        assert_eq!(sensor.read().err(), Some(As7262Error::Busy));
    }

    #[test]
    fn a_soft_reset_clears_the_gain_and_the_mode() {
        let mut fake = Fake::new();
        fake.registers[CONTROL_SETUP as usize] = GAIN_16X | MODE_ONE_SHOT | DATA_READY;
        let mut sensor = As7262::new(fake, ADDRESS).unwrap();
        // The reset was the first write, so the control register is back to
        // its power-on value with no conversion pending.
        sensor.write_register(CONTROL_SETUP, SOFT_RESET).unwrap();
        assert_eq!(
            sensor.i2c.registers[CONTROL_SETUP as usize] & (GAIN_16X | DATA_READY),
            0,
            "the gain and the pending conversion are back to power-on"
        );
    }
}
