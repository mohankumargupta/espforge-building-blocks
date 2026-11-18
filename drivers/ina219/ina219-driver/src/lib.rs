//! A TI INA219 current and power monitor, over any `embedded-hal` I2C bus.
//!
//! [`Ina219::new`] writes the configuration register and the calibration
//! register, which is what makes the part report amps and watts rather than
//! raw counts. [`Ina219::read`] then reads the four data registers in one
//! transaction and converts them with the datasheet's scale factors:
//!
//! - bus voltage: 4 mV per count, in the top 13 bits of its register,
//! - shunt voltage: 10 µV per signed count,
//! - current: 100 µA per signed count,
//! - power: 2 mW per count.
//!
//! The calibration is fixed for a 0.1 Ω shunt at a 100 µA current LSB, the
//! common breakout's arrangement: calibration `0.04096 / (0.0001 * 0.1)` is
//! 4096, and the power LSB is twenty times the current LSB.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: one continuous shunt-and-bus measurement with
//! the default 16 V range, ±320 mV gain and 12-bit 32-sample averaging, no
//! reset, no other ranges, no triggered or power-down modes, no alert pin.
//! See the fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep a register
// file in an array.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The part's address with both address pins low; 0x41 to 0x4F are the other
/// straps.
pub const ADDRESS: u8 = 0x40;

/// `configuration`: the mode, the ranges and the averaging.
const CONFIG: u8 = 0x00;
/// `shunt_voltage`, then `bus_voltage`, `power` and `current`: the four data
/// registers, contiguous from 0x01.
const DATA: u8 = 0x01;
const DATA_LEN: usize = 8;
/// `calibration`: what turns counts into amps.
const CALIBRATION: u8 = 0x05;

/// 16 V bus range, ±320 mV shunt range, 12-bit 32-sample averaging on both,
/// shunt and bus continuous.
const CONFIG_VALUE: u16 = 0x399F;
/// `0.04096 / (100 µA * 0.1 Ω)`.
const CALIBRATION_VALUE: u16 = 4096;

/// The shunt the calibration assumes.
pub const SHUNT_OHMS: f32 = 0.1;
/// The current one count stands for, in amps.
const CURRENT_LSB_A: f32 = 0.0001;
/// The power one count stands for, in watts: twenty times the current LSB.
const POWER_LSB_W: f32 = 20.0 * CURRENT_LSB_A;
/// The bus voltage one count stands for, in volts.
const BUS_VOLTAGE_LSB_V: f32 = 0.004;
/// The shunt voltage one count stands for, in volts.
const SHUNT_VOLTAGE_LSB_V: f32 = 0.000_01;

/// One sample.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    /// Volts across the load.
    pub bus_voltage_v: f32,
    /// Millivolts across the shunt.
    pub shunt_voltage_mv: f32,
    /// Milliamps through the shunt.
    pub current_ma: f32,
    /// Milliwatts into the load.
    pub power_mw: f32,
}

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ina219Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
}

/// An INA219 on an I2C bus.
pub struct Ina219<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Ina219<I2C> {
    /// Configure the part and load the calibration.
    ///
    /// The 1 ms the datasheet gives the part to come out of reset is not
    /// waited for: the emulator completes at once, and a real caller's next
    /// transaction is slow enough on its own.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Ina219Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.write_register(CONFIG, CONFIG_VALUE)?;
        this.write_register(CALIBRATION, CALIBRATION_VALUE)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Read the shunt voltage, bus voltage, power and current registers in
    /// one transaction and convert them.
    ///
    /// The conversion time of the chosen averaging is not waited for: the
    /// emulator answers at once, and a real caller's next transaction is slow
    /// enough on its own.
    pub fn read(&mut self) -> Result<Reading, Ina219Error<I2C::Error>> {
        let mut raw = [0u8; DATA_LEN];
        self.i2c
            .write_read(self.address, &[DATA], &mut raw)
            .map_err(Ina219Error::Bus)?;
        Ok(decode(&raw))
    }

    /// Write one 16-bit register, big-endian, after its address.
    fn write_register(&mut self, register: u8, value: u16) -> Result<(), Ina219Error<I2C::Error>> {
        let bytes = value.to_be_bytes();
        self.i2c
            .write(self.address, &[register, bytes[0], bytes[1]])
            .map_err(Ina219Error::Bus)
    }
}

/// The eight data bytes, shunt first, as physical units.
fn decode(raw: &[u8; DATA_LEN]) -> Reading {
    let shunt = i16::from_be_bytes([raw[0], raw[1]]);
    let bus = u16::from_be_bytes([raw[2], raw[3]]);
    let power = u16::from_be_bytes([raw[4], raw[5]]);
    let current = i16::from_be_bytes([raw[6], raw[7]]);
    Reading {
        // The top 13 bits are the conversion; the bottom three are the
        // overflow and conversion-ready flags.
        bus_voltage_v: f32::from(bus >> 3) * BUS_VOLTAGE_LSB_V,
        shunt_voltage_mv: f32::from(shunt) * SHUNT_VOLTAGE_LSB_V * 1000.0,
        current_ma: f32::from(current) * CURRENT_LSB_A * 1000.0,
        power_mw: f32::from(power) * POWER_LSB_W * 1000.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to its six 16-bit registers: a write's first byte is
    /// the register pointer, the bytes after it fill that register and the
    /// ones that follow, and a read answers from the pointer, two bytes per
    /// register.
    struct Fake {
        words: [u16; 6],
        pointer: u8,
        writes: Vec<Vec<u8>>,
    }

    impl Fake {
        /// 5.00 V, 0.10 A through the 0.1 Ω shunt: 10.00 mV, 500 mW.
        fn new() -> Self {
            let mut words = [0u16; 6];
            words[1] = 1000;
            words[2] = (1250 << 3) | 0b010;
            words[3] = 250;
            words[4] = 1000;
            Self {
                words,
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

    fn monitor() -> Ina219<Fake> {
        Ina219::new(Fake::new(), ADDRESS).expect("an INA219")
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.01
    }

    #[test]
    fn it_configures_and_calibrates_the_part() {
        let monitor = monitor();
        assert_eq!(monitor.address(), 0x40);
        assert_eq!(
            monitor.i2c.writes,
            [
                Vec::from([CONFIG, 0x39, 0x9F]),
                Vec::from([CALIBRATION, 0x10, 0x00]),
            ],
            "the configuration, then the calibration"
        );
    }

    #[test]
    fn the_configuration_is_the_datasheets_continuous_16_v_mode() {
        assert_eq!(CONFIG_VALUE, 0x399F);
        // 16 V range (bit 13 clear), ±320 mV gain, 12-bit 32-sample averages,
        // shunt and bus continuous.
        assert_eq!(CONFIG_VALUE & 0x8000, 0, "no reset");
        assert_eq!((CONFIG_VALUE >> 11) & 0x03, 0b11, "gain /8");
        assert_eq!(CONFIG_VALUE & 0x07, 0b111, "continuous");
    }

    #[test]
    fn the_calibration_matches_a_tenth_ohm_shunt() {
        // 0.04096 / (100 µA * 0.1 Ω), truncated.
        assert_eq!(CALIBRATION_VALUE, 4096);
    }

    #[test]
    fn a_read_decodes_the_four_data_registers() {
        let mut monitor = monitor();
        let reading = monitor.read().unwrap();
        assert!(close(reading.bus_voltage_v, 5.0), "{reading:?}");
        assert!(close(reading.shunt_voltage_mv, 10.0), "{reading:?}");
        assert!(close(reading.current_ma, 100.0), "{reading:?}");
        assert!(close(reading.power_mw, 500.0), "{reading:?}");
        assert_eq!(
            monitor.i2c.writes.last(),
            Some(&Vec::from([DATA])),
            "a read is one pointer write and one eight-byte read"
        );
    }

    #[test]
    fn the_bus_voltage_ignores_the_flag_bits() {
        let mut monitor = monitor();
        // 12 V at 4 mV per count is 3000 counts, and the conversion-ready
        // flag is bit 1.
        monitor.i2c.words[2] = (3000 << 3) | 0b010;
        assert!(close(monitor.read().unwrap().bus_voltage_v, 12.0));
    }

    #[test]
    fn the_shunt_and_current_registers_are_signed() {
        let mut monitor = monitor();
        monitor.i2c.words[1] = -2500i16 as u16;
        monitor.i2c.words[4] = -2500i16 as u16;
        let reading = monitor.read().unwrap();
        assert!(close(reading.shunt_voltage_mv, -25.0), "{reading:?}");
        assert!(close(reading.current_ma, -250.0), "{reading:?}");
    }

    #[test]
    fn the_scale_is_the_datasheets() {
        assert!(close(SHUNT_OHMS, 0.1));
        assert!(close(CURRENT_LSB_A * 20.0, POWER_LSB_W));
    }
}
