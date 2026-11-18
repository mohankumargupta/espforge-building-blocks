//! A Texas Instruments ADS1015 and ADS1115 analog-to-digital converter, over
//! any `embedded-hal` I2C bus.
//!
//! The chip's I2C protocol is a pointer register: write the register number,
//! then read or write its 16-bit big-endian value. There is no identity
//! register, so [`Ads1x15::new`] reads the configuration register to prove
//! something answers at the address.
//!
//! One conversion is two transactions: the configuration register is written
//! with the chosen input, gain and single-shot mode (bit 15 starts the
//! conversion), then the two bytes of the conversion register at `0x00` are
//! read back and scaled. [`Input`] selects a single-ended channel against
//! ground or one of the four differential pairs the multiplexer offers;
//! [`Gain`] selects the full-scale range.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.

#![no_std]

use embedded_hal::i2c::I2c;

/// `conversion`: the last result, 16-bit two's complement.
const CONVERSION: u8 = 0x00;
/// `config`: operational status, input, gain, mode, data rate, comparator.
const CONFIG: u8 = 0x01;

/// `OS` (bit 15): write 1 to start a single conversion; reads 1 when idle.
const OS: u16 = 0x8000;
/// `MODE` (bit 8): 1 is single-shot, 0 continuous.
const MODE_SINGLE: u16 = 0x0100;
/// `DR` (bits 7:5): 100 is 128 samples per second, the power-on rate.
const DATA_RATE_128_SPS: u16 = 0x0080;
/// `COMP_QUE` (bits 1:0): 11 disables the comparator, as at power-on.
const COMPARATOR_DISABLED: u16 = 0x0003;

/// The power-on configuration: single-shot, AIN0-AIN1, ±2.048 V, 128 SPS,
/// comparator off (datasheet reset value 0x8583).
pub const DEFAULT_CONFIG: u16 = 0x8583;

/// Why a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ads1x15Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The multiplexer cannot select that differential pair.
    UnsupportedInput,
}

/// The full-scale range, as the configuration register's `PGA` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Gain {
    /// `000`: ±6.144 V.
    SixVolt144,
    /// `001`: ±4.096 V.
    FourVolt096,
    /// `010`: ±2.048 V, the power-on range.
    #[default]
    TwoVolt048,
    /// `011`: ±1.024 V.
    OneVolt024,
    /// `100`: ±0.512 V.
    ZeroVolt512,
    /// `101`: ±0.256 V.
    ZeroVolt256,
}

impl Gain {
    /// The `PGA` field value.
    fn bits(self) -> u16 {
        match self {
            Gain::SixVolt144 => 0b000,
            Gain::FourVolt096 => 0b001,
            Gain::TwoVolt048 => 0b010,
            Gain::OneVolt024 => 0b011,
            Gain::ZeroVolt512 => 0b100,
            Gain::ZeroVolt256 => 0b101,
        }
    }

    /// The range the input may swing over, in volts either side of ground.
    pub fn full_scale_volts(self) -> f32 {
        match self {
            Gain::SixVolt144 => 6.144,
            Gain::FourVolt096 => 4.096,
            Gain::TwoVolt048 => 2.048,
            Gain::OneVolt024 => 1.024,
            Gain::ZeroVolt512 => 0.512,
            Gain::ZeroVolt256 => 0.256,
        }
    }
}

/// The chip, which decides the resolution of a conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Chip {
    /// 12-bit: the conversion register holds the code in its top 12 bits.
    Ads1015,
    /// 16-bit, the part the emulator's virtual ADC models.
    #[default]
    Ads1115,
}

impl Chip {
    /// The count a full-scale input produces: half the range, since the other
    /// half is negative.
    fn counts(self) -> f32 {
        match self {
            Chip::Ads1015 => 2_048.0,
            Chip::Ads1115 => 32_768.0,
        }
    }

    /// The size of one count at `gain`, in volts.
    pub fn lsb_volts(self, gain: Gain) -> f32 {
        gain.full_scale_volts() / self.counts()
    }

    /// The conversion register as a signed count.
    fn count_from_register(self, word: u16) -> i16 {
        match self {
            // The 12-bit code sits in the top bits, so the low four are zero.
            Chip::Ads1015 => (word as i16) >> 4,
            Chip::Ads1115 => word as i16,
        }
    }
}

/// What the multiplexer measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// One channel against ground, `0..=3` for AIN0..=AIN3.
    SingleEnded(u8),
    /// One of the four pairs the multiplexer offers: 0-1, 0-3, 1-3 or 2-3.
    Differential { positive: u8, negative: u8 },
}

impl Input {
    /// The `MUX` field value, or `None` for a pair the chip cannot select.
    fn mux(self) -> Option<u16> {
        match self {
            Input::SingleEnded(channel) if channel < 4 => Some(0b100 + channel as u16),
            Input::Differential {
                positive: 0,
                negative: 1,
            } => Some(0b000),
            Input::Differential {
                positive: 0,
                negative: 3,
            } => Some(0b001),
            Input::Differential {
                positive: 1,
                negative: 3,
            } => Some(0b010),
            Input::Differential {
                positive: 2,
                negative: 3,
            } => Some(0b011),
            _ => None,
        }
    }
}

/// One conversion, as the chip's own signed count and in volts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub raw: i16,
    pub volts: f32,
}

/// An ADS1015 or ADS1115 on an I2C bus.
pub struct Ads1x15<I2C> {
    i2c: I2C,
    address: u8,
    chip: Chip,
}

impl<I2C: I2c> Ads1x15<I2C> {
    /// Read the configuration register to prove the part answers, and leave
    /// it at its power-on configuration.
    ///
    /// There is no identity register to check, so a device at another address
    /// is caught only if it rejects the read.
    pub fn new(i2c: I2C, address: u8, chip: Chip) -> Result<Self, Ads1x15Error<I2C::Error>> {
        let mut this = Self { i2c, address, chip };
        this.read_register(CONFIG)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// The chip the driver decodes for.
    pub fn chip(&self) -> Chip {
        self.chip
    }

    /// Start one conversion on `input` at `gain` and read it back.
    ///
    /// The configuration write sets `OS`, which starts a single-shot
    /// conversion. There is no `OS` poll: the emulator converts immediately,
    /// while on real hardware the first read after the write can still be the
    /// previous sample (at most one data-rate period early).
    pub fn read(&mut self, input: Input, gain: Gain) -> Result<Reading, Ads1x15Error<I2C::Error>> {
        let mux = input.mux().ok_or(Ads1x15Error::UnsupportedInput)?;
        let config = OS
            | (mux << 12)
            | (gain.bits() << 9)
            | MODE_SINGLE
            | DATA_RATE_128_SPS
            | COMPARATOR_DISABLED;
        self.write_register(CONFIG, config)?;
        let word = self.read_register(CONVERSION)?;
        let raw = self.chip.count_from_register(word);
        Ok(Reading {
            raw,
            volts: raw as f32 * self.chip.lsb_volts(gain),
        })
    }

    /// One 16-bit big-endian register: the pointer, then a repeated start.
    fn read_register(&mut self, register: u8) -> Result<u16, Ads1x15Error<I2C::Error>> {
        let mut bytes = [0u8; 2];
        self.i2c
            .write_read(self.address, &[register], &mut bytes)
            .map_err(Ads1x15Error::Bus)?;
        Ok(u16::from_be_bytes(bytes))
    }

    /// One 16-bit big-endian register write: the pointer, then two values.
    fn write_register(&mut self, register: u8, value: u16) -> Result<(), Ads1x15Error<I2C::Error>> {
        let bytes = value.to_be_bytes();
        self.i2c
            .write(self.address, &[register, bytes[0], bytes[1]])
            .map_err(Ads1x15Error::Bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};

    /// The chip reduced to its four 16-bit registers: a pointer write, then
    /// big-endian bytes.
    struct Fake {
        words: [u16; 4],
        pointer: u8,
    }

    impl Fake {
        /// A part whose conversion register already holds `conversion`.
        fn new(conversion: u16) -> Self {
            Self {
                words: [conversion, DEFAULT_CONFIG, 0x8000, 0x7FFF],
                pointer: 0,
            }
        }

        /// A pointer byte with big-endian register values after it. The
        /// pointer is a register number (0..=3), so each two bytes fill one
        /// 16-bit word.
        fn write_bytes(&mut self, bytes: &[u8]) {
            let Some(&register) = bytes.first() else {
                return;
            };
            self.pointer = (register & 0x03) * 2;
            for (offset, &value) in bytes[1..].iter().enumerate() {
                let index = (self.pointer as usize + offset) & 0x07;
                if index % 2 == 0 {
                    self.words[index / 2] = (self.words[index / 2] & 0x00FF) | (value as u16) << 8;
                } else {
                    self.words[index / 2] = (self.words[index / 2] & 0xFF00) | value as u16;
                }
            }
            self.pointer = (self.pointer + (bytes.len().saturating_sub(1)) as u8) & 0x07;
        }

        fn read_bytes(&mut self, read: &mut [u8]) {
            for byte in read.iter_mut() {
                let index = (self.pointer & 0x03) as usize;
                let word = self.words[index / 2];
                *byte = if index % 2 == 0 {
                    (word >> 8) as u8
                } else {
                    word as u8
                };
                self.pointer = self.pointer.wrapping_add(1);
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
                    Operation::Write(bytes) => self.write_bytes(bytes),
                    Operation::Read(read) => self.read_bytes(read),
                }
            }
            Ok(())
        }
    }

    /// The configuration the driver wrote, from the fake's register file.
    fn written_config(sensor: &Ads1x15<Fake>) -> u16 {
        sensor.i2c.words[1]
    }

    fn close(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 1e-4
    }

    #[test]
    fn a_single_ended_read_selects_the_channel_and_the_default_gain() {
        // 0x2000 is 8192 counts: 0.512 V at ±2.048 V on a 16-bit part.
        let mut sensor = Ads1x15::new(Fake::new(0x2000), 0x48, Chip::Ads1115).unwrap();
        let reading = sensor
            .read(Input::SingleEnded(0), Gain::TwoVolt048)
            .unwrap();
        assert_eq!(reading.raw, 8_192);
        assert!(close(reading.volts, 0.512), "{reading:?}");

        // OS, MUX 100 (AIN0-GND), PGA 010, single-shot, 128 SPS, comparator off.
        assert_eq!(written_config(&sensor), 0xC583);
        assert_eq!(sensor.address(), 0x48);
        assert_eq!(sensor.chip(), Chip::Ads1115);
    }

    #[test]
    fn a_negative_differential_read_decodes_as_a_negative_count() {
        // 0xE000 is -8192 counts: -0.512 V at ±2.048 V.
        let mut sensor = Ads1x15::new(Fake::new(0xE000), 0x48, Chip::Ads1115).unwrap();
        let reading = sensor
            .read(
                Input::Differential {
                    positive: 0,
                    negative: 1,
                },
                Gain::TwoVolt048,
            )
            .unwrap();
        assert_eq!(reading.raw, -8_192);
        assert!(close(reading.volts, -0.512), "{reading:?}");

        // MUX 000 (AIN0-AIN1).
        assert_eq!(written_config(&sensor), 0x8583);
    }

    #[test]
    fn the_gain_scales_one_count() {
        // ±1.024 V: 0x4000 is 16384 counts, 0.512 V.
        let mut sensor = Ads1x15::new(Fake::new(0x4000), 0x48, Chip::Ads1115).unwrap();
        let reading = sensor
            .read(Input::SingleEnded(1), Gain::OneVolt024)
            .unwrap();
        assert!(close(reading.volts, 0.512), "{reading:?}");
        // MUX 101 (AIN1-GND), PGA 011.
        assert_eq!(written_config(&sensor), 0xD783);

        // The full-scale range itself is the useful number to expose.
        assert!(close(Gain::SixVolt144.full_scale_volts(), 6.144));
        assert!(close(Chip::Ads1115.lsb_volts(Gain::TwoVolt048), 0.000_062_5));
        assert!(close(Chip::Ads1015.lsb_volts(Gain::TwoVolt048), 0.001));
    }

    #[test]
    fn an_ads1015_reads_its_code_from_the_top_twelve_bits() {
        // The same 0x2000 word is 512 counts at 12 bits, and 0.512 V at
        // ±2.048 V (1 mV per count).
        let mut sensor = Ads1x15::new(Fake::new(0x2000), 0x48, Chip::Ads1015).unwrap();
        let reading = sensor
            .read(Input::SingleEnded(0), Gain::TwoVolt048)
            .unwrap();
        assert_eq!(reading.raw, 512);
        assert!(close(reading.volts, 0.512), "{reading:?}");
    }

    #[test]
    fn a_pointer_write_then_a_read_answers_from_the_pointer() {
        // A read with no pointer of its own continues where the last one
        // stopped: the configuration word's low byte follows its high byte.
        let mut sensor = Ads1x15::new(Fake::new(0x2000), 0x48, Chip::Ads1115).unwrap();
        let mut config = [0u8; 2];
        sensor
            .i2c
            .write_read(0x48, &[CONFIG], &mut config)
            .expect("a read");
        assert_eq!(u16::from_be_bytes(config), DEFAULT_CONFIG);
        assert!(DEFAULT_CONFIG & OS != 0, "idle after power-on");
    }
}
