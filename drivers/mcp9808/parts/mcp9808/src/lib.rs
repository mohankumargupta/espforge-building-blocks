//! A Microchip MCP9808 temperature sensor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: a register
//! address byte followed by a read or a write, with the pointer
//! auto-incrementing. 16-bit big-endian registers:
//!
//! - 0x00 `configuration`, which powers up at 0x0000 (continuous conversion,
//!   comparator output, alerts off) and is plain storage,
//! - 0x05 `ambient_temperature`, 0.0625 °C per signed two's complement count
//!   in bits 12:0, alert flags in bits 15:13,
//! - 0x06 `manufacturer_id`, 0x0054,
//! - 0x07 `device_id`, 0x0400,
//! - 0x08 `resolution`, which powers up at 0x0003 (0.0625 °C per count) and
//!   is plain storage.
//!
//! The part keeps the temperature it is asked to report and encodes the
//! ambient register from it. README.md lists what is left out.
//!
//! `Mcp9808` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The address with all three address pins low; 0x19 to 0x1F are the other
/// straps.
const DEFAULT_ADDR: u32 = 0x18;

/// The registers, each two bytes. 0x01 to 0x04 and 0x09 to 0x0F are the alert
/// limits and reserved addresses, which this part does not model.
const CONFIG: u8 = 0x00;
const AMBIENT: u8 = 0x05;
const MANUFACTURER: u8 = 0x06;
const DEVICE: u8 = 0x07;
const RESOLUTION: u8 = 0x08;
const REGISTERS: u8 = 9;

const MANUFACTURER_ID: u16 = 0x0054;
const DEVICE_ID: u16 = 0x0400;

/// The temperature is the low 13 bits; bit 12 is its sign.
const TEMPERATURE_MASK: u16 = 0x1FFF;
/// One count of the temperature register.
const TEMPERATURE_LSB_C: f64 = 0.0625;

/// What it reads when nobody has changed it: 24.00 °C, room temperature.
const DEFAULT_TEMPERATURE: f64 = 24.0;

/// The part's operating range.
const MIN_TEMPERATURE: f64 = -40.0;
const MAX_TEMPERATURE: f64 = 125.0;

struct Mcp9808 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Register 0x00, as written.
    config: u16,
    /// Register 0x08, as written.
    resolution: u16,
    /// What it reports, in degrees Celsius.
    temperature: f64,
}

impl Default for Mcp9808 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            config: 0x0000,
            resolution: 0x0003,
            temperature: DEFAULT_TEMPERATURE,
        }
    }
}

impl Device for Mcp9808 {
    const NAME: &'static str = "mcp9808";
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

    /// The address this sensor answers for, on the two pins it is wired to.
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

    /// Change the temperature: `set:temperature=<C>`, or `rest` for the
    /// default.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            if what == "temperature" {
                self.temperature = value.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE);
            }
        } else if name == "rest" {
            self.temperature = DEFAULT_TEMPERATURE;
        }
    }

    /// The guest's I2C transaction: a register address, then a read or a
    /// write. A write of several bytes stores them in the registers that
    /// follow, and a read answers them in order.
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
        let register = i2c.write.first().copied().unwrap_or(0);
        for (offset, &byte) in i2c.write[1..].iter().enumerate() {
            self.write_byte(register, offset, byte);
        }
        if i2c.read_len > 0 {
            reply.data = (0..i2c.read_len as usize)
                .map(|offset| self.byte(register, offset))
                .collect();
        }
        // A probe, a register write, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(State::new().set("temperature", self.temperature))
    }
}

impl Mcp9808 {
    /// The 16-bit register at `index`.
    fn word(&self, index: u8) -> u16 {
        match index {
            CONFIG => self.config,
            AMBIENT => self.ambient_counts(),
            MANUFACTURER => MANUFACTURER_ID,
            DEVICE => DEVICE_ID,
            RESOLUTION => self.resolution,
            _ => 0,
        }
    }

    /// Byte `offset` of an answer that started at register `register`: each
    /// register is two bytes, most significant first, and the register index
    /// wraps within the nine.
    fn byte(&self, register: u8, offset: usize) -> u8 {
        let index = register.wrapping_add((offset / 2) as u8) % REGISTERS;
        let word = self.word(index);
        if offset % 2 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        }
    }

    /// One register byte write. Only 0x00 and 0x08 are writable; the
    /// identification and ambient registers are read-only, as on the chip.
    fn write_byte(&mut self, register: u8, offset: usize, value: u8) {
        let index = register.wrapping_add((offset / 2) as u8) % REGISTERS;
        let high = offset % 2 == 0;
        match index {
            CONFIG => {
                self.config = if high {
                    (self.config & 0x00FF) | (u16::from(value) << 8)
                } else {
                    (self.config & 0xFF00) | u16::from(value)
                }
            }
            RESOLUTION => {
                self.resolution = if high {
                    (self.resolution & 0x00FF) | (u16::from(value) << 8)
                } else {
                    (self.resolution & 0xFF00) | u16::from(value)
                }
            }
            _ => {}
        }
    }

    /// The ambient temperature register: 0.0625 °C per signed two's complement
    /// count in the low 13 bits, alert flags clear, held to the part's range.
    fn ambient_counts(&self) -> u16 {
        let counts = (self.temperature / TEMPERATURE_LSB_C)
            .round()
            .clamp(-4096.0, 4095.0) as i32;
        (counts as u16) & TEMPERATURE_MASK
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Mcp9808, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the 0x18 strap.
    fn chip() -> Mcp9808 {
        let mut chip = Mcp9808::default();
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

    /// One transaction to the sensor's address: the bytes read back, or
    /// `None` for a NACK.
    fn transfer(chip: &mut Mcp9808, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    #[test]
    fn the_ambient_register_is_room_temperature() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[AMBIENT], 2),
            Some(vec![0x01, 0x80]),
            "24.00 °C is 384 counts of 0.0625"
        );
    }

    #[test]
    fn the_inputs_reencode_the_ambient_register() {
        let mut chip = chip();
        chip.on_input("set:temperature=30");
        assert_eq!(
            transfer(&mut chip, &[AMBIENT], 2),
            Some(vec![0x01, 0xE0]),
            "30.00 °C is 480 counts"
        );
        chip.on_input("set:temperature=-5.5");
        assert_eq!(
            transfer(&mut chip, &[AMBIENT], 2),
            Some(vec![0x1F, 0xA8]),
            "-5.50 °C is -88 counts, two's complement in the low 13 bits"
        );
    }

    #[test]
    fn a_negative_temperature_is_two_s_complement() {
        let mut chip = chip();
        chip.on_input("set:temperature=-25");
        assert_eq!(
            transfer(&mut chip, &[AMBIENT], 2),
            Some(vec![0x1E, 0x70]),
            "-25.00 °C is -400 counts"
        );
        assert_eq!(chip.temperature, -25.0);
    }

    #[test]
    fn the_inputs_stop_at_the_datasheet_range() {
        let mut chip = chip();
        chip.on_input("set:temperature=200");
        assert_eq!(chip.temperature, MAX_TEMPERATURE);
        chip.on_input("set:temperature=-100");
        assert_eq!(chip.temperature, MIN_TEMPERATURE);
    }

    #[test]
    fn rest_puts_the_default_temperature_back() {
        let mut chip = chip();
        chip.on_input("set:temperature=-5");
        chip.on_input("rest");
        assert_eq!(
            transfer(&mut chip, &[AMBIENT], 2),
            Some(vec![0x01, 0x80])
        );
    }

    #[test]
    fn it_identifies_itself() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[MANUFACTURER], 2),
            Some(vec![0x00, 0x54])
        );
        assert_eq!(
            transfer(&mut chip, &[DEVICE], 2),
            Some(vec![0x04, 0x00])
        );
    }

    #[test]
    fn the_configuration_and_resolution_power_up_and_are_writable() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[CONFIG], 2), Some(vec![0x00, 0x00]));
        assert_eq!(
            transfer(&mut chip, &[RESOLUTION], 2),
            Some(vec![0x00, 0x03]),
            "0.0625 °C per count"
        );
        assert_eq!(
            transfer(&mut chip, &[RESOLUTION, 0x00, 0x02], 0),
            Some(vec![])
        );
        assert_eq!(transfer(&mut chip, &[RESOLUTION], 2), Some(vec![0x00, 0x02]));
    }

    #[test]
    fn the_ambient_register_is_read_only() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[AMBIENT, 0x7F, 0xFF], 0),
            Some(vec![]),
            "a write to the measurement is acknowledged"
        );
        assert_eq!(
            transfer(&mut chip, &[AMBIENT], 2),
            Some(vec![0x01, 0x80]),
            "and ignored"
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x18,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_temperature() {
        let mut chip = chip();
        chip.on_input("set:temperature=30");
        assert_eq!(chip.state().unwrap().get("temperature"), Some("30"));
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x19, &[AMBIENT], 2)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x19");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x18, &[AMBIENT], 2);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
