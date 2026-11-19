//! A TCA9535 16-bit I2C I/O expander, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: one
//! transaction is a register index followed by the bytes for it, and the
//! pointer auto-increments between them, so a multi-byte write walks the
//! register file:
//!
//! - `0x00` and `0x01` are the input ports, one per 8-bit port,
//! - `0x02` and `0x03` are the output latches,
//! - `0x04` and `0x05` are the polarity inversion registers,
//! - `0x06` and `0x07` are the configuration registers: a set bit is an
//!   input, so a cleared register drives that whole port.
//!
//! The input port answers the output latch on pins configured as outputs and
//! the part's pin levels on the others (all high, as the pull-ups leave them).
//!
//! README.md lists what is left out.
//!
//! `Tca95xx` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with every address pin low.
const DEFAULT_ADDR: u32 = 0x20;

/// The configuration register of the first port: a set bit is an input.
const CONFIGURATION0: u8 = 0x06;
/// The output latch of the first port; the second port follows it.
const OUTPUT_PORT0: u8 = 0x02;
/// The input port of the first port; the second port follows it.
const INPUT_PORT0: u8 = 0x00;
/// The polarity inversion register of the first port.
const POLARITY0: u8 = 0x04;

/// The part's power-on register values: every pin an input, every latch high,
/// no inversion.
const POWER_ON_CONFIGURATION: u8 = 0xFF;
const POWER_ON_OUTPUT: u8 = 0xFF;

/// The pin levels the world holds the inputs at: nothing drives them, so the
/// pull-ups do.
const IDLE_LEVELS: u16 = 0xFFFF;

struct Tca95xx {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The register file: the two input ports, the two output latches, the two
    /// polarity registers and the two configuration registers.
    regs: [u8; 0x08],
    /// Where the next read starts: the register the last write addressed.
    pointer: u8,
    /// The levels on the sixteen pins, for the ones configured as inputs.
    levels: u16,
}

impl Default for Tca95xx {
    fn default() -> Self {
        let mut regs = [0u8; 0x08];
        for port in 0..2 {
            regs[(OUTPUT_PORT0 + port) as usize] = POWER_ON_OUTPUT;
            regs[(CONFIGURATION0 + port) as usize] = POWER_ON_CONFIGURATION;
        }
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            regs,
            pointer: INPUT_PORT0,
            levels: IDLE_LEVELS,
        }
    }
}

impl Device for Tca95xx {
    const NAME: &'static str = "tca95xx";
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

    /// `rest` puts the register file back to its power-on values. `set:levels=`
    /// is the world driving the pins configured as inputs.
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
            if let Ok(levels) = u16::from_str_radix(value.trim_start_matches("0x"), 16) {
                self.levels = levels;
            }
        }
    }

    /// The guest's I2C transaction: a register index, then the bytes for it.
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
        if let Some((&register, data)) = i2c.write.split_first() {
            let mut index = register;
            for byte in data {
                // The input ports are not writable: the pins decide them.
                if index != INPUT_PORT0 && index != INPUT_PORT0 + 1 {
                    self.regs[index as usize & 0x07] = *byte;
                }
                index = index.wrapping_add(1);
            }
            self.pointer = register;
        }
        if i2c.read_len > 0 {
            reply.data = (0..i2c.read_len)
                .map(|_| {
                    let byte = self.read_register(self.pointer & 0x07);
                    self.pointer = self.pointer.wrapping_add(1);
                    byte
                })
                .collect();
        }
        // A probe, a register write, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("config", self.port_word(CONFIGURATION0))
                .set("outputs", self.port_word(OUTPUT_PORT0))
                .set("polarity", self.port_word(POLARITY0))
                .set("inputs", self.port_word(INPUT_PORT0)),
        )
    }
}

impl Tca95xx {
    /// One register as the guest reads it: the input ports answer the pins,
    /// every other register is the byte that was written.
    fn read_register(&self, register: u8) -> u8 {
        let (port, base) = if register <= INPUT_PORT0 + 1 {
            (register - INPUT_PORT0, INPUT_PORT0)
        } else {
            (register & 1, register & !1)
        };
        if base == INPUT_PORT0 {
            let config = self.regs[(CONFIGURATION0 + port) as usize];
            let latch = self.regs[(OUTPUT_PORT0 + port) as usize];
            let levels = (self.levels >> (8 * port)) as u8;
            // An output pin reads back its latch; an input pin reads the pin.
            return (latch & !config) | (levels & config);
        }
        self.regs[register as usize]
    }

    /// Two consecutive registers as one sixteen-bit word, low register first.
    fn port_word(&self, first: u8) -> u16 {
        u16::from(self.read_register(first)) | (u16::from(self.read_register(first + 1)) << 8)
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Tca95xx, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the default 0x20 address.
    fn chip() -> Tca95xx {
        let mut chip = Tca95xx::default();
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
    fn transfer(chip: &mut Tca95xx, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// What the driver's constructor sends: both ports as outputs.
    fn initialise(chip: &mut Tca95xx) {
        assert_eq!(transfer(chip, &[CONFIGURATION0, 0x00, 0x00], 0), Some(vec![]));
    }

    #[test]
    fn a_fresh_part_is_all_inputs_with_high_latches() {
        let chip = chip();
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("config"), shown.get("outputs"), shown.get("polarity")),
            (Some("65535"), Some("65535"), Some("0")),
            "power-on: every pin an input, every latch high, no inversion"
        );
        assert_eq!(shown.get("inputs"), Some("65535"));
    }

    #[test]
    fn the_initialisation_makes_every_pin_an_output() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(chip.state().unwrap().get("config"), Some("0"));
    }

    #[test]
    fn a_word_write_drives_both_ports() {
        let mut chip = chip();
        initialise(&mut chip);
        // 0x00A5: P0.0, P0.2, P0.5 and P0.7 high.
        assert_eq!(transfer(&mut chip, &[OUTPUT_PORT0, 0xA5, 0x00], 0), Some(vec![]));
        let shown = chip.state().unwrap();
        assert_eq!((shown.get("outputs"), shown.get("inputs")), (Some("165"), Some("165")));
    }

    #[test]
    fn the_pointer_walks_past_the_first_port() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(transfer(&mut chip, &[OUTPUT_PORT0, 0x34, 0x12], 0), Some(vec![]));
        assert_eq!(
            (chip.regs[0x02], chip.regs[0x03]),
            (0x34, 0x12),
            "one transaction reaches both output latches"
        );
        assert_eq!(chip.state().unwrap().get("outputs"), Some("4660"));
    }

    #[test]
    fn the_configuration_can_span_both_ports() {
        let mut chip = chip();
        // Port 1 back to inputs, port 0 outputs.
        assert_eq!(transfer(&mut chip, &[CONFIGURATION0, 0x00, 0xFF], 0), Some(vec![]));
        assert_eq!(chip.state().unwrap().get("config"), Some("65280"));
    }

    #[test]
    fn an_input_pin_reads_its_pin_and_an_output_its_latch() {
        let mut chip = chip();
        // Port 0 all outputs at 0x0F; port 1 still inputs.
        assert_eq!(transfer(&mut chip, &[CONFIGURATION0, 0x00, 0xFF], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[OUTPUT_PORT0, 0x0F, 0x00], 0), Some(vec![]));
        chip.on_input("set:levels=0x0000");
        assert_eq!(
            transfer(&mut chip, &[INPUT_PORT0], 2),
            Some(vec![0x0F, 0x00]),
            "an output pin reads its latch, an input pin with no drive reads low"
        );
        chip.on_input("set:levels=0xFF00");
        assert_eq!(
            transfer(&mut chip, &[INPUT_PORT0 + 1], 1),
            Some(vec![0xFF]),
            "an input pin reads the world"
        );
    }

    #[test]
    fn a_read_answers_from_the_register_file() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(
            transfer(&mut chip, &[OUTPUT_PORT0], 2),
            Some(vec![0xFF, 0xFF]),
            "the power-on latches are still high"
        );
        assert_eq!(
            transfer(&mut chip, &[CONFIGURATION0], 1),
            Some(vec![0x00]),
            "the configuration reads back"
        );
    }

    #[test]
    fn rest_puts_the_power_on_values_back() {
        let mut chip = chip();
        initialise(&mut chip);
        let _ = transfer(&mut chip, &[OUTPUT_PORT0, 0x00, 0x00], 0);
        chip.on_input("rest");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("config"), shown.get("outputs")),
            (Some("65535"), Some("65535"))
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
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x20, &[CONFIGURATION0, 0x00], 0);
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
        let reply = chip.on_bus(&request(0x21, &[CONFIGURATION0, 0x00], 0)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x21");
        assert!(reply.data.is_empty());
    }
}
