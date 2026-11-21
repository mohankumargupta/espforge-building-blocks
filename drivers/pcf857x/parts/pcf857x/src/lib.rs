//! A NXP PCF8574 8-bit I2C I/O expander, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip, and the real
//! chip has **no register file**: every write byte *is* the port's output
//! latch, and every read answers with the level on the eight pins.
//!
//! The pins are quasi-bidirectional, which is what [`Pcf857x::pins`] models: a
//! latch bit of 0 drives the pin low, and a latch bit of 1 only releases the
//! pin to its weak pull-up. The world can therefore pull a released pin low, but
//! it cannot make a pin that is driven low read high.
//!
//! README.md lists what is left out.
//!
//! `Pcf857x` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with every address pin low.
const DEFAULT_ADDR: u32 = 0x20;

/// The power-on port: every pin released to its pull-up, so every pin reads
/// high.
const POWER_ON: u8 = 0xFF;

struct Pcf857x {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The output latch: a 0 drives a pin low, a 1 releases it.
    latch: u8,
    /// The world's drive on the pins. A 0 here pulls a released pin low; a 1
    /// leaves the pull-up alone. A pin the latch drives low stays low.
    levels: u8,
}

impl Default for Pcf857x {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            latch: POWER_ON,
            levels: POWER_ON,
        }
    }
}

impl Device for Pcf857x {
    const NAME: &'static str = "pcf857x";
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

    /// `set:levels=<hex>` is the world driving the pins low. `rest` is the
    /// power-on port.
    fn on_input(&mut self, name: &str) {
        if name == "rest" {
            let (scl, sda, addr) = (self.scl, self.sda, self.addr);
            *self = Self {
                scl,
                sda,
                addr,
                ..Self::default()
            };
        } else if let Some(value) = name.strip_prefix("set:levels=") {
            if let Ok(levels) = u8::from_str_radix(value.trim_start_matches("0x"), 16) {
                self.levels = levels;
            }
        }
    }

    /// One transaction to this part: the bytes written are the port, the bytes
    /// read are the pins.
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
        // There is no register to point at: each byte written is the port, so
        // the last byte of a multi-byte write is what stays on the pins.
        if let Some(&last) = i2c.write.last() {
            self.latch = last;
        }
        if i2c.read_len > 0 {
            // A repeated read keeps answering with the same pins, as the part
            // does until the latch changes.
            let pins = self.pins();
            reply.data = (0..i2c.read_len).map(|_| pins).collect();
        }
        // A probe, a port write, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("outputs", u16::from(self.latch))
                .set("pins", u16::from(self.pins())),
        )
    }
}

impl Pcf857x {
    /// The level on the pins: a pin the latch drives low stays low, and a
    /// released pin follows its pull-up unless the world pulls it low.
    fn pins(&self) -> u8 {
        self.latch & self.levels
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Pcf857x, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the default 0x20 address.
    fn chip() -> Pcf857x {
        let mut chip = Pcf857x::default();
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
    fn transfer(chip: &mut Pcf857x, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    #[test]
    fn a_fresh_part_releases_every_pin_to_its_pull_up() {
        let shown = chip().state().unwrap();
        assert_eq!(
            (shown.get("outputs"), shown.get("pins")),
            (Some("255"), Some("255")),
            "power-on: every pin released, so every pin reads high"
        );
    }

    #[test]
    fn a_write_is_the_port_and_nothing_else() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x5A], 0), Some(vec![]));
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("outputs"), shown.get("pins")),
            (Some("90"), Some("90")),
            "a released pin reads its pull-up, a driven pin reads low"
        );
    }

    #[test]
    fn the_last_byte_of_a_write_is_the_port() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x0F, 0xF0], 0), Some(vec![]));
        assert_eq!(
            chip.state().unwrap().get("outputs"),
            Some("240"),
            "the part latches each byte in turn, so the last one stays"
        );
    }

    #[test]
    fn the_world_can_pull_a_released_pin_low() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0xAA], 0), Some(vec![]));
        chip.on_input("set:levels=0x0F");
        assert_eq!(
            transfer(&mut chip, &[], 1),
            Some(vec![0x0A]),
            "0xAA released, the world pulling the low nibble low"
        );
    }

    #[test]
    fn a_pin_driven_low_ignores_the_world_driving_it_high() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x00], 0), Some(vec![]));
        chip.on_input("set:levels=0xFF");
        assert_eq!(
            transfer(&mut chip, &[], 1),
            Some(vec![0x00]),
            "the latch's low is a driven low, not a pull-up"
        );
    }

    #[test]
    fn a_read_repeats_the_same_pins() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x3C], 0), Some(vec![]));
        assert_eq!(
            transfer(&mut chip, &[], 3),
            Some(vec![0x3C, 0x3C, 0x3C]),
            "the pins do not move while the transaction runs"
        );
    }

    #[test]
    fn a_probe_is_acknowledged_with_no_data() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[], 0),
            Some(vec![]),
            "a zero-byte write is an address probe"
        );
    }

    #[test]
    fn a_read_only_leaves_the_latch_alone() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x55], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[], 1), Some(vec![0x55]));
        assert_eq!(chip.latch, 0x55, "reading is not writing");
    }

    #[test]
    fn rest_puts_the_power_on_port_back() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x00], 0), Some(vec![]));
        chip.on_input("set:levels=0x00");
        chip.on_input("rest");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("outputs"), shown.get("pins")),
            (Some("255"), Some("255"))
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x20,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x21, &[], 1)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x21 on a 0x20 part");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x20, &[0xFF], 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
