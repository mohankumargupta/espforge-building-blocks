//! An AiP31068 character LCD controller (16 characters, 2 lines), written as
//! a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: the first byte
//! of a write is a control byte, `0x00` for an instruction and `0x40` for
//! display data, and the bytes after it are applied in order. The instruction
//! set is the HD44780's: clear, return home, entry mode, display control,
//! cursor/display shift, function set, set CGRAM address and set DDRAM
//! address. Data goes into DDRAM at the cursor, which advances (or retreats)
//! as the entry mode says.
//!
//! The panel is 16x2: row 0 is DDRAM `0x00..=0x0F` and row 1 is `0x40..=0x4F`.
//! Addresses past those windows exist in the 128-cell DDRAM but are not
//! shown, as on the real part. README.md lists what is left out.
//!
//! `Aip31068` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The AiP31068's only strapped address, 0x3E in 7 bits.
const DEFAULT_ADDR: u32 = 0x3E;

/// The control byte before an instruction (`RS` clear) and before display
/// data (`RS` set).
const CONTROL_COMMAND: u8 = 0x00;
const CONTROL_DATA: u8 = 0x40;

/// The instructions, by their opcode bits. The low bits carry a parameter for
/// the ones that take one.
const CLEAR_DISPLAY: u8 = 0x01;
const RETURN_HOME: u8 = 0x02;
const ENTRY_MODE_SET: u8 = 0x04;
const DISPLAY_CONTROL: u8 = 0x08;
const CURSOR_OR_DISPLAY_SHIFT: u8 = 0x10;
const FUNCTION_SET: u8 = 0x20;
const SET_CGRAM_ADDR: u8 = 0x40;
const SET_DDRAM_ADDR: u8 = 0x80;

/// The panel: 16 characters on each of two lines.
const COLUMNS: usize = 16;
const ROWS: usize = 2;
/// Row 0 starts at DDRAM 0x00 and row 1 at 0x40, as on the HD44780.
const ROW_STARTS: [u8; ROWS] = [0x00, 0x40];
/// DDRAM addresses are seven bits wide.
const DDRAM_LEN: usize = 128;
/// What a cleared DDRAM cell holds.
const BLANK: u8 = b' ';

struct Aip31068 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The display's DDRAM, by address. Only the two row windows are shown.
    ddram: [u8; DDRAM_LEN],
    /// The cursor: the DDRAM address the next data byte goes to.
    cursor: u8,
    /// Entry mode `I/D`: whether the cursor advances or retreats.
    increment: bool,
    /// Display control `D`. Off at power-on until the firmware turns it on.
    display_on: bool,
}

impl Default for Aip31068 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            ddram: [BLANK; DDRAM_LEN],
            cursor: 0,
            increment: true,
            display_on: false,
        }
    }
}

impl Device for Aip31068 {
    const NAME: &'static str = "aip31068";
    const ON_BUS: bool = true;

    /// Where the board wired the part, and the address it strapped it to.
    fn configure(&mut self, config: &PartConfig) {
        self.scl = config.pins.get(SCL).copied();
        self.sda = config.pins.get(SDA).copied();
        // Zero means the part's own default.
        self.addr = if config.addr == 0 {
            DEFAULT_ADDR
        } else {
            config.addr
        };
    }

    /// The address this panel answers for, on the two pins it is wired to.
    /// The emulator works out the I2C port from those pins.
    fn claims(&self) -> Vec<Claim> {
        vec![Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: self.addr,
                scl_pin: self.scl,
                sda_pin: self.sda,
            })),
        }]
    }

    /// The guest's I2C transaction: the control byte decides whether the
    /// bytes after it are instructions or display data. A transaction that
    /// reads is answered with nothing driven, which the master sees as 0xFF;
    /// this display is written, never read.
    fn on_bus(&mut self, request: &BusRequest) -> Option<BusReply> {
        let (correlation_id, op) = (request.correlation_id, BusOp::I2c as i32);
        let mut reply = BusReply {
            correlation_id,
            op,
            ..Default::default()
        };
        // The emulator sends only transactions to this part's address. Any
        // other is answered as an empty address would be, with a NACK.
        let Some(bus_request::Op::I2c(i2c)) = &request.op else {
            return Some(reply);
        };
        if i2c.addr != self.addr {
            return Some(reply);
        }
        if let Some((&control, bytes)) = i2c.write.split_first() {
            match control {
                CONTROL_COMMAND => {
                    for &byte in bytes {
                        self.command(byte);
                    }
                }
                CONTROL_DATA => {
                    for &byte in bytes {
                        self.write_data(byte);
                    }
                }
                // Any other control byte carries a read or a continuation,
                // which this part's driver never sends.
                _ => {}
            }
        }
        if i2c.read_len > 0 {
            // Nothing drives the line on a read from this panel.
            reply.data = vec![0xFF; i2c.read_len as usize];
        }
        // A probe, a write on its own, or a read.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("line1", self.line(0))
                .set("line2", self.line(1)),
        )
    }
}

impl Aip31068 {
    /// What one visible row shows, with the blank cells at its end dropped.
    fn line(&self, row: usize) -> String {
        let start = ROW_STARTS[row] as usize;
        self.ddram[start..start + COLUMNS]
            .iter()
            .map(|&byte| {
                if byte.is_ascii_graphic() || byte == BLANK {
                    byte as char
                } else {
                    // A control byte or a custom glyph: not text.
                    '?'
                }
            })
            .collect::<String>()
            .trim_end_matches(' ')
            .to_string()
    }

    /// One instruction byte.
    fn command(&mut self, command: u8) {
        match command {
            CLEAR_DISPLAY => {
                self.ddram.fill(BLANK);
                self.cursor = 0;
                // The clear also resets the entry mode to increment.
                self.increment = true;
            }
            RETURN_HOME => self.cursor = 0,
            // `0x04..=0x07`: `I/D` is bit 1, `S` (display shift) is bit 0,
            // which a 16x2 panel with the driver's text never needs.
            c if c & 0xFC == ENTRY_MODE_SET => self.increment = c & 0x02 != 0,
            // `0x08..=0x0F`: `D` is bit 2, `C` bit 1, `B` bit 0.
            c if c & 0xF8 == DISPLAY_CONTROL => self.display_on = c & 0x04 != 0,
            // `0x10..=0x1F`: cursor and display shifts, not modelled.
            c if c & 0xF0 == CURSOR_OR_DISPLAY_SHIFT => {}
            // `0x20..=0x3F`: the panel is 8-bit and 2-line whatever is asked.
            c if c & 0xE0 == FUNCTION_SET => {}
            // `0x80..=0xFF`: set the DDRAM address (the cursor).
            c if c & 0x80 == SET_DDRAM_ADDR => self.cursor = c & 0x7F,
            // `0x40..=0x7F`: custom glyphs are not modelled.
            c if c & 0xC0 == SET_CGRAM_ADDR => {}
            _ => {}
        }
    }

    /// One display-data byte at the cursor, which then moves as the entry
    /// mode says.
    fn write_data(&mut self, byte: u8) {
        self.ddram[(self.cursor & 0x7F) as usize] = byte;
        self.cursor = if self.increment {
            self.cursor.wrapping_add(1) & 0x7F
        } else {
            self.cursor.wrapping_sub(1) & 0x7F
        };
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Aip31068, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the part's fixed address.
    fn chip() -> Aip31068 {
        let mut chip = Aip31068::default();
        let pins = [(SCL.to_string(), 6), (SDA.to_string(), 5)].into();
        chip.configure(&PartConfig { pins, addr: 0 });
        chip
    }

    fn request(addr: u32, write: &[u8], read_len: u32) -> BusRequest {
        BusRequest {
            correlation_id: 7,
            op: Some(bus_request::Op::I2c(I2cTransaction {
                bus: 0,
                addr,
                write: write.to_vec(),
                read_len,
                continues: false,
            })),
        }
    }

    /// One transaction to the panel's address: the bytes read back, or `None`
    /// for a NACK.
    fn transfer(chip: &mut Aip31068, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// One instruction, the way the driver sends it.
    fn command(chip: &mut Aip31068, command: u8) {
        assert_eq!(
            transfer(chip, &[CONTROL_COMMAND, command], 0),
            Some(vec![]),
            "a command write is acknowledged"
        );
    }

    /// Display data, the way the driver sends it.
    fn data(chip: &mut Aip31068, text: &[u8]) {
        let mut write = Vec::from([CONTROL_DATA]);
        write.extend_from_slice(text);
        assert_eq!(
            transfer(chip, &write, 0),
            Some(vec![]),
            "a data write is acknowledged"
        );
    }

    /// The initialization the driver's `new()` sends.
    fn initialize(chip: &mut Aip31068) {
        for instruction in [
            FUNCTION_SET | 0x18,
            DISPLAY_CONTROL | 0x04,
            CLEAR_DISPLAY,
            ENTRY_MODE_SET | 0x02,
        ] {
            command(chip, instruction);
        }
    }

    #[test]
    fn the_power_on_panel_is_blank_and_off() {
        let chip = chip();
        assert!(!chip.display_on, "display control is off at power-on");
        assert_eq!(chip.cursor, 0);
        let shown = chip.state().unwrap();
        assert_eq!((shown.get("line1"), shown.get("line2")), (Some(""), Some("")));
    }

    #[test]
    fn initialization_turns_the_display_on_and_clears_it() {
        let mut chip = chip();
        initialize(&mut chip);
        assert!(chip.display_on);
        assert_eq!(chip.cursor, 0);
        assert!(chip.increment, "entry mode `I/D` is set");
        let shown = chip.state().unwrap();
        assert_eq!((shown.get("line1"), shown.get("line2")), (Some(""), Some("")));
    }

    #[test]
    fn data_lands_at_the_row_the_cursor_was_sent_to() {
        let mut chip = chip();
        initialize(&mut chip);
        command(&mut chip, SET_DDRAM_ADDR | ROW_STARTS[0]);
        data(&mut chip, b"aip31068-lcd");
        command(&mut chip, SET_DDRAM_ADDR | ROW_STARTS[1]);
        data(&mut chip, b"hello-i2c");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("line1"), shown.get("line2")),
            (Some("aip31068-lcd"), Some("hello-i2c"))
        );
        assert_eq!(chip.cursor, ROW_STARTS[1] + 9, "the cursor advanced");
    }

    #[test]
    fn the_cursor_moves_one_cell_per_data_byte() {
        let mut chip = chip();
        command(&mut chip, SET_DDRAM_ADDR | 3);
        data(&mut chip, b"xyz");
        assert_eq!(&chip.ddram[3..6], b"xyz");
        assert_eq!(chip.cursor, 6);
    }

    #[test]
    fn a_decrementing_entry_mode_moves_the_cursor_back() {
        let mut chip = chip();
        command(&mut chip, ENTRY_MODE_SET); // `I/D` clear: decrement
        command(&mut chip, SET_DDRAM_ADDR | (ROW_STARTS[1] + 5));
        data(&mut chip, b"ab");
        assert_eq!(&chip.ddram[ROW_STARTS[1] as usize + 4..][..2], b"ba");
        assert_eq!(chip.cursor, ROW_STARTS[1] + 3);
    }

    #[test]
    fn clearing_blanks_both_rows_and_homes_the_cursor() {
        let mut chip = chip();
        initialize(&mut chip);
        command(&mut chip, SET_DDRAM_ADDR);
        data(&mut chip, b"something");
        command(&mut chip, CLEAR_DISPLAY);
        assert_eq!(chip.cursor, 0);
        let shown = chip.state().unwrap();
        assert_eq!((shown.get("line1"), shown.get("line2")), (Some(""), Some("")));
    }

    #[test]
    fn a_byte_past_the_visible_window_is_stored_but_not_shown() {
        let mut chip = chip();
        command(&mut chip, SET_DDRAM_ADDR | 0x10);
        data(&mut chip, b"hidden");
        let shown = chip.state().unwrap();
        assert_eq!((shown.get("line1"), shown.get("line2")), (Some(""), Some("")));
        assert_eq!(&chip.ddram[0x10..0x16], b"hidden");
    }

    #[test]
    fn a_custom_glyph_code_shows_as_a_question_mark() {
        let mut chip = chip();
        command(&mut chip, SET_DDRAM_ADDR);
        data(&mut chip, b"A\x01B");
        assert_eq!(chip.state().unwrap().get("line1"), Some("A?B"));
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x3E,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip
            .on_bus(&request(0x3F, &[CONTROL_COMMAND, CLEAR_DISPLAY], 0))
            .unwrap();
        assert!(!reply.ack, "nothing answers at 0x3F");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn a_read_is_answered_with_the_line_undriven() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[], 2),
            Some(vec![0xFF, 0xFF]),
            "the panel is never read"
        );
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x3E, &[CONTROL_COMMAND, CLEAR_DISPLAY], 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
