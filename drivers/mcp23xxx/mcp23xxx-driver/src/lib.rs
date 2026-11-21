//! A Microchip MCP23017 16-bit I2C I/O expander, over any `embedded-hal` I2C
//! bus.
//!
//! The part has two 8-bit ports behind a register file addressed by one byte,
//! and the pointer auto-increments, so a multi-byte write reaches consecutive
//! registers in one transaction. In the power-on `IOCON.BANK = 0` layout the
//! registers relevant here are
//!
//! - `0x00`/`0x01` IODIRA/IODIRB: a set bit is an input,
//! - `0x0C`/`0x0D` GPPUA/GPPUB: the 100 kΩ pull-ups on input pins,
//! - `0x12`/`0x13` GPIOA/GPIOB: the pins as they read,
//! - `0x14`/`0x15` OLATA/OLATB: the output latches.
//!
//! [`Mcp23xxx::new`] makes all sixteen pins outputs, in one transaction, and
//! then [`Mcp23xxx::write_outputs`], [`Mcp23xxx::read_inputs`],
//! [`Mcp23xxx::set_direction`] and [`Mcp23xxx::set_pull_ups`] work per port.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: no interrupt outputs, no polarity inversion, no
//! `IOCON` modes, no hardware-address variants of the register file, and the
//! SPI variants of the family. See the fixture's `README.md` for what is left
//! out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the write log
// in a vector.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with every address pin low; 0x20 to 0x27 are the straps.
pub const ADDRESS: u8 = 0x20;

/// The direction register of port A: a set bit is an input.
const IODIR: u8 = 0x00;
/// The pull-up register of port A; port B follows it.
const GPPU: u8 = 0x0C;
/// The pin register of port A; port B follows it.
const GPIO: u8 = 0x12;
/// The output latch of port A; port B follows it.
const OLAT: u8 = 0x14;

/// One of the part's two 8-bit ports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Port {
    /// Port A, PA0 to PA7.
    A,
    /// Port B, PB0 to PB7.
    B,
}

impl Port {
    /// The register offset of this port within each A/B pair.
    const fn offset(self) -> u8 {
        match self {
            Port::A => 0,
            Port::B => 1,
        }
    }
}

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mcp23xxxError<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// An MCP23017 on an I2C bus.
pub struct Mcp23xxx<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Mcp23xxx<I2C> {
    /// Configure all sixteen pins as outputs.
    ///
    /// Both direction registers are written in one transaction, which the
    /// part's auto-incrementing pointer walks. The output latches keep their
    /// power-on zeros, so the pins come up driven low.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Mcp23xxxError<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.write(&[IODIR, 0x00, 0x00])?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Make the port's pins inputs (a set bit) or outputs (a cleared bit).
    pub fn set_direction(
        &mut self,
        port: Port,
        direction: u8,
    ) -> Result<(), Mcp23xxxError<I2C::Error>> {
        self.write(&[IODIR + port.offset(), direction])
    }

    /// Turn the port's 100 kΩ pull-ups on where `mask` is set.
    ///
    /// The pull-ups act on input pins only, so a pin that drives a load as an
    /// output ignores them.
    pub fn set_pull_ups(
        &mut self,
        port: Port,
        mask: u8,
    ) -> Result<(), Mcp23xxxError<I2C::Error>> {
        self.write(&[GPPU + port.offset(), mask])
    }

    /// Drive the port's output pins from its latch.
    ///
    /// Writing the latch rather than GPIO keeps a read of the port from
    /// disturbing the output pins, as it would on the part.
    pub fn write_outputs(
        &mut self,
        port: Port,
        outputs: u8,
    ) -> Result<(), Mcp23xxxError<I2C::Error>> {
        self.write(&[OLAT + port.offset(), outputs])
    }

    /// Read the port's pins: an output pin reads back its latch, an input pin
    /// the level on the pin.
    pub fn read_inputs(&mut self, port: Port) -> Result<u8, Mcp23xxxError<I2C::Error>> {
        self.write(&[GPIO + port.offset()])?;
        let mut pins = [0u8; 1];
        self.i2c
            .read(self.address, &mut pins)
            .map_err(Mcp23xxxError::Bus)?;
        Ok(pins[0])
    }

    /// One register write: the register index and its value.
    fn write(&mut self, bytes: &[u8]) -> Result<(), Mcp23xxxError<I2C::Error>> {
        self.i2c
            .write(self.address, bytes)
            .map_err(Mcp23xxxError::Bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorKind, ErrorType, Operation};
    use std::vec::Vec;

    /// The register file the driver can reach: the pointer walks it, so a write
    /// followed by a read answers like the part.
    const REGISTERS: u8 = 0x16;

    /// The part reduced to its register file and a write log.
    struct Fake {
        regs: [u8; REGISTERS as usize],
        pointer: u8,
        writes: Vec<Vec<u8>>,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                regs: [0u8; REGISTERS as usize],
                pointer: 0,
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
                        if let Some((&register, data)) = bytes.split_first() {
                            self.pointer = register;
                            for byte in data {
                                self.regs[usize::from(self.pointer)] = *byte;
                                self.pointer = (self.pointer + 1) % REGISTERS;
                            }
                        }
                    }
                    Operation::Read(read) => {
                        for slot in read.iter_mut() {
                            *slot = self.regs[usize::from(self.pointer)];
                            self.pointer = (self.pointer + 1) % REGISTERS;
                        }
                    }
                }
            }
            Ok(())
        }
    }

    fn expander() -> Mcp23xxx<Fake> {
        Mcp23xxx::new(Fake::new(), ADDRESS).expect("an MCP23017")
    }

    #[test]
    fn it_makes_every_pin_an_output() {
        let expander = expander();
        assert_eq!(expander.address(), 0x20);
        assert_eq!(
            expander.i2c.writes,
            [Vec::from([IODIR, 0x00, 0x00])],
            "both direction registers in one transaction, every bit cleared"
        );
    }

    #[test]
    fn a_direction_write_reaches_the_right_port() {
        let mut expander = expander();
        expander.set_direction(Port::B, 0x7F).unwrap();
        assert_eq!(
            expander.i2c.writes.last(),
            Some(&Vec::from([0x01, 0x7F])),
            "port B is the second direction register"
        );
        expander.set_direction(Port::A, 0xF0).unwrap();
        assert_eq!(
            expander.i2c.writes.last(),
            Some(&Vec::from([0x00, 0xF0])),
            "port A is the first"
        );
    }

    #[test]
    fn an_output_write_sets_the_latch() {
        let mut expander = expander();
        expander.write_outputs(Port::B, 0x5A).unwrap();
        assert_eq!(expander.i2c.writes.last(), Some(&Vec::from([0x15, 0x5A])));
        assert_eq!(
            expander.i2c.regs[0x15], 0x5A,
            "the latch, not the pin register"
        );
    }

    #[test]
    fn the_inputs_are_read_from_the_pin_register() {
        let mut expander = expander();
        expander.i2c.regs[0x12] = 0x3C;
        expander.i2c.regs[0x13] = 0xA5;
        assert_eq!(expander.read_inputs(Port::A).unwrap(), 0x3C);
        assert_eq!(
            expander.i2c.writes.last(),
            Some(&Vec::from([0x12])),
            "the read points the part at the port first"
        );
        assert_eq!(expander.read_inputs(Port::B).unwrap(), 0xA5);
        assert_eq!(expander.i2c.writes.last(), Some(&Vec::from([0x13])));
    }

    #[test]
    fn the_pull_ups_are_the_gppu_register() {
        let mut expander = expander();
        expander.set_pull_ups(Port::B, 0xF0).unwrap();
        assert_eq!(
            expander.i2c.writes.last(),
            Some(&Vec::from([0x0D, 0xF0])),
            "GPPUB"
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
            Mcp23xxx::new(Broken, ADDRESS).err(),
            Some(Mcp23xxxError::Bus(ErrorKind::Other))
        );
    }
}
