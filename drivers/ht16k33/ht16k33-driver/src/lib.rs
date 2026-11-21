//! A Holtek HT16K33 LED matrix and alphanumeric display driver, over any
//! `embedded-hal` I2C bus.
//!
//! The part is command-addressed: the first byte of a write is a command and
//! the bytes after it are that command's parameters. [`Ht16k33::new`] turns the
//! internal oscillator on (`0x21`) and the display on with no blinking
//! (`0x81`). After that
//!
//! - [`Ht16k33::write_ram`] loads the 16-byte display RAM (`0x00` plus the
//!   frame), which is 8 rows of 16 columns as two bytes per row,
//! - [`Ht16k33::read_ram`] reads the same RAM back,
//! - [`Ht16k33::set_brightness`] sets the dimming level (`0xE0 | level`).
//!
//! The RAM address auto-increments and wraps at the end of the 16 bytes, so one
//! write loads a whole frame and a read past the end starts over.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: no blinking, no keyscan, no interrupt output,
//! no 1/16-duty or external-clock variants. See the fixture's `README.md` for
//! what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the command
// log in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with all three address jumpers low; 0x71 to 0x77 are the
/// other straps.
pub const ADDRESS: u8 = 0x70;

/// The display RAM: 16 bytes, two per row.
pub const RAM_BYTES: usize = 16;

/// The dimming levels the part has.
pub const MAX_BRIGHTNESS: u8 = 15;

/// System setup: the internal oscillator on.
const OSCILLATOR_ON: u8 = 0x21;
/// Display setup: display on, no blinking.
const DISPLAY_ON: u8 = 0x81;
/// The dimming command; the low nibble is the level.
const DIMMING: u8 = 0xE0;
/// Display RAM address 0x00, the first of the sixteen bytes.
const RAM_START: u8 = 0x00;

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ht16k33Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// An HT16K33 on an I2C bus.
pub struct Ht16k33<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Ht16k33<I2C> {
    /// Start the part up: oscillator on, display on, no blinking.
    ///
    /// The two setup commands are all the part needs before it can display;
    /// the RAM keeps its power-on zeros until a frame is written.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Ht16k33Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.command(&[OSCILLATOR_ON])?;
        this.command(&[DISPLAY_ON])?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Set the dimming level, 0 (dim) to 15 (bright).
    ///
    /// A level above 15 is clamped, since the command's low nibble holds it.
    pub fn set_brightness(&mut self, level: u8) -> Result<(), Ht16k33Error<I2C::Error>> {
        self.command(&[DIMMING | level.min(MAX_BRIGHTNESS)])
    }

    /// Load a whole frame into the display RAM.
    ///
    /// The frame is the part's 16 bytes, two per row: the first is row 0's
    /// left eight columns, the second its right eight.
    pub fn write_ram(&mut self, ram: &[u8; RAM_BYTES]) -> Result<(), Ht16k33Error<I2C::Error>> {
        let mut frame = [0u8; RAM_BYTES + 1];
        frame[0] = RAM_START;
        frame[1..].copy_from_slice(ram);
        self.command(&frame)
    }

    /// Read the display RAM back.
    pub fn read_ram(&mut self) -> Result<[u8; RAM_BYTES], Ht16k33Error<I2C::Error>> {
        self.command(&[RAM_START])?;
        let mut ram = [0u8; RAM_BYTES];
        self.i2c
            .read(self.address, &mut ram)
            .map_err(Ht16k33Error::Bus)?;
        Ok(ram)
    }

    /// One command write: the command byte and its parameters.
    fn command(&mut self, bytes: &[u8]) -> Result<(), Ht16k33Error<I2C::Error>> {
        self.i2c
            .write(self.address, bytes)
            .map_err(Ht16k33Error::Bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to its command byte and its RAM.
    struct Fake {
        ram: [u8; RAM_BYTES],
        writes: Vec<Vec<u8>>,
        reads: Vec<usize>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                ram: [0u8; RAM_BYTES],
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
                        // The RAM command carries the address and then the
                        // frame, auto-incrementing and wrapping.
                        if let Some((&address, data)) = bytes.split_first() {
                            if address < RAM_BYTES as u8 {
                                for (offset, byte) in data.iter().enumerate() {
                                    let at = (usize::from(address) + offset) % RAM_BYTES;
                                    self.ram[at] = *byte;
                                }
                            }
                        }
                    }
                    Operation::Read(read) => {
                        self.reads.push(read.len());
                        let n = read.len().min(RAM_BYTES);
                        read[..n].copy_from_slice(&self.ram[..n]);
                    }
                }
            }
            Ok(())
        }
    }

    fn display() -> Ht16k33<Fake> {
        Ht16k33::new(Fake::new(), ADDRESS).expect("an HT16K33")
    }

    /// A frame with a different byte in every position.
    fn frame() -> [u8; RAM_BYTES] {
        let mut ram = [0u8; RAM_BYTES];
        for (index, byte) in ram.iter_mut().enumerate() {
            *byte = index as u8 | 0x80;
        }
        ram
    }

    #[test]
    fn it_starts_the_oscillator_and_the_display() {
        let display = display();
        assert_eq!(display.address(), 0x70);
        assert_eq!(
            display.i2c.writes,
            [std::vec![OSCILLATOR_ON], std::vec![DISPLAY_ON]],
            "oscillator, then display, and nothing else"
        );
    }

    #[test]
    fn a_frame_is_the_ram_command_and_sixteen_bytes() {
        let mut display = display();
        display.write_ram(&frame()).unwrap();
        let written = display.i2c.writes.last().unwrap();
        assert_eq!(written.len(), RAM_BYTES + 1);
        assert_eq!(written[0], RAM_START, "the RAM address");
        assert_eq!(&written[1..], &frame(), "the frame, unchanged");
        assert_eq!(display.i2c.ram, frame(), "the part took it");
    }

    #[test]
    fn a_frame_reads_back() {
        let mut display = display();
        display.write_ram(&frame()).unwrap();
        display.i2c.reads.clear();
        assert_eq!(display.read_ram().unwrap(), frame());
        assert_eq!(
            display.i2c.writes.last(),
            Some(&std::vec![RAM_START]),
            "the read points the part at RAM 0 first"
        );
        assert_eq!(display.i2c.reads, [RAM_BYTES]);
    }

    #[test]
    fn the_brightness_is_the_dimming_command() {
        let mut display = display();
        display.set_brightness(3).unwrap();
        assert_eq!(display.i2c.writes.last(), Some(&std::vec![0xE3]));

        display.set_brightness(200).unwrap();
        assert_eq!(
            display.i2c.writes.last(),
            Some(&std::vec![0xE0 | MAX_BRIGHTNESS]),
            "the level is clamped to the nibble"
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

        assert_eq!(
            Ht16k33::new(Broken, ADDRESS).err(),
            Some(Ht16k33Error::Bus(ErrorKind::Other))
        );
    }
}
