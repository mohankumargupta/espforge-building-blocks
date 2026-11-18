//! A TI INA219 current and power monitor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: a register
//! address byte followed by a read or a write, with the pointer
//! auto-incrementing. Six 16-bit big-endian registers:
//!
//! - 0x00 `configuration`, which powers up at 0x399F and is plain storage,
//! - 0x01 `shunt_voltage`, 10 µV per signed count,
//! - 0x02 `bus_voltage`, 4 mV per count in bits 15:3, flags in bits 2:0,
//! - 0x03 `power`, 2 mW per count,
//! - 0x04 `current`, 100 µA per signed count,
//! - 0x05 `calibration`, which powers up at 0 and is plain storage.
//!
//! The part keeps what the load draws and the bus it draws it from, and
//! encodes the data registers from those two values the way the datasheet's
//! calibration does for a 0.1 Ω shunt. README.md lists what is left out.
//!
//! `Ina219` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The address with both address pins low; 0x41 to 0x4F are the other straps.
const DEFAULT_ADDR: u32 = 0x40;

/// The six registers, each two bytes.
const CONFIG: u8 = 0x00;
const SHUNT: u8 = 0x01;
const BUS: u8 = 0x02;
const POWER: u8 = 0x03;
const CURRENT: u8 = 0x04;
const CALIBRATION: u8 = 0x05;
const REGISTERS: u8 = 6;

/// The configuration the driver writes: 16 V range, ±320 mV gain, 12-bit
/// 32-sample averaging on both, shunt and bus continuous. It is also the
/// power-on value.
const CONFIG_VALUE: u16 = 0x399F;

/// The scale factors of a 0.1 Ω shunt at a 100 µA current LSB, as the driver
/// calibrates the part.
const SHUNT_OHMS: f64 = 0.1;
const CURRENT_LSB_A: f64 = 0.000_1;
const POWER_LSB_W: f64 = 20.0 * CURRENT_LSB_A;
const BUS_VOLTAGE_LSB_V: f64 = 0.004;
const SHUNT_VOLTAGE_LSB_V: f64 = 0.000_01;

/// Bit 1 of the bus voltage register: a conversion is ready.
const CONVERSION_READY: u16 = 0b010;

/// What the load draws when nobody has changed it: 5 V at 100 mA.
const DEFAULT_BUS_VOLTAGE: f64 = 5.0;
const DEFAULT_CURRENT: f64 = 0.1;

/// The 16 V bus range and the ±320 mV shunt range, in the units the part
/// keeps.
const MIN_BUS_VOLTAGE: f64 = 0.0;
const MAX_BUS_VOLTAGE: f64 = 16.0;
const MAX_CURRENT: f64 = 0.32 / SHUNT_OHMS;

struct Ina219 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Register 0x00, as written.
    config: u16,
    /// Register 0x05, as written.
    calibration: u16,
    /// The bus the load draws from, in volts.
    bus_voltage: f64,
    /// What the load draws through the shunt, in amps.
    current: f64,
}

impl Default for Ina219 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            config: CONFIG_VALUE,
            calibration: 0,
            bus_voltage: DEFAULT_BUS_VOLTAGE,
            current: DEFAULT_CURRENT,
        }
    }
}

impl Device for Ina219 {
    const NAME: &'static str = "ina219";
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

    /// The address this monitor answers for, on the two pins it is wired to.
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

    /// Change the load: `set:voltage=<V>`, `set:current=<A>`, or `rest` for
    /// the defaults.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "voltage" => {
                    self.bus_voltage = value.clamp(MIN_BUS_VOLTAGE, MAX_BUS_VOLTAGE)
                }
                "current" => self.current = value.clamp(-MAX_CURRENT, MAX_CURRENT),
                _ => {}
            }
        } else if name == "rest" {
            self.bus_voltage = DEFAULT_BUS_VOLTAGE;
            self.current = DEFAULT_CURRENT;
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
        Some(
            State::new()
                .set("bus_voltage", self.bus_voltage)
                .set("current", self.current)
                .set("shunt_voltage_mv", self.shunt_counts() as f64 * SHUNT_VOLTAGE_LSB_V * 1000.0),
        )
    }
}

impl Ina219 {
    /// The 16-bit register at `index`.
    fn word(&self, index: u8) -> u16 {
        match index {
            CONFIG => self.config,
            SHUNT => self.shunt_counts() as u16,
            BUS => self.bus_counts() | CONVERSION_READY,
            POWER => self.power_counts(),
            CURRENT => self.current_counts() as u16,
            CALIBRATION => self.calibration,
            _ => 0,
        }
    }

    /// Byte `offset` of an answer that started at register `register`: each
    /// register is two bytes, most significant first, and the register index
    /// wraps within the six.
    fn byte(&self, register: u8, offset: usize) -> u8 {
        let index = register.wrapping_add((offset / 2) as u8) % REGISTERS;
        let word = self.word(index);
        if offset % 2 == 0 {
            (word >> 8) as u8
        } else {
            word as u8
        }
    }

    /// One register byte write. Only 0x00 and 0x05 are writable; the data
    /// registers are read-only, as on the chip.
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
            CALIBRATION => {
                self.calibration = if high {
                    (self.calibration & 0x00FF) | (u16::from(value) << 8)
                } else {
                    (self.calibration & 0xFF00) | u16::from(value)
                }
            }
            _ => {}
        }
    }

    /// The shunt voltage register: the drop across the shunt at 10 µV per
    /// count, signed and held to the ±320 mV range.
    fn shunt_counts(&self) -> i16 {
        (self.current * SHUNT_OHMS / SHUNT_VOLTAGE_LSB_V)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
    }

    /// The current register at 100 µA per count, signed.
    fn current_counts(&self) -> i16 {
        (self.current / CURRENT_LSB_A)
            .round()
            .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
    }

    /// The bus voltage register's top 13 bits: 4 mV per count, held to the
    /// 16 V range.
    fn bus_counts(&self) -> u16 {
        ((self.bus_voltage / BUS_VOLTAGE_LSB_V).round().clamp(0.0, 4095.0) as u16) << 3
    }

    /// The power register: `bus * current` at 2 mW per count.
    fn power_counts(&self) -> u16 {
        (self.bus_voltage * self.current / POWER_LSB_W)
            .round()
            .clamp(0.0, f64::from(u16::MAX)) as u16
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Ina219, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the 0x40 strap.
    fn chip() -> Ina219 {
        let mut chip = Ina219::default();
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

    /// One transaction to the monitor's address: the bytes read back, or
    /// `None` for a NACK.
    fn transfer(chip: &mut Ina219, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// The four data registers, the way the driver reads them.
    fn data(chip: &mut Ina219) -> Vec<u8> {
        transfer(chip, &[SHUNT], 8).expect("eight data bytes")
    }

    #[test]
    fn the_default_load_is_five_volts_at_a_hundred_milliamps() {
        let mut chip = chip();
        assert_eq!(
            data(&mut chip),
            [0x03, 0xE8, 0x27, 0x12, 0x00, 0xFA, 0x03, 0xE8],
            "10.00 mV, 5.00 V, 500 mW, 100.0 mA"
        );
    }

    #[test]
    fn the_inputs_reencode_the_data_registers() {
        let mut chip = chip();
        chip.on_input("set:voltage=12");
        chip.on_input("set:current=1");
        assert_eq!(
            data(&mut chip),
            [0x27, 0x10, 0x5D, 0xC2, 0x17, 0x70, 0x27, 0x10],
            "100.0 mV, 12.00 V, 12 W, 1000 mA"
        );
    }

    #[test]
    fn the_two_byte_registers_are_stored_from_the_pointer() {
        let mut chip = chip();
        // A read of one register's two bytes: the high byte first.
        assert_eq!(
            transfer(&mut chip, &[BUS], 2),
            Some(vec![0x27, 0x12]),
            "5.00 V and the conversion-ready flag"
        );
        // A read that runs past the last register wraps to the first.
        assert_eq!(
            transfer(&mut chip, &[CALIBRATION], 2),
            Some(vec![0x00, 0x00])
        );
    }

    #[test]
    fn a_negative_current_is_two_s_complement() {
        let mut chip = chip();
        chip.on_input("set:current=-0.25");
        let bytes = data(&mut chip);
        // -2500 counts is 0xF63C.
        assert_eq!(&bytes[0..2], [0xF6, 0x3C], "shunt voltage");
        assert_eq!(&bytes[6..8], [0xF6, 0x3C], "current");
        assert_eq!(chip.current, -0.25);
    }

    #[test]
    fn the_bus_voltage_leaves_the_flag_bits_alone() {
        let mut chip = chip();
        let bytes = data(&mut chip);
        // 1250 counts at 4 mV, shifted up three bits, with the
        // conversion-ready flag in bit 1.
        assert_eq!(&bytes[2..4], [0x27, 0x12]);
        assert_eq!(u16::from_be_bytes([bytes[2], bytes[3]]) & 0b111, 0b010);
    }

    #[test]
    fn the_configuration_powers_up_at_0x399f_and_is_writable() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[CONFIG], 2), Some(vec![0x39, 0x9F]));
        assert_eq!(
            transfer(&mut chip, &[CONFIG, 0x1F, 0xFF], 0),
            Some(vec![]),
            "a two-byte register write"
        );
        assert_eq!(transfer(&mut chip, &[CONFIG], 2), Some(vec![0x1F, 0xFF]));
        // A read of the four data registers starts at 0x01, whatever the
        // configuration holds.
        assert_eq!(data(&mut chip)[0], 0x03, "the shunt's high byte first");
    }

    #[test]
    fn the_calibration_powers_up_at_zero_and_stores_the_drivers_value() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[CALIBRATION], 2), Some(vec![0x00, 0x00]));
        assert_eq!(
            transfer(&mut chip, &[CALIBRATION, 0x10, 0x00], 0),
            Some(vec![])
        );
        assert_eq!(chip.calibration, 4096);
        assert_eq!(transfer(&mut chip, &[CALIBRATION], 2), Some(vec![0x10, 0x00]));
    }

    #[test]
    fn the_data_registers_are_read_only() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[SHUNT, 0xFF, 0xFF], 0),
            Some(vec![]),
            "a write to a data register is acknowledged"
        );
        assert_eq!(&data(&mut chip)[0..2], [0x03, 0xE8], "and ignored");
    }

    #[test]
    fn the_inputs_stop_at_the_datasheet_ranges() {
        let mut chip = chip();
        chip.on_input("set:voltage=32");
        assert_eq!(chip.bus_voltage, MAX_BUS_VOLTAGE);
        chip.on_input("set:voltage=-5");
        assert_eq!(chip.bus_voltage, MIN_BUS_VOLTAGE);
        chip.on_input("set:current=10");
        assert_eq!(chip.current, MAX_CURRENT);
        chip.on_input("set:current=-10");
        assert_eq!(chip.current, -MAX_CURRENT);
    }

    #[test]
    fn rest_puts_the_default_load_back() {
        let mut chip = chip();
        chip.on_input("set:voltage=12");
        chip.on_input("set:current=1");
        chip.on_input("rest");
        assert_eq!(
            data(&mut chip),
            [0x03, 0xE8, 0x27, 0x12, 0x00, 0xFA, 0x03, 0xE8]
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x40,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_load() {
        let mut chip = chip();
        chip.on_input("set:voltage=12");
        chip.on_input("set:current=1");
        let shown = chip.state().unwrap();
        assert_eq!(
            (
                shown.get("bus_voltage"),
                shown.get("current"),
                shown.get("shunt_voltage_mv")
            ),
            (Some("12"), Some("1"), Some("100"))
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x41, &[SHUNT], 1)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x41");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x40, &[SHUNT], 1);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
