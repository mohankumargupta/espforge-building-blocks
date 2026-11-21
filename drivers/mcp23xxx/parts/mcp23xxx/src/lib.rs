//! A Microchip MCP23017 16-bit I2C I/O expander, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: one transaction
//! is a register index followed by the bytes for it, and the pointer
//! auto-increments, so a multi-byte write walks the register file. The layout
//! is the power-on `IOCON.BANK = 0` one:
//!
//! | Registers | Meaning |
//! |---|---|
//! | `0x00`/`0x01` | IODIRA/IODIRB: a set bit is an input |
//! | `0x02`/`0x03` | IPOLA/IPOLB: input polarity inversion |
//! | `0x04`/`0x05` | GPINTENA/GPINTENB |
//! | `0x06`/`0x07` | DEFVALA/DEFVALB |
//! | `0x08`/`0x09` | INTCONA/INTCONB |
//! | `0x0A`/`0x0B` | IOCON (both addresses are the same byte in bank 0) |
//! | `0x0C`/`0x0D` | GPPUA/GPPUB: the input pull-ups |
//! | `0x0E`/`0x0F` | INTFA/INTFB, read-only |
//! | `0x10`/`0x11` | INTCAPA/INTCAPB, read-only |
//! | `0x12`/`0x13` | GPIOA/GPIOB |
//! | `0x14`/`0x15` | OLATA/OLATB |
//!
//! A pin reads its output latch when it is an output and its (inverted) pin
//! level when it is an input. Interrupts are not modelled, so the interrupt
//! flag and capture registers read zero.
//!
//! README.md lists what is left out.
//!
//! `Mcp23xxx` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with every address pin low.
const DEFAULT_ADDR: u32 = 0x20;

/// The direction register of port A: a set bit is an input.
const IODIR: u8 = 0x00;
/// The input polarity register of port A.
const IPOL: u8 = 0x02;
/// `IOCON`, at 0x0A and again at 0x0B in bank 0.
const IOCON: u8 = 0x0A;
/// The pull-up register of port A.
const GPPU: u8 = 0x0C;
/// The pin register of port A.
const GPIO: u8 = 0x12;
/// The output latch of port A.
const OLAT: u8 = 0x14;
/// The register file: 0x00 to 0x15. A write to anything else is NACKed.
const REGISTERS: u8 = 0x16;

/// The part's power-on direction: every pin an input.
const POWER_ON_IODIR: u8 = 0xFF;

struct Mcp23xxx {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The register file, at its power-on values until the guest writes it.
    regs: [u8; REGISTERS as usize],
    /// The register the next read starts at.
    pointer: u8,
    /// The levels the world holds the sixteen pins at, port A in the low byte.
    levels: u16,
}

impl Default for Mcp23xxx {
    fn default() -> Self {
        let mut regs = [0u8; REGISTERS as usize];
        // Every pin an input; every latch and every other register low.
        regs[IODIR as usize] = POWER_ON_IODIR;
        regs[IODIR as usize + 1] = POWER_ON_IODIR;
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            regs,
            pointer: 0,
            levels: 0,
        }
    }
}

impl Device for Mcp23xxx {
    const NAME: &'static str = "mcp23xxx";
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

    /// `set:levels=<16-bit hex>` is the world driving the pins, port A in the
    /// low byte. `rest` is the power-on register file.
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
            // The part has no register at or above 0x16, and no banking.
            if register >= REGISTERS {
                return Some(reply);
            }
            let mut index = register;
            for byte in data {
                self.write_register(index, *byte);
                index = next(index);
            }
            self.pointer = register;
        }
        if i2c.read_len > 0 {
            reply.data = (0..i2c.read_len)
                .map(|_| {
                    let byte = self.read_register(self.pointer);
                    self.pointer = next(self.pointer);
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
                .set("config", self.word(IODIR))
                .set("outputs", self.word(OLAT))
                .set("polarity", self.word(IPOL))
                .set("pullups", self.word(GPPU))
                .set("inputs", self.pins(0) as u16 | (u16::from(self.pins(1)) << 8)),
        )
    }
}

impl Mcp23xxx {
    /// One register as the guest reads it.
    fn read_register(&self, register: u8) -> u8 {
        match register {
            // Bank 0 maps IOCON at both 0x0A and 0x0B.
            0x0B => self.regs[IOCON as usize],
            // The interrupt flag and capture registers: no interrupts.
            0x0E | 0x0F | 0x10 | 0x11 => 0,
            // The pins: an output reads its latch, an input its level.
            GPIO | 0x13 => self.pins(usize::from(register & 0x01)),
            _ => self.regs[register as usize],
        }
    }

    /// One register as the guest writes it. The pin registers write the output
    /// latches, and the read-only registers ignore the write.
    fn write_register(&mut self, register: u8, value: u8) {
        match register {
            0x0A | 0x0B => self.regs[IOCON as usize] = value,
            0x0E | 0x0F | 0x10 | 0x11 => {}
            GPIO | 0x13 => self.regs[OLAT as usize + usize::from(register & 0x01)] = value,
            _ => self.regs[register as usize] = value,
        }
    }

    /// The level on one port's pins, with the polarity inversion applied to its
    /// input pins.
    fn pins(&self, port: usize) -> u8 {
        let iodir = self.regs[IODIR as usize + port];
        let latch = self.regs[OLAT as usize + port];
        let levels = (self.levels >> (8 * port)) as u8 ^ self.regs[IPOL as usize + port];
        (latch & !iodir) | (levels & iodir)
    }

    /// Two consecutive registers as one sixteen-bit word, low register first.
    fn word(&self, first: u8) -> u16 {
        u16::from(self.read_register(first)) | (u16::from(self.read_register(first + 1)) << 8)
    }
}

/// The register after this one, wrapping at the end of the file.
fn next(register: u8) -> u8 {
    if register + 1 >= REGISTERS {
        0
    } else {
        register + 1
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Mcp23xxx, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the default 0x20 address.
    fn chip() -> Mcp23xxx {
        let mut chip = Mcp23xxx::default();
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
    fn transfer(chip: &mut Mcp23xxx, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// What the driver's constructor sends: both ports as outputs.
    fn initialise(chip: &mut Mcp23xxx) {
        assert_eq!(transfer(chip, &[IODIR, 0x00, 0x00], 0), Some(vec![]));
    }

    #[test]
    fn a_fresh_part_is_all_inputs_with_low_latches() {
        let shown = chip().state().unwrap();
        assert_eq!(
            (
                shown.get("config"),
                shown.get("outputs"),
                shown.get("polarity"),
                shown.get("pullups"),
            ),
            (Some("65535"), Some("0"), Some("0"), Some("0")),
            "power-on: every pin an input, every latch low"
        );
        assert_eq!(shown.get("inputs"), Some("0"));
    }

    #[test]
    fn the_initialisation_makes_every_pin_an_output() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(chip.state().unwrap().get("config"), Some("0"));
    }

    #[test]
    fn a_port_offset_reaches_the_second_port() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x01, 0x7F], 0), Some(vec![]));
        assert_eq!(chip.regs[0x01], 0x7F, "IODIRB");
        assert_eq!(
            chip.state().unwrap().get("config"),
            Some("32767"),
            "port A is still all inputs (0xFF), port B is 0x7F"
        );
    }

    #[test]
    fn an_output_latch_write_drives_the_pins() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(transfer(&mut chip, &[OLAT, 0x5A, 0x00], 0), Some(vec![]));
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("outputs"), shown.get("inputs")),
            (Some("90"), Some("90")),
            "an output pin reads back its latch"
        );
    }

    #[test]
    fn a_pin_register_write_sets_the_latch() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(transfer(&mut chip, &[GPIO, 0x0F], 0), Some(vec![]));
        assert_eq!(chip.regs[OLAT as usize], 0x0F, "GPIO writes OLATA");
    }

    #[test]
    fn an_input_pin_reads_its_pin_and_an_output_its_latch() {
        let mut chip = chip();
        // Port A outputs, port B inputs with every pull-up on.
        assert_eq!(transfer(&mut chip, &[IODIR, 0x00, 0xFF], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[GPPU, 0x00, 0xFF], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[OLAT, 0x0F], 0), Some(vec![]));
        chip.on_input("set:levels=0xFF00");
        assert_eq!(
            transfer(&mut chip, &[GPIO], 2),
            Some(vec![0x0F, 0xFF]),
            "an output pin reads its latch, an input pin reads the world"
        );
        assert_eq!(
            chip.state().unwrap().get("pullups"),
            Some("65280"),
            "GPPU reads back"
        );
    }

    #[test]
    fn the_polarity_inverts_input_pins_only() {
        let mut chip = chip();
        // Both ports inverted, port A inputs and port B outputs.
        assert_eq!(transfer(&mut chip, &[IPOL, 0xFF, 0xFF], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[IODIR, 0xFF, 0x00], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[OLAT, 0x00, 0x0F], 0), Some(vec![]));
        chip.on_input("set:levels=0x0000");
        assert_eq!(
            transfer(&mut chip, &[GPIO], 2),
            Some(vec![0xFF, 0x0F]),
            "the input pins read inverted, the output pins unchanged"
        );
    }

    #[test]
    fn a_read_walks_the_register_file() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(transfer(&mut chip, &[OLAT, 0x11, 0x22], 0), Some(vec![]));
        assert_eq!(
            transfer(&mut chip, &[GPIO], 4),
            Some(vec![0x11, 0x22, 0x11, 0x22]),
            "GPIOA, GPIOB, OLATA, OLATB in one read"
        );
    }

    #[test]
    fn interrupts_are_not_modelled_and_read_zero() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[0x0E], 4),
            Some(vec![0x00, 0x00, 0x00, 0x00]),
            "INTFA, INTFB, INTCAPA, INTCAPB"
        );
    }

    #[test]
    fn iocon_reads_back_at_both_of_its_addresses() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x0A, 0x20], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[0x0A], 1), Some(vec![0x20]));
        assert_eq!(
            transfer(&mut chip, &[0x0B], 1),
            Some(vec![0x20]),
            "bank 0 maps IOCON at 0x0A and 0x0B"
        );
    }

    #[test]
    fn a_register_the_part_lacks_is_nacked() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x16], 0), None);
        assert_eq!(transfer(&mut chip, &[0xFF], 0), None);
    }

    #[test]
    fn rest_puts_the_power_on_values_back() {
        let mut chip = chip();
        initialise(&mut chip);
        let _ = transfer(&mut chip, &[OLAT, 0xFF, 0xFF], 0);
        chip.on_input("rest");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("config"), shown.get("outputs")),
            (Some("65535"), Some("0"))
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
        let mut request = request(0x20, &[IODIR, 0x00], 0);
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
        let reply = chip.on_bus(&request(0x21, &[GPIO], 1)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x21 on a 0x20 part");
        assert!(reply.data.is_empty());
    }
}
