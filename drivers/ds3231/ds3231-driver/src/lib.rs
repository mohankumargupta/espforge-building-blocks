//! A Maxim DS3231 temperature-compensated real-time clock, over any
//! `embedded-hal` I2C bus.
//!
//! [`Ds3231::new`] clears the oscillator-stop flag so the part keeps time after
//! a power loss, and checks that the seven time registers decode: a part that
//! answers at 0x68 but holds no valid BCD is not a DS3231. [`Ds3231::datetime`]
//! then reads seconds, minutes, hours, the day-of-week counter, the date, the
//! month and the year, and [`Ds3231::temperature`] reads the die temperature.
//!
//! The registers hold binary-coded decimal, so the driver converts both ways:
//! [`Ds3231::set_datetime`] writes the same seven registers in one transaction.
//! The hour register is decoded in either of the chip's modes, 24-hour or
//! 12-hour with the PM flag.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: the alarms, the square-wave output, the aging
//! offset and the 32 kHz enable are left at their power-on values. See the
//! fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep a register
// file in an array.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The DS3231's only I2C address.
pub const ADDRESS: u8 = 0x68;

/// Register 0x00, and the seven time registers that follow it.
const SECONDS: u8 = 0x00;
const TIME_LEN: usize = 7;

/// Register 0x0F: the oscillator-stop flag is bit 7.
const STATUS: u8 = 0x0F;
const OSF: u8 = 0x80;

/// Registers 0x11 and 0x12: the die temperature, in quarter degrees.
const TEMPERATURE: u8 = 0x11;

/// The seconds register keeps the tenths in its top nibble; bit 7 is unused.
const SECONDS_MASK: u8 = 0x7F;
/// The hour register: bit 6 selects 12-hour mode, bit 5 is PM in that mode.
const HOUR_12: u8 = 0x40;
const HOUR_PM: u8 = 0x20;
/// The date register is two BCD digits; its top two bits are unused.
const DATE_MASK: u8 = 0x3F;
/// The month register: bit 7 is the century.
const MONTH_MASK: u8 = 0x1F;
const CENTURY: u8 = 0x80;

/// One date and time, as the chip keeps it.
///
/// `weekday` is the chip's day-of-week counter, 1 to 7: the DS3231 does not say
/// which day is which, so a firmware picks the convention it wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateTime {
    /// The full year, 2000 to 2199.
    pub year: u16,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
    /// 1 to 7, user-defined.
    pub weekday: u8,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
    /// 0 to 59.
    pub second: u8,
}

/// Why a transfer failed, or the part held something that is not a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ds3231Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The registers did not hold a valid binary-coded-decimal time, or the
    /// time handed to [`Ds3231::set_datetime`] is outside the field ranges.
    InvalidTime,
}

/// A DS3231 on an I2C bus.
pub struct Ds3231<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Ds3231<I2C> {
    /// Clear the oscillator-stop flag and check that the part keeps a time.
    ///
    /// OSF is set when the oscillator stopped, so a clock that was never set
    /// reads as invalid until one is written. Clearing it here means the part
    /// starts counting from whatever the registers hold.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Ds3231Error<I2C::Error>> {
        let mut this = Self { i2c, address };

        let mut status = [0u8; 1];
        this.i2c
            .write_read(address, &[STATUS], &mut status)
            .map_err(Ds3231Error::Bus)?;
        if status[0] & OSF != 0 {
            // OSF is cleared by writing a 0; the other bits are kept.
            this.i2c
                .write(address, &[STATUS, status[0] & !OSF])
                .map_err(Ds3231Error::Bus)?;
        }

        // 0x68 is a crowded address; a part that holds no BCD is not a DS3231.
        this.datetime()?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Read the seven time registers.
    pub fn datetime(&mut self) -> Result<DateTime, Ds3231Error<I2C::Error>> {
        let mut raw = [0u8; TIME_LEN];
        self.i2c
            .write_read(self.address, &[SECONDS], &mut raw)
            .map_err(Ds3231Error::Bus)?;
        decode(&raw).ok_or(Ds3231Error::InvalidTime)
    }

    /// Write the seven time registers in one transaction.
    pub fn set_datetime(&mut self, time: &DateTime) -> Result<(), Ds3231Error<I2C::Error>> {
        let registers = encode(time).ok_or(Ds3231Error::InvalidTime)?;
        let mut buffer = [0u8; TIME_LEN + 1];
        buffer[0] = SECONDS;
        buffer[1..].copy_from_slice(&registers);
        self.i2c
            .write(self.address, &buffer)
            .map_err(Ds3231Error::Bus)
    }

    /// The die temperature in degrees Celsius, in quarter-degree steps.
    pub fn temperature(&mut self) -> Result<f32, Ds3231Error<I2C::Error>> {
        let mut raw = [0u8; 2];
        self.i2c
            .write_read(self.address, &[TEMPERATURE], &mut raw)
            .map_err(Ds3231Error::Bus)?;
        // The integer part is signed; bits 7:6 of the second byte are the
        // quarter degrees.
        Ok(f32::from(raw[0] as i8) + f32::from(raw[1] >> 6) * 0.25)
    }
}

/// The seven time registers as a [`DateTime`], or `None` if any of them is not
/// a valid BCD field.
fn decode(raw: &[u8; TIME_LEN]) -> Option<DateTime> {
    let second = from_bcd(raw[0] & SECONDS_MASK)?;
    let minute = from_bcd(raw[1])?;
    let hour = if raw[2] & HOUR_12 != 0 {
        // 12-hour mode: bits 4:0 are 1 to 12, and 12 means midnight or noon.
        let hour = from_bcd(raw[2] & 0x1F)?;
        if !(1..=12).contains(&hour) {
            return None;
        }
        let hour = hour % 12;
        if raw[2] & HOUR_PM != 0 {
            hour + 12
        } else {
            hour
        }
    } else {
        from_bcd(raw[2] & DATE_MASK)?
    };
    let weekday = raw[3] & 0x07;
    let day = from_bcd(raw[4] & DATE_MASK)?;
    let month = from_bcd(raw[5] & MONTH_MASK)?;
    // The low two digits of the year are BCD; bit 7 of the month is the
    // century, so the part covers 2000 to 2199.
    let year = 2000
        + u16::from(from_bcd(raw[6])?)
        + if raw[5] & CENTURY != 0 { 100 } else { 0 };

    let valid = second <= 59
        && minute <= 59
        && hour <= 23
        && (1..=7).contains(&weekday)
        && (1..=31).contains(&day)
        && (1..=12).contains(&month);
    valid.then_some(DateTime {
        year,
        month,
        day,
        weekday,
        hour,
        minute,
        second,
    })
}

/// The seven time registers for `time`, in 24-hour mode, or `None` if a field
/// is outside its range.
fn encode(time: &DateTime) -> Option<[u8; TIME_LEN]> {
    let valid = (2000..=2199).contains(&time.year)
        && time.second <= 59
        && time.minute <= 59
        && time.hour <= 23
        && (1..=7).contains(&time.weekday)
        && (1..=31).contains(&time.day)
        && (1..=12).contains(&time.month);
    if !valid {
        return None;
    }
    Some([
        to_bcd(time.second),
        to_bcd(time.minute),
        // Bit 6 clear: 24-hour mode.
        to_bcd(time.hour),
        time.weekday,
        to_bcd(time.day),
        to_bcd(time.month) | if time.year >= 2100 { CENTURY } else { 0 },
        to_bcd((time.year % 100) as u8),
    ])
}

/// A binary-coded-decimal byte, or `None` for a nibble above nine.
fn from_bcd(value: u8) -> Option<u8> {
    let (high, low) = (value >> 4, value & 0x0F);
    (high <= 9 && low <= 9).then_some(high * 10 + low)
}

/// The binary value as two decimal digits.
fn to_bcd(value: u8) -> u8 {
    ((value / 10) << 4) | (value % 10)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to the 19 registers the driver touches.
    struct Fake {
        registers: [u8; 0x13],
        pointer: u8,
        writes: Vec<Vec<u8>>,
    }

    impl Fake {
        /// The power-on part: 2026-01-01 00:00:00, day 1, the oscillator-stop
        /// flag set, and 25.00 degrees on the die.
        fn new() -> Self {
            let mut registers = [0u8; 0x13];
            registers[0x03] = 0x01;
            registers[0x04] = 0x01;
            registers[0x05] = 0x01;
            registers[0x06] = 0x26;
            registers[0x0F] = 0x88;
            registers[0x11] = 0x19;
            Self {
                registers,
                pointer: 0,
                writes: Vec::new(),
            }
        }
    }

    impl ErrorType for Fake {
        type Error = Infallible;
    }

    impl I2c for Fake {
        /// A write's first byte is the register pointer; the rest are stored
        /// from there on. A read answers from the pointer on, incrementing.
        fn transaction(
            &mut self,
            _address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            for operation in operations {
                match operation {
                    Operation::Write(bytes) => {
                        if let Some(&first) = bytes.first() {
                            self.pointer = first;
                            self.writes.push(bytes.to_vec());
                            for (offset, &value) in bytes[1..].iter().enumerate() {
                                let register = usize::from(first) + offset;
                                self.registers[register] = value;
                            }
                        }
                    }
                    Operation::Read(read) => {
                        for (offset, byte) in read.iter_mut().enumerate() {
                            let register = usize::from(self.pointer) + offset;
                            *byte = self.registers[register];
                        }
                    }
                }
            }
            Ok(())
        }
    }

    fn sensor() -> Ds3231<Fake> {
        Ds3231::new(Fake::new(), ADDRESS).expect("a DS3231")
    }

    #[test]
    fn it_clears_the_oscillator_stop_flag() {
        let sensor = sensor();
        assert_eq!(sensor.address(), 0x68);
        assert_eq!(sensor.i2c.registers[0x0F], 0x08, "OSF cleared, 32 kHz kept");
        assert!(
            sensor.i2c.writes.contains(&Vec::from([STATUS, 0x08])),
            "the status register was written back"
        );
    }

    #[test]
    fn a_running_part_is_not_written_to() {
        let mut fake = Fake::new();
        fake.registers[0x0F] = 0x08;
        let sensor = Ds3231::new(fake, ADDRESS).unwrap();
        assert!(
            !sensor
                .i2c
                .writes
                .iter()
                .any(|write| write.len() > 1 && write[0] == STATUS),
            "OSF was already clear, so the status register was not written"
        );
    }

    #[test]
    fn it_reads_the_time_registers() {
        let mut sensor = sensor();
        assert_eq!(
            sensor.datetime().unwrap(),
            DateTime {
                year: 2026,
                month: 1,
                day: 1,
                weekday: 1,
                hour: 0,
                minute: 0,
                second: 0,
            }
        );
    }

    #[test]
    fn a_time_is_written_as_bcd() {
        let mut sensor = sensor();
        let time = DateTime {
            year: 2026,
            month: 11,
            day: 18,
            weekday: 3,
            hour: 12,
            minute: 34,
            second: 56,
        };
        sensor.set_datetime(&time).unwrap();
        assert_eq!(
            sensor.i2c.registers[0x00..0x07],
            [0x56, 0x34, 0x12, 0x03, 0x18, 0x11, 0x26],
            "second, minute, hour, day, date, month, year"
        );
        assert_eq!(
            sensor.i2c.writes.last(),
            Some(&Vec::from([0x00, 0x56, 0x34, 0x12, 0x03, 0x18, 0x11, 0x26])),
            "one transaction of the register pointer and seven bytes"
        );
        assert_eq!(sensor.datetime().unwrap(), time, "and it reads back");
    }

    #[test]
    fn the_century_is_bit_7_of_the_month() {
        let mut sensor = sensor();
        sensor
            .set_datetime(&DateTime {
                year: 2101,
                month: 2,
                day: 3,
                weekday: 4,
                hour: 5,
                minute: 6,
                second: 7,
            })
            .unwrap();
        assert_eq!(sensor.i2c.registers[0x05], 0x82, "February in the 2100s");
        assert_eq!(sensor.datetime().unwrap().year, 2101);
    }

    #[test]
    fn a_twelve_hour_register_is_decoded() {
        let mut sensor = sensor();
        sensor.i2c.registers[0x02] = 0x11 & 0x1F; // 11 in 12-hour mode, AM
        sensor.i2c.registers[0x02] |= 0x40;
        assert_eq!(sensor.datetime().unwrap().hour, 11);
        sensor.i2c.registers[0x02] = 0x40 | 0x20 | 0x11; // 11 PM
        assert_eq!(sensor.datetime().unwrap().hour, 23);
        sensor.i2c.registers[0x02] = 0x40 | 0x12; // 12 AM is midnight
        assert_eq!(sensor.datetime().unwrap().hour, 0);
    }

    #[test]
    fn a_part_that_holds_no_bcd_is_refused() {
        let mut fake = Fake::new();
        fake.registers[0x00] = 0x7A; // not BCD
        assert_eq!(
            Ds3231::new(fake, ADDRESS).err(),
            Some(Ds3231Error::InvalidTime)
        );
    }

    #[test]
    fn a_time_outside_the_field_ranges_is_refused() {
        let mut sensor = sensor();
        let mut time = DateTime {
            year: 2026,
            month: 13,
            day: 1,
            weekday: 1,
            hour: 0,
            minute: 0,
            second: 0,
        };
        assert_eq!(
            sensor.set_datetime(&time).err(),
            Some(Ds3231Error::InvalidTime)
        );
        time.month = 12;
        time.hour = 24;
        assert_eq!(
            sensor.set_datetime(&time).err(),
            Some(Ds3231Error::InvalidTime)
        );
    }

    #[test]
    fn the_temperature_is_quarter_degrees() {
        let mut sensor = sensor();
        assert_eq!(sensor.temperature().unwrap(), 25.0);
        sensor.i2c.registers[0x12] = 0x40;
        assert_eq!(sensor.temperature().unwrap(), 25.25);
        // -5.25 degrees: -6 whole degrees and three quarters of one.
        sensor.i2c.registers[0x11] = 0xFA;
        sensor.i2c.registers[0x12] = 0xC0;
        assert_eq!(sensor.temperature().unwrap(), -5.25);
    }
}
