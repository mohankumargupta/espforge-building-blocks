//! A TI TMP121/TMP123 temperature sensor, over any `embedded-hal` SPI device.
//!
//! The sensor has no data input: while its chip select is low it clocks out one
//! 16-bit word, most significant bit first, whose bits 15..3 are the
//! temperature in 13-bit two's complement at 0.0625 °C a count (SBOS273C,
//! Table 2). Bits 2..0 are 0.
//!
//! The endpoint is generic, so espforge's `ConfiguredSpiDevice`, the emulator's
//! virtual part and a host test double all take the same code.

#![no_std]

use embedded_hal::spi::{Operation, SpiDevice};

/// A TMP121/TMP123 on an SPI device endpoint.
pub struct Tmp123<SPI> {
    spi: SPI,
}

impl<SPI: SpiDevice> Tmp123<SPI> {
    /// Take the endpoint. The sensor needs no configuration, so this cannot
    /// fail and the manifest declares `fallible #false`.
    pub const fn new(spi: SPI) -> Self {
        Self { spi }
    }

    /// One measurement, in degrees Celsius.
    ///
    /// A read is one 16-bit transfer. The part converts continuously; this
    /// does not wait for a conversion to finish, so a read returns the last
    /// result, which is what the datasheet's interface allows.
    pub fn read_celsius(&mut self) -> Result<f32, SPI::Error> {
        let mut bytes = [0u8; 2];
        self.spi.transaction(&mut [Operation::Read(&mut bytes)])?;
        let raw = i16::from_be_bytes(bytes);
        // Arithmetic shift: the sign is bit 15, and 13 bits remain.
        Ok((raw >> 3) as f32 * 0.0625)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::spi::ErrorType;

    /// The part, reduced to what a transfer sees: one 16-bit word, then 0xFF.
    struct Fake {
        word: u16,
    }

    impl ErrorType for Fake {
        type Error = Infallible;
    }

    impl SpiDevice for Fake {
        fn transaction(
            &mut self,
            operations: &mut [Operation<'_, u8>],
        ) -> Result<(), Self::Error> {
            for operation in operations {
                if let Operation::Read(buf) = operation {
                    let word = self.word.to_be_bytes();
                    for (i, byte) in buf.iter_mut().enumerate() {
                        *byte = if i < 2 { word[i] } else { 0xFF };
                    }
                }
            }
            Ok(())
        }
    }

    fn read(word: u16) -> f32 {
        Tmp123::new(Fake { word }).read_celsius().unwrap()
    }

    #[test]
    fn every_row_of_the_datasheets_table_2_reads_as_listed() {
        // SBOS273C Table 2, "Temperature Data Format".
        let table = [
            (150.0, 0x4B00),
            (125.0, 0x3E80),
            (25.0, 0x0C80),
            (0.0625, 0x0008),
            (0.0, 0x0000),
            (-0.0625, 0xFFF8),
            (-25.0, 0xF380),
            (-55.0, 0xE480),
        ];
        for (celsius, word) in table {
            assert_eq!(read(word), celsius, "{celsius} °C");
        }
    }

    #[test]
    fn a_floating_bus_reads_minus_one_count_not_garbage() {
        // 0xFFFF sign-extends to -1 and shifts to -1, so a missing part reads
        // -0.0625 °C. That looks plausible: the emulator test must assert exact
        // values and the attach line, never just "some reading arrived".
        assert_eq!(read(0xFFFF), -0.0625);
    }
}
