//! An HD44780 character display behind a PCF8574T I2C backpack, written as a
//! virtual part.
//!
//! The firmware talks to it the way it would to the real adapter: each I2C
//! byte written to the expander is one control word, with RS in bit 0, RW in
//! bit 1, EN in bit 2, the backlight in bit 3 and the four display data lines
//! in bits 4..7. A display byte travels as two nibbles, each latched on the
//! EN falling edge: the high nibble first, then the low one. The part runs the
//! HD44780's command set for the subset the driver uses, keeps the 128-byte
//! DDRAM, and shows both 16-column lines through `part_state`. README.md lists
//! what is left out.
//!
//! `Hd44780` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names. The board says which chip pins they are wired
/// to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The PCF8574T adapter's usual address, and the crate's default.
const DEFAULT_ADDR: u32 = 0x27;

/// The PCF8574T control word the 4-bit Adafruit-style wiring uses.
const RS: u8 = 0x01;
const RW: u8 = 0x02;
const ENABLE: u8 = 0x04;
const BACKLIGHT: u8 = 0x08;

/// HD44780 DDRAM: two rows of 40 characters, at 0x00 and 0x40.
const DDRAM: usize = 128;
const ROW_2: usize = 0x40;
/// The panel is 16x2.
const COLUMNS: usize = 16;

/// HD44780 commands the driver uses.
const CLEAR_DISPLAY: u8 = 0x01;
const RETURN_HOME: u8 = 0x02;
const SET_DDRAM_ADDR: u8 = 0x80;

struct Hd44780 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The previous control word, so only EN edges act.
    last_control: Option<u8>,
    /// The high nibble and its RS, waiting for the low one.
    pending: Option<(bool, u8)>,
    /// The display data RAM, spaces at power-on.
    ddram: [u8; DDRAM],
    /// The address counter.
    addr_counter: usize,
    /// Entry mode: increment the counter after a write.
    increment: bool,
    /// Display control bits.
    display_on: bool,
    cursor_on: bool,
    blink_on: bool,
    /// The backpack's backlight line.
    backlight: bool,
}

impl Default for Hd44780 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            last_control: None,
            pending: None,
            ddram: [b' '; DDRAM],
            addr_counter: 0,
            increment: true,
            display_on: false,
            cursor_on: false,
            blink_on: false,
            backlight: false,
        }
    }
}

impl Device for Hd44780 {
    const NAME: &'static str = "hd44780";
    const ON_BUS: bool = true;

    /// Where the board wired the part, and the address it strapped it to.
    fn configure(&mut self, config: &PartConfig) {
        self.scl = config.pins.get(SCL).copied();
        self.sda = config.pins.get(SDA).copied();
        // Zero means the adapter's own default.
        self.addr = if config.addr == 0 {
            DEFAULT_ADDR
        } else {
            config.addr
        };
    }

    /// The address this adapter answers for, on the two pins it is wired to.
    fn claims(&self) -> Vec<Claim> {
        vec![Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: self.addr,
                scl_pin: self.scl,
                sda_pin: self.sda,
            })),
        }]
    }

    /// The guest's I2C transaction. A write is one PCF8574T control word; a
    /// read asks for the expander's pins, and the controller's busy flag is
    /// bit 7 of that answer, so a part that has finished every command
    /// answers zero.
    fn on_bus(&mut self, request: &BusRequest) -> Option<BusReply> {
        let (correlation_id, op) = (request.correlation_id, BusOp::I2c as i32);
        let mut reply = BusReply {
            correlation_id,
            op,
            ..Default::default()
        };
        let Some(bus_request::Op::I2c(i2c)) = &request.op else {
            return Some(reply);
        };
        if i2c.addr != self.addr {
            return Some(reply);
        }
        for &control in &i2c.write {
            self.control(control);
        }
        if i2c.read_len > 0 {
            // All data lines low, so the busy flag reads clear.
            reply.data = vec![0u8; i2c.read_len as usize];
        }
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("line1", self.line(0))
                .set("line2", self.line(ROW_2))
                .set("display", if self.display_on { "on" } else { "off" })
                .set("backlight", if self.backlight { "on" } else { "off" })
                .set("cursor", if self.cursor_on { "on" } else { "off" })
                .set("blink", if self.blink_on { "on" } else { "off" }),
        )
    }
}

impl Hd44780 {
    /// One PCF8574T control word. EN falling latches the nibble on bits
    /// 4..7; the two nibbles together are one display byte. A control word
    /// with RW set is the adapter's read setup and carries no nibble.
    fn control(&mut self, word: u8) {
        self.backlight = word & BACKLIGHT != 0;
        let previous = self.last_control.replace(word);
        let falling = match previous {
            Some(previous) => previous & ENABLE != 0 && word & ENABLE == 0,
            None => false,
        };
        if !falling || word & RW != 0 {
            return;
        }
        let nibble = (word >> 4) & 0x0F;
        let rs = word & RS != 0;
        match self.pending.take() {
            Some((high_rs, high)) => self.byte(high_rs, (high << 4) | nibble),
            None => self.pending = Some((rs, nibble)),
        }
    }

    /// One display byte, now that both nibbles have arrived.
    fn byte(&mut self, rs: bool, byte: u8) {
        if rs {
            self.write_data(byte);
        } else {
            self.command(byte);
        }
    }

    /// A character into DDRAM at the address counter, which then moves the way
    /// the entry mode says.
    fn write_data(&mut self, byte: u8) {
        if self.addr_counter < DDRAM {
            self.ddram[self.addr_counter] = byte;
        }
        if self.increment {
            self.addr_counter = (self.addr_counter + 1) % DDRAM;
        } else {
            self.addr_counter = self.addr_counter.saturating_sub(1);
        }
    }

    /// The HD44780 commands the driver sends. Everything else is a no-op the
    /// part does not model.
    fn command(&mut self, byte: u8) {
        match byte {
            CLEAR_DISPLAY => {
                self.ddram = [b' '; DDRAM];
                self.addr_counter = 0;
            }
            RETURN_HOME => self.addr_counter = 0,
            0x04..=0x07 => {
                self.increment = byte & 0x02 != 0;
            }
            0x08..=0x0F => {
                self.display_on = byte & 0x04 != 0;
                self.cursor_on = byte & 0x02 != 0;
                self.blink_on = byte & 0x01 != 0;
            }
            SET_DDRAM_ADDR..=0xFF => {
                self.addr_counter = (byte & 0x7F) as usize;
            }
            _ => {}
        }
    }

    /// One 16-column row of text, trailing blanks trimmed and unprintable
    /// bytes shown as `.`.
    fn line(&self, start: usize) -> String {
        let row = &self.ddram[start..start + COLUMNS];
        let text: String = row
            .iter()
            .map(|&byte| {
                if (0x20..=0x7E).contains(&byte) {
                    byte as char
                } else {
                    '.'
                }
            })
            .collect();
        text.trim_end().to_string()
    }
}

// Big enough for the longest state line and for the labels a host sends.
esp_part::export_part!(Hd44780, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on 5.
    fn chip() -> Hd44780 {
        let mut chip = Hd44780::default();
        let pins = [(SCL.to_string(), 6), (SDA.to_string(), 5)].into();
        chip.configure(&PartConfig {
            pins,
            addr: 0,
            ..Default::default()
        });
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

    /// One control word at an EN level.
    fn control(chip: &mut Hd44780, rs: bool, enable: bool, nibble: u8, backlight: bool) {
        let word = (nibble << 4)
            | if rs { RS } else { 0 }
            | if enable { ENABLE } else { 0 }
            | if backlight { BACKLIGHT } else { 0 };
        let reply = chip.on_bus(&request(chip.addr, &[word], 0)).unwrap();
        assert!(reply.ack);
    }

    /// `write_byte_to_controller`: each nibble is an EN pulse, high then low.
    fn write_byte(chip: &mut Hd44780, rs: bool, byte: u8) {
        let high = byte >> 4;
        let low = byte & 0x0F;
        for nibble in [high, low] {
            control(chip, rs, true, nibble, true);
            control(chip, rs, false, nibble, true);
        }
    }

    #[test]
    fn it_powers_up_blank_with_the_display_off() {
        let shown = chip().state().unwrap();
        assert_eq!(shown.get("line1"), Some(""));
        assert_eq!(shown.get("line2"), Some(""));
        assert_eq!(shown.get("display"), Some("off"));
        assert_eq!(shown.get("backlight"), Some("off"));
    }

    #[test]
    fn display_control_and_backlight_are_recorded() {
        let mut chip = chip();
        write_byte(&mut chip, false, 0x0C);
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("display"), Some("on"));
        assert_eq!(shown.get("backlight"), Some("on"));
    }

    #[test]
    fn characters_land_in_ddram_at_the_address_counter() {
        let mut chip = chip();
        write_byte(&mut chip, false, 0x0C);
        for byte in b"HELLO" {
            write_byte(&mut chip, true, *byte);
        }
        assert_eq!(chip.state().unwrap().get("line1"), Some("HELLO"));
    }

    #[test]
    fn a_ddram_address_moves_the_text_to_the_second_row() {
        let mut chip = chip();
        write_byte(&mut chip, false, SET_DDRAM_ADDR | 0x40);
        for byte in b"esp32c3" {
            write_byte(&mut chip, true, *byte);
        }
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("line1"), Some(""));
        assert_eq!(shown.get("line2"), Some("esp32c3"));
    }

    #[test]
    fn clear_blanks_the_whole_display() {
        let mut chip = chip();
        for byte in b"HELLO" {
            write_byte(&mut chip, true, *byte);
        }
        write_byte(&mut chip, false, CLEAR_DISPLAY);
        assert_eq!(chip.state().unwrap().get("line1"), Some(""));
    }

    #[test]
    fn a_read_answers_not_busy() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x27, &[RW | 0x0F], 1)).unwrap();
        assert!(reply.ack);
        assert_eq!(reply.data, [0]);
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x27,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x3F, &[0x0C], 0)).unwrap();
        assert!(!reply.ack);
    }

    /// One raw control word, for the adapter's read setup.
    fn raw(chip: &mut Hd44780, word: u8) {
        let reply = chip.on_bus(&request(chip.addr, &[word], 0)).unwrap();
        assert!(reply.ack);
    }

    #[test]
    fn the_read_setup_carries_no_nibble() {
        let mut chip = chip();
        // The adapter's busy check: RS low, RW high, data lines released.
        // Two EN pulses, both with RW set, must not become a display byte.
        for word in [0xF2, 0xF6, 0xF2, 0xF6, 0xF2] {
            raw(&mut chip, word);
        }
        write_byte(&mut chip, true, b'A');
        assert_eq!(chip.state().unwrap().get("line1"), Some("A"));
    }
}
