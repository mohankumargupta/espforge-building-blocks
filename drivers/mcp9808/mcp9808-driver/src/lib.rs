//! A Microchip MCP9808 temperature sensor, over any `embedded-hal` I2C bus.
//!
//! [`Mcp9808::new`] reads the manufacturer and device identification registers
//! and sets the resolution to 0.0625 °C, the part's finest and its power-on
//! value. [`Mcp9808::read`] then reads the ambient temperature register and
//! decodes it: the low 13 bits are two's complement hundredths of a degree,
//! 0.0625 °C per count, and the top three bits are the alert flags.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: the default continuous conversion, comparator
//! output, alerts off and 0.0625 °C resolution, one ambient reading at a time,
//! no alert limits, no shutdown, no critical-lock, no `init="deferred"`. See
//! the fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep a register
// file in an array.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with all three address pins low; 0x19 to 0x1F are the
/// other straps.
pub const ADDRESS: u8 = 0x18;

/// `ambient_temperature`: the measurement, and the alert flags above it.
const AMBIENT: u8 = 0x05;
/// `manufacturer_id`, 0x0054.
const MANUFACTURER: u8 = 0x06;
/// `device_id`, 0x0400.
const DEVICE: u8 = 0x07;
/// `resolution`: 0.0625 °C per count when it reads 0b11.
const RESOLUTION: u8 = 0x08;

const MANUFACTURER_ID: u16 = 0x0054;
const DEVICE_ID: u16 = 0x0400;
/// 0.0625 °C per count, the part's power-on resolution.
const RESOLUTION_00625: u16 = 0x0003;

/// The temperature is the low 13 bits; bit 12 is its sign.
const TEMPERATURE_MASK: u16 = 0x1FFF;
const SIGN_BIT: u16 = 0x1000;
/// One count of the temperature register.
const TEMPERATURE_LSB_C: f32 = 0.0625;

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mcp9808Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The identification registers are not an MCP9808's.
    UnknownDevice { manufacturer: u16, device: u16 },
}

/// An MCP9808 on an I2C bus.
pub struct Mcp9808<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Mcp9808<I2C> {
    /// Identify the part and set its resolution.
    ///
    /// A part that answers with another manufacturer or device id is rejected
    /// rather than read as if it were an MCP9808.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Mcp9808Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        let manufacturer = this.read_register(MANUFACTURER)?;
        let device = this.read_register(DEVICE)?;
        if manufacturer != MANUFACTURER_ID || device != DEVICE_ID {
            return Err(Mcp9808Error::UnknownDevice {
                manufacturer,
                device,
            });
        }
        this.write_register(RESOLUTION, RESOLUTION_00625)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// The ambient temperature, in degrees Celsius.
    ///
    /// The conversion time of the chosen resolution is not waited for: the
    /// emulator answers at once, and a real caller's next transaction is slow
    /// enough on its own.
    pub fn read(&mut self) -> Result<f32, Mcp9808Error<I2C::Error>> {
        Ok(decode(self.read_register(AMBIENT)?))
    }

    /// Read one 16-bit register, big-endian, from its address.
    fn read_register(&mut self, register: u8) -> Result<u16, Mcp9808Error<I2C::Error>> {
        let mut bytes = [0u8; 2];
        self.i2c
            .write_read(self.address, &[register], &mut bytes)
            .map_err(Mcp9808Error::Bus)?;
        Ok(u16::from_be_bytes(bytes))
    }

    /// Write one 16-bit register, big-endian, after its address.
    fn write_register(&mut self, register: u8, value: u16) -> Result<(), Mcp9808Error<I2C::Error>> {
        let bytes = value.to_be_bytes();
        self.i2c
            .write(self.address, &[register, bytes[0], bytes[1]])
            .map_err(Mcp9808Error::Bus)
    }
}

/// The register's low 13 bits as degrees Celsius: two's complement counts of
/// 0.0625, with the alert flags in the top three bits ignored.
fn decode(raw: u16) -> f32 {
    let temperature = raw & TEMPERATURE_MASK;
    let counts = if temperature & SIGN_BIT != 0 {
        i32::from(temperature) - 8192
    } else {
        i32::from(temperature)
    };
    counts as f32 * TEMPERATURE_LSB_C
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to its 16-bit registers: a write's first byte is the
    /// register pointer, the bytes after it fill that register and the ones
    /// that follow, and a read answers from the pointer, two bytes per
    /// register.
    struct Fake {
        words: [u16; 9],
        pointer: u8,
        writes: Vec<Vec<u8>>,
        reads: usize,
    }

    impl Fake {
        /// An MCP9808 at 24.00 °C, its identification and resolution.
        fn new() -> Self {
            let mut words = [0u16; 9];
            words[usize::from(AMBIENT)] = 24 * 16;
            words[usize::from(MANUFACTURER)] = MANUFACTURER_ID;
            words[usize::from(DEVICE)] = DEVICE_ID;
            words[usize::from(RESOLUTION)] = 0x0003;
            Self {
                words,
                pointer: 0,
                writes: Vec::new(),
                reads: 0,
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
                        if let Some((&register, values)) = bytes.split_first() {
                            self.pointer = register;
                            for (offset, &value) in values.iter().enumerate() {
                                let index = (usize::from(register) + offset / 2) % self.words.len();
                                let word = &mut self.words[index];
                                if offset % 2 == 0 {
                                    *word = (*word & 0x00FF) | (u16::from(value) << 8);
                                } else {
                                    *word = (*word & 0xFF00) | u16::from(value);
                                }
                            }
                        }
                    }
                    Operation::Read(read) => {
                        self.reads += 1;
                        let start = usize::from(self.pointer);
                        for (offset, byte) in read.iter_mut().enumerate() {
                            let word = self.words[(start + offset / 2) % self.words.len()];
                            *byte = if offset % 2 == 0 {
                                (word >> 8) as u8
                            } else {
                                word as u8
                            };
                        }
                    }
                }
            }
            Ok(())
        }
    }

    fn sensor() -> Mcp9808<Fake> {
        Mcp9808::new(Fake::new(), ADDRESS).expect("an MCP9808")
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.01
    }

    #[test]
    fn it_identifies_the_part_and_sets_the_resolution() {
        let sensor = sensor();
        assert_eq!(sensor.address(), 0x18);
        assert_eq!(
            sensor.i2c.writes.last(),
            Some(&Vec::from([RESOLUTION, 0x00, 0x03])),
            "0.0625 °C per count, nothing else written"
        );
        assert_eq!(
            sensor.i2c
                .writes
                .iter()
                .filter(|write| write.len() > 1)
                .count(),
            1,
            "the only multi-byte write is the resolution"
        );
        assert_eq!(sensor.i2c.reads, 2, "the manufacturer and device id");
    }

    #[test]
    fn another_device_is_rejected() {
        let mut fake = Fake::new();
        fake.words[usize::from(DEVICE)] = 0x0401;
        assert_eq!(
            Mcp9808::new(fake, ADDRESS).err(),
            Some(Mcp9808Error::UnknownDevice {
                manufacturer: MANUFACTURER_ID,
                device: 0x0401,
            })
        );
    }

    #[test]
    fn a_read_returns_the_ambient_temperature() {
        let mut sensor = sensor();
        let temperature = sensor.read().unwrap();
        assert!(close(temperature, 24.0), "{temperature}");
        assert_eq!(
            sensor.i2c.writes.last(),
            Some(&Vec::from([AMBIENT])),
            "a read is one pointer write and one two-byte read"
        );
    }

    #[test]
    fn a_negative_temperature_is_two_s_complement() {
        let mut sensor = sensor();
        // -25.00 °C is -400 counts, 0x1E70 in the low 13 bits.
        sensor.i2c.words[usize::from(AMBIENT)] = 0x1E70;
        let temperature = sensor.read().unwrap();
        assert!(close(temperature, -25.0), "{temperature}");
    }

    #[test]
    fn the_alert_flags_are_ignored() {
        let mut sensor = sensor();
        // 24.00 °C with all three alert flags set above the temperature.
        sensor.i2c.words[usize::from(AMBIENT)] = 0x0180 | 0xE000;
        let temperature = sensor.read().unwrap();
        assert!(close(temperature, 24.0), "{temperature}");
    }

    #[test]
    fn the_scale_is_the_datasheets() {
        assert!(close(TEMPERATURE_LSB_C, 1.0 / 16.0));
        assert_eq!(TEMPERATURE_MASK & SIGN_BIT, SIGN_BIT);
    }
}
