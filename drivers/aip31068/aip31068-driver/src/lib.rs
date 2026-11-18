//! An AiP31068 character LCD controller (16 characters, 2 lines), over any
//! `embedded-hal` I2C bus.
//!
//! A transaction is a control byte followed by the bytes it applies to:
//! `0x00` starts an instruction (`RS` clear) and `0x40` starts display data
//! (`RS` set). The instruction set is the HD44780's, so [`Aip31068::new`]
//! sends the initialization such a part needs — function set for an 8-bit,
//! 2-line panel, display on, clear, and entry mode with the cursor
//! incrementing — and [`Aip31068::write_line`] addresses the start of a row in
//! DDRAM and writes a string there.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: the part is written, never read, and the
//! instruction timings (1.53 ms after a clear or a home) are the caller's. See
//! the fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep the command
// log in vectors.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The AiP31068's only I2C address.
pub const ADDRESS: u8 = 0x3E;

/// Characters on each line of the panel this driver addresses.
pub const COLUMNS: u8 = 16;
/// Lines on the panel.
pub const ROWS: u8 = 2;

/// Control byte before an instruction: `RS` clear.
const COMMAND: u8 = 0x00;
/// Control byte before display data: `RS` set.
const DATA: u8 = 0x40;

/// Function set: 8-bit interface, 2 lines, 5x8 font.
const FUNCTION_SET_8BIT_2LINE: u8 = 0x38;
/// Display control: display on, cursor off, blink off.
const DISPLAY_ON: u8 = 0x0C;
/// Clear display, then home the cursor.
const CLEAR_DISPLAY: u8 = 0x01;
/// Entry mode: increment the cursor, do not shift the display.
const ENTRY_INCREMENT: u8 = 0x06;

/// The DDRAM address each row starts at, as on the HD44780.
const ROW_STARTS: [u8; ROWS as usize] = [0x00, 0x40];
/// `SET_DDRAM_ADDR`, with the address in the low seven bits.
const SET_DDRAM_ADDR: u8 = 0x80;

/// Why a transfer failed, or a position the panel does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Aip31068Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The row or column is outside the panel.
    InvalidPosition,
}

/// An AiP31068 panel on an I2C bus.
pub struct Aip31068<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Aip31068<I2C> {
    /// Initialize the panel and leave it ready for [`Aip31068::write_line`].
    ///
    /// The four instructions are the HD44780's power-on sequence for an
    /// 8-bit, 2-line part, which the I2C interface makes one write each. The
    /// clear's 1.53 ms is not waited for; a caller that writes immediately
    /// after this is still within the part's timing on most modules.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Aip31068Error<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.command(FUNCTION_SET_8BIT_2LINE)?;
        this.command(DISPLAY_ON)?;
        this.command(CLEAR_DISPLAY)?;
        this.command(ENTRY_INCREMENT)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// Clear the panel and home the cursor.
    pub fn clear(&mut self) -> Result<(), Aip31068Error<I2C::Error>> {
        self.command(CLEAR_DISPLAY)
    }

    /// Write `text` from the start of `row`, `0` being the top line.
    ///
    /// Anything past the last column of the row continues into the next DDRAM
    /// address, which a 16x2 panel does not display; the driver does not wrap
    /// or pad. Clear the panel first if a shorter line must not leave the end
    /// of an older one behind.
    pub fn write_line(&mut self, row: u8, text: &str) -> Result<(), Aip31068Error<I2C::Error>> {
        if row >= ROWS {
            return Err(Aip31068Error::InvalidPosition);
        }
        self.set_cursor(row, 0)?;
        self.write_text(text)
    }

    /// Put the cursor at `column` of `row`.
    fn set_cursor(&mut self, row: u8, column: u8) -> Result<(), Aip31068Error<I2C::Error>> {
        if row >= ROWS || column >= COLUMNS {
            return Err(Aip31068Error::InvalidPosition);
        }
        self.command(SET_DDRAM_ADDR | (ROW_STARTS[row as usize] + column))
    }

    /// The string as display data, one write per row's worth of bytes.
    fn write_text(&mut self, text: &str) -> Result<(), Aip31068Error<I2C::Error>> {
        let mut frame = [0u8; COLUMNS as usize + 1];
        for chunk in text.as_bytes().chunks(COLUMNS as usize) {
            frame[0] = DATA;
            frame[1..=chunk.len()].copy_from_slice(chunk);
            self.i2c
                .write(self.address, &frame[..=chunk.len()])
                .map_err(Aip31068Error::Bus)?;
        }
        Ok(())
    }

    /// One instruction: the control byte, then the instruction byte.
    fn command(&mut self, command: u8) -> Result<(), Aip31068Error<I2C::Error>> {
        self.i2c
            .write(self.address, &[COMMAND, command])
            .map_err(Aip31068Error::Bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The panel reduced to a log of the writes it receives.
    struct Fake {
        fail: bool,
        writes: Vec<Vec<u8>>,
    }

    #[derive(Debug, PartialEq, Eq)]
    struct FakeError;

    impl Fake {
        fn new() -> Self {
            Self {
                fail: false,
                writes: Vec::new(),
            }
        }
    }

    impl embedded_hal::i2c::Error for FakeError {
        fn kind(&self) -> embedded_hal::i2c::ErrorKind {
            embedded_hal::i2c::ErrorKind::Other
        }
    }

    impl ErrorType for Fake {
        type Error = FakeError;
    }

    impl I2c for Fake {
        fn transaction(
            &mut self,
            _address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            if self.fail {
                return Err(FakeError);
            }
            for operation in operations {
                if let Operation::Write(bytes) = operation {
                    self.writes.push(bytes.to_vec());
                }
            }
            Ok(())
        }
    }

    /// A panel that has already been initialized, with the initialization's
    /// writes dropped so a test's own writes are the only ones in the log.
    fn panel() -> Aip31068<Fake> {
        let mut panel = Aip31068::new(Fake::new(), ADDRESS).expect("an AiP31068");
        panel.i2c.writes.clear();
        panel
    }

    fn bytes(text: &[u8]) -> Vec<u8> {
        let mut frame = Vec::from([DATA]);
        frame.extend_from_slice(text);
        frame
    }

    #[test]
    fn it_initializes_the_panel() {
        let panel = Aip31068::new(Fake::new(), ADDRESS).unwrap();
        assert_eq!(
            panel.i2c.writes,
            [
                Vec::from([COMMAND, FUNCTION_SET_8BIT_2LINE]),
                Vec::from([COMMAND, DISPLAY_ON]),
                Vec::from([COMMAND, CLEAR_DISPLAY]),
                Vec::from([COMMAND, ENTRY_INCREMENT]),
            ],
            "function set, display on, clear, entry mode"
        );
        assert_eq!(panel.address(), ADDRESS);
        assert_eq!(panel.address(), 0x3E);
    }

    #[test]
    fn a_line_starts_at_its_rows_ddram_address() {
        let mut panel = panel();
        panel.write_line(0, "ok").unwrap();
        assert_eq!(
            panel.i2c.writes,
            [
                Vec::from([COMMAND, SET_DDRAM_ADDR]), // row 0 starts at 0x00
                bytes(b"ok"),
            ]
        );

        panel.i2c.writes.clear();
        panel.write_line(1, "ok").unwrap();
        assert_eq!(
            panel.i2c.writes,
            [
                Vec::from([COMMAND, SET_DDRAM_ADDR | 0x40]), // row 1 starts at 0x40
                bytes(b"ok"),
            ]
        );
    }

    #[test]
    fn a_row_the_panel_does_not_have_is_refused() {
        let mut panel = panel();
        assert_eq!(
            panel.write_line(2, "ok").err(),
            Some(Aip31068Error::InvalidPosition)
        );
        assert!(panel.i2c.writes.is_empty(), "nothing was sent");
    }

    #[test]
    fn a_line_longer_than_the_panel_is_written_in_chunks() {
        let mut panel = panel();
        let text = "0123456789abcdef!";
        panel.write_line(0, text).unwrap();
        assert_eq!(panel.i2c.writes.len(), 3, "the address and two data writes");
        assert_eq!(panel.i2c.writes[1], bytes(b"0123456789abcdef"));
        assert_eq!(panel.i2c.writes[2], bytes(b"!"));
    }

    #[test]
    fn clear_sends_the_clear_instruction() {
        let mut panel = panel();
        panel.clear().unwrap();
        assert_eq!(panel.i2c.writes.last(), Some(&Vec::from([COMMAND, CLEAR_DISPLAY])));
    }

    #[test]
    fn a_bus_error_is_reported() {
        let mut fake = Fake::new();
        fake.fail = true;
        assert_eq!(
            Aip31068::new(fake, ADDRESS).err(),
            Some(Aip31068Error::Bus(FakeError))
        );
    }

    #[test]
    fn an_empty_line_still_addresses_the_row() {
        let mut panel = panel();
        panel.write_line(1, "").unwrap();
        assert_eq!(
            panel.i2c.writes,
            [Vec::from([COMMAND, SET_DDRAM_ADDR | 0x40])],
            "the cursor moves, no data is written"
        );
    }
}
