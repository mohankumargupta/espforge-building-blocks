//! A Holtek HT16K33 LED matrix and alphanumeric display driver, written as a
//! virtual part.
//!
//! The firmware talks to it the way it would to the real chip: the part is
//! command-addressed. The first byte of a write is the command and the bytes
//! after it are that command's parameters:
//!
//! | Command | Meaning |
//! |---|---|
//! | `0x00..=0x0F` | display RAM address; the bytes after it fill RAM from there, auto-incrementing and wrapping |
//! | `0x20`/`0x21` | system setup: oscillator off/on |
//! | `0x80..=0x8F` | display setup: bit 0 display on, bits 2:1 blink rate |
//! | `0xE0..=0xEF` | dimming: the low nibble is the level |
//!
//! A read answers RAM from the last RAM address, wrapping at the end of the 16
//! bytes. A command the part does not have is NACKed, as the keyscan commands
//! are not modelled.
//!
//! README.md lists what is left out.
//!
//! `Ht16k33` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with all three address jumpers low; 0x71 to 0x77 are the
/// other straps.
const DEFAULT_ADDR: u32 = 0x70;

/// The display RAM: 16 bytes, two per row.
const RAM_BYTES: usize = 16;
/// The dimming levels the part has.
const MAX_BRIGHTNESS: u8 = 15;

struct Ht16k33 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The display RAM, all zeros at power-on.
    ram: [u8; RAM_BYTES],
    /// Where a RAM write or a read starts.
    pointer: usize,
    /// System setup: the internal oscillator.
    oscillator: bool,
    /// Display setup: the display and its blink rate.
    display_on: bool,
    blink: u8,
    /// The dimming level.
    brightness: u8,
}

impl Default for Ht16k33 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            ram: [0u8; RAM_BYTES],
            pointer: 0,
            oscillator: false,
            display_on: false,
            blink: 0,
            brightness: 0,
        }
    }
}

impl Device for Ht16k33 {
    const NAME: &'static str = "ht16k33";
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

    /// The address this part answers for, on the two pins it is wired to.
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

    /// Change the part's own settings: `set:brightness=<0..15>`, `off` and
    /// `on` for the display, or `rest` for the power-on state.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            if what == "brightness" {
                self.brightness = (value.clamp(0.0, f64::from(MAX_BRIGHTNESS)).round()) as u8;
            }
        } else {
            match name {
                "off" => self.display_on = false,
                "on" => self.display_on = true,
                "rest" => *self = Self::configured_like(self),
                _ => {}
            }
        }
    }

    /// The guest's I2C transaction: the first byte of a write is the command,
    /// and a read answers RAM from the pointer.
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
        if let Some((&command, data)) = i2c.write.split_first() {
            match command {
                // A display RAM address, and possibly the bytes that go there.
                0x00..=0x0F => {
                    self.pointer = usize::from(command);
                    for (offset, byte) in data.iter().enumerate() {
                        let at = (self.pointer + offset) % RAM_BYTES;
                        self.ram[at] = *byte;
                    }
                    self.pointer = (self.pointer + data.len()) % RAM_BYTES;
                }
                // System setup: bit 0 is the oscillator.
                0x20..=0x21 => self.oscillator = command & 0x01 != 0,
                // Display setup: bit 0 is the display, bits 2:1 the blink rate.
                0x80..=0x8F => {
                    self.display_on = command & 0x01 != 0;
                    self.blink = (command >> 1) & 0x03;
                }
                // Dimming: the low nibble is the level.
                0xE0..=0xEF => self.brightness = command & 0x0F,
                // Keyscan and the commands the part does not have.
                _ => return Some(reply),
            }
        }
        if i2c.read_len > 0 {
            let start = self.pointer;
            reply.data = (0..i2c.read_len as usize)
                .map(|offset| self.ram[(start + offset) % RAM_BYTES])
                .collect();
            // The pointer walks on, wrapping at the end of the RAM.
            self.pointer = (start + i2c.read_len as usize) % RAM_BYTES;
        }
        // A probe, a command write, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("oscillator", u8::from(self.oscillator))
                .set("display", u8::from(self.display_on))
                .set("blink", self.blink)
                .set("brightness", self.brightness),
        )
    }
}

impl Ht16k33 {
    /// The same wiring and address, with the registers back at power-on.
    fn configured_like(&self) -> Self {
        Self {
            scl: self.scl,
            sda: self.sda,
            addr: self.addr,
            ..Self::default()
        }
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Ht16k33, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the 0x70 strap.
    fn chip() -> Ht16k33 {
        let mut chip = Ht16k33::default();
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

    /// One transaction to the part's address: the bytes read back, or `None`
    /// for a NACK.
    fn transfer(chip: &mut Ht16k33, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// A frame with a different byte in every position.
    fn frame() -> Vec<u8> {
        (0..RAM_BYTES).map(|index| index as u8 | 0x80).collect()
    }

    #[test]
    fn a_fresh_part_is_dark_and_off() {
        let chip = chip();
        let state = chip.state().unwrap();
        assert_eq!(state.get("oscillator"), Some("0"));
        assert_eq!(state.get("display"), Some("0"));
        assert_eq!(state.get("brightness"), Some("0"));
    }

    #[test]
    fn the_setup_commands_start_the_part() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x21], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[0x81], 0), Some(vec![]));
        let state = chip.state().unwrap();
        assert_eq!(state.get("oscillator"), Some("1"));
        assert_eq!(state.get("display"), Some("1"));
        assert_eq!(state.get("blink"), Some("0"));
    }

    #[test]
    fn a_frame_written_to_ram_reads_back() {
        let mut chip = chip();
        let mut write = vec![0x00];
        write.extend(frame());
        assert_eq!(transfer(&mut chip, &write, 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[0x00], RAM_BYTES as u32), Some(frame()));
    }

    #[test]
    fn a_ram_write_wraps_at_the_end() {
        let mut chip = chip();
        // Twelve bytes from address 10 run past byte 15 and wrap to 0.
        let data: Vec<u8> = (1..=12).collect();
        let mut write = vec![0x0A];
        write.extend(&data);
        assert_eq!(transfer(&mut chip, &write, 0), Some(vec![]));
        let ram = transfer(&mut chip, &[0x00], RAM_BYTES as u32).unwrap();
        assert_eq!(&ram[10..16], &data[..6]);
        assert_eq!(&ram[0..6], &data[6..]);
    }

    #[test]
    fn a_read_from_the_end_of_the_ram_wraps() {
        let mut chip = chip();
        let mut write = vec![0x00];
        write.extend(frame());
        assert_eq!(transfer(&mut chip, &write, 0), Some(vec![]));
        assert_eq!(
            transfer(&mut chip, &[0x0E], 4),
            Some(vec![0x8E, 0x8F, 0x80, 0x81])
        );
    }

    #[test]
    fn the_dimming_command_sets_the_level() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0xE7], 0), Some(vec![]));
        assert_eq!(chip.state().unwrap().get("brightness"), Some("7"));
        assert_eq!(transfer(&mut chip, &[0xEF], 0), Some(vec![]));
        assert_eq!(chip.state().unwrap().get("brightness"), Some("15"));
    }

    #[test]
    fn the_display_setup_carries_the_blink_rate() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x85], 0), Some(vec![]));
        let state = chip.state().unwrap();
        assert_eq!(state.get("display"), Some("1"));
        assert_eq!(state.get("blink"), Some("2"));
        assert_eq!(transfer(&mut chip, &[0x84], 0), Some(vec![]));
        assert_eq!(chip.state().unwrap().get("display"), Some("0"));
    }

    #[test]
    fn a_command_the_part_lacks_is_nacked() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x40], 0), None, "keyscan");
        assert_eq!(transfer(&mut chip, &[0x90], 0), None);
        assert_eq!(transfer(&mut chip, &[0xFF], 0), None);
    }

    #[test]
    fn the_inputs_change_the_settings() {
        let mut chip = chip();
        chip.on_input("set:brightness=9");
        chip.on_input("on");
        let state = chip.state().unwrap();
        assert_eq!(state.get("brightness"), Some("9"));
        assert_eq!(state.get("display"), Some("1"));

        chip.on_input("set:brightness=99");
        assert_eq!(chip.state().unwrap().get("brightness"), Some("15"));
        chip.on_input("off");
        assert_eq!(chip.state().unwrap().get("display"), Some("0"));

        chip.on_input("rest");
        let state = chip.state().unwrap();
        assert_eq!(state.get("brightness"), Some("0"));
        assert_eq!(state.get("display"), Some("0"));
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x70,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x70, &[0x21], 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x71, &[], 16)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x71 on a 0x70 part");
        assert!(reply.data.is_empty());
    }
}
