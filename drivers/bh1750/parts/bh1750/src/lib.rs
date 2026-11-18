//! A ROHM BH1750 ambient light sensor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: one-byte
//! commands on the bus, and two big-endian bytes of raw counts back for a
//! measurement. The part keeps what the sensor sees in lux and encodes it with
//! the datasheet's counts-per-lux for the mode that was selected — high
//! resolution (1.2), high resolution 2 (2.4) or low resolution (0.15).
//!
//! There is no time in the part model, so a measurement latches the current
//! light as soon as its command arrives. README.md lists what is left out.
//!
//! `Bh1750` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with its ADDR pin low; the other strapping is 0x5C.
const DEFAULT_ADDR: u32 = 0x23;

/// The one-byte commands the datasheet defines.
const POWER_DOWN: u8 = 0x00;
const POWER_ON: u8 = 0x01;
const RESET: u8 = 0x07;
const CONTINUOUS_HIGH_RES: u8 = 0x10;
const CONTINUOUS_HIGH_RES2: u8 = 0x11;
const CONTINUOUS_LOW_RES: u8 = 0x13;
const ONE_SHOT_HIGH_RES: u8 = 0x20;
const ONE_SHOT_HIGH_RES2: u8 = 0x21;
const ONE_SHOT_LOW_RES: u8 = 0x23;

/// Raw counts per lux, per mode, from the datasheet.
const HIGH_RES_COUNTS_PER_LUX: f64 = 1.2;
const HIGH_RES2_COUNTS_PER_LUX: f64 = 2.4;
const LOW_RES_COUNTS_PER_LUX: f64 = 0.15;

/// The largest count the two data bytes can hold, in lux at high resolution
/// (65535 / 1.2).
const MAX_LUX: f64 = 54_612.5;
/// What the sensor sees when nobody has changed it: an ordinary lit room.
const DEFAULT_LUX: f64 = 300.0;

struct Bh1750 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Whether the part is in power-on rather than power-down state. It also
    /// stays on across a measurement command, as the datasheet says.
    powered: bool,
    /// The last measurement, as the two data-register bytes hold it.
    raw: u16,
    /// What the sensor sees, in lux.
    lux: f64,
}

impl Default for Bh1750 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            powered: false,
            // The power-on state is power down, so there is no measurement.
            raw: 0,
            lux: DEFAULT_LUX,
        }
    }
}

impl Device for Bh1750 {
    const NAME: &'static str = "bh1750";
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

    /// Change what the sensor sees. `set:lux=<value>` sets the illuminance;
    /// `dark` makes it 0 lx and `rest` puts the default back. The next
    /// measurement command latches it.
    fn on_input(&mut self, name: &str) {
        if let Some(("lux", value)) = set_value(name) {
            self.lux = value.clamp(0.0, MAX_LUX);
        } else if name == "dark" {
            self.lux = 0.0;
        } else if name == "rest" {
            self.lux = DEFAULT_LUX;
        }
    }

    /// The guest's I2C transaction: a one-byte command, a two-byte
    /// measurement, or both in one transaction.
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
        if let Some(&command) = i2c.write.first() {
            self.command(command);
        }
        if i2c.read_len > 0 {
            let bytes = self.raw.to_be_bytes();
            let mut data = vec![0u8; i2c.read_len as usize];
            for (index, byte) in data.iter_mut().enumerate() {
                *byte = bytes[index.min(bytes.len() - 1)];
            }
            reply.data = data;
        }
        // A probe, a command, or a measurement with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(State::new().set("lux", self.lux))
    }
}

impl Bh1750 {
    /// One command byte: power state, reset, or a measurement in one of the
    /// three resolutions.
    fn command(&mut self, command: u8) {
        match command {
            POWER_DOWN => self.powered = false,
            POWER_ON => self.powered = true,
            RESET => {
                // The reset returns the part to its power-down state with no
                // measurement; what the sensor sees stays.
                self.powered = false;
                self.raw = 0;
            }
            CONTINUOUS_HIGH_RES | ONE_SHOT_HIGH_RES => {
                self.measure(HIGH_RES_COUNTS_PER_LUX)
            }
            CONTINUOUS_HIGH_RES2 | ONE_SHOT_HIGH_RES2 => {
                self.measure(HIGH_RES2_COUNTS_PER_LUX)
            }
            CONTINUOUS_LOW_RES | ONE_SHOT_LOW_RES => {
                self.measure(LOW_RES_COUNTS_PER_LUX)
            }
            _ => {}
        }
    }

    /// Latch the current light as raw counts for `counts_per_lux`.
    fn measure(&mut self, counts_per_lux: f64) {
        self.powered = true;
        self.raw = (self.lux * counts_per_lux).round().clamp(0.0, 65_535.0) as u16;
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Bh1750, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the part's ADDRESS-low address.
    fn chip() -> Bh1750 {
        let mut chip = Bh1750::default();
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
    fn transfer(chip: &mut Bh1750, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// One measurement, the way the driver takes it: the one-shot command,
    /// then the two data bytes.
    fn measurement(chip: &mut Bh1750, command: u8) -> u16 {
        assert_eq!(transfer(chip, &[command], 0), Some(vec![]));
        let bytes = transfer(chip, &[], 2).expect("two data bytes");
        u16::from_be_bytes([bytes[0], bytes[1]])
    }

    #[test]
    fn the_power_on_part_has_no_measurement() {
        let chip = chip();
        assert!(!chip.powered, "power-on state is power down");
        assert_eq!(chip.raw, 0);
    }

    #[test]
    fn a_one_shot_measurement_latches_the_default_light() {
        let mut chip = chip();
        // 300 lx at 1.2 counts per lux is 360 counts, 0x0168.
        assert_eq!(measurement(&mut chip, ONE_SHOT_HIGH_RES), 0x0168);
        assert!(chip.powered, "a measurement command powers the part on");
    }

    #[test]
    fn the_data_bytes_are_big_endian() {
        let mut chip = chip();
        chip.on_input("set:lux=1000");
        // 1200 counts, 0x04B0.
        let bytes = {
            transfer(&mut chip, &[ONE_SHOT_HIGH_RES], 0);
            transfer(&mut chip, &[], 2).unwrap()
        };
        assert_eq!(bytes, [0x04, 0xB0]);
    }

    #[test]
    fn the_higher_resolution_doubles_the_counts() {
        let mut chip = chip();
        assert_eq!(measurement(&mut chip, ONE_SHOT_HIGH_RES2), 720);
    }

    #[test]
    fn the_low_resolution_has_four_lux_per_count() {
        let mut chip = chip();
        assert_eq!(measurement(&mut chip, ONE_SHOT_LOW_RES), 45);
    }

    #[test]
    fn continuous_modes_latch_too() {
        let mut chip = chip();
        chip.on_input("set:lux=1000");
        assert_eq!(measurement(&mut chip, CONTINUOUS_HIGH_RES), 1200);
    }

    #[test]
    fn power_down_stops_measuring_until_the_next_command() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[POWER_DOWN], 0), Some(vec![]));
        assert!(!chip.powered);
        assert_eq!(transfer(&mut chip, &[POWER_ON], 0), Some(vec![]));
        assert!(chip.powered);
        assert_eq!(measurement(&mut chip, ONE_SHOT_HIGH_RES), 360);
    }

    #[test]
    fn a_reset_clears_the_measurement() {
        let mut chip = chip();
        let _ = measurement(&mut chip, ONE_SHOT_HIGH_RES);
        assert_eq!(transfer(&mut chip, &[RESET], 0), Some(vec![]));
        assert_eq!(chip.raw, 0);
        assert!(!chip.powered, "the reset returns to power down");
    }

    #[test]
    fn an_unknown_command_changes_nothing() {
        let mut chip = chip();
        let _ = measurement(&mut chip, ONE_SHOT_HIGH_RES);
        assert_eq!(transfer(&mut chip, &[0x55], 0), Some(vec![]));
        assert_eq!(chip.raw, 360);
        assert!(chip.powered);
    }

    #[test]
    fn the_inputs_set_and_restore_the_light() {
        let mut chip = chip();
        chip.on_input("set:lux=1000");
        assert_eq!(measurement(&mut chip, ONE_SHOT_HIGH_RES), 1200);

        chip.on_input("dark");
        assert_eq!(measurement(&mut chip, ONE_SHOT_HIGH_RES), 0);

        chip.on_input("rest");
        assert_eq!(measurement(&mut chip, ONE_SHOT_HIGH_RES), 360);
    }

    #[test]
    fn the_input_is_clamped_to_the_counts() {
        let mut chip = chip();
        chip.on_input("set:lux=1000000");
        assert_eq!(measurement(&mut chip, ONE_SHOT_HIGH_RES), 65_535);
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x23,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_light() {
        let mut chip = chip();
        chip.on_input("set:lux=42.5");
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("lux"), Some("42.5"));
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x5C, &[POWER_ON], 0)).unwrap();
        assert!(!reply.ack, "nothing answers at the ADDR-high address");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x23, &[POWER_ON], 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
