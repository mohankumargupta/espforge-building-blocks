//! A TI HDC302x temperature and humidity sensor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: the part is
//! command-driven and has no register pointer. The driver writes a two-byte
//! command and then either reads the answer or nothing:
//!
//! - `0x2400` triggers one on-demand measurement, and the next read answers
//!   four bytes: temperature and humidity, each 16-bit big-endian,
//! - `0x30A2` is the soft reset.
//!
//! The part keeps what the sensor sees in degrees Celsius and percent relative
//! humidity and encodes it with the datasheet's conversions:
//!
//! - temperature: `(T + 45) / 175 * 65535`,
//! - humidity: `RH / 100 * 65535`.
//!
//! README.md lists what is left out.
//!
//! `Hdc302x` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with both address pins low; 0x45 to 0x47 are the other
/// straps.
const DEFAULT_ADDR: u32 = 0x44;

/// Trigger one on-demand measurement, low-power mode, no CRC.
const TRIGGER_ON_DEMAND: [u8; 2] = [0x24, 0x00];
/// Soft reset.
const SOFT_RESET: [u8; 2] = [0x30, 0xA2];

/// What the sensor sees when nobody has changed it: a mild room.
const DEFAULT_TEMPERATURE: f64 = 22.5;
const DEFAULT_HUMIDITY: f64 = 45.0;

/// The ranges the datasheet gives.
const MIN_TEMPERATURE: f64 = -40.0;
const MAX_TEMPERATURE: f64 = 125.0;
const MAX_HUMIDITY: f64 = 100.0;

/// The two ends of the temperature conversion.
const CONVERSION_MIN_TEMPERATURE: f64 = -45.0;
const CONVERSION_TEMPERATURE_SPAN: f64 = 175.0;
/// Both pairs span the whole 16-bit range.
const FULL_SCALE: f64 = 65_535.0;

struct Hdc302x {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Whether a measurement has been triggered since the last reset.
    triggered: bool,
    /// What the sensor sees.
    temperature: f64,
    humidity: f64,
}

impl Default for Hdc302x {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            triggered: false,
            temperature: DEFAULT_TEMPERATURE,
            humidity: DEFAULT_HUMIDITY,
        }
    }
}

impl Device for Hdc302x {
    const NAME: &'static str = "hdc302x";
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

    /// Change what the sensor sees: `set:temperature=<°C>`,
    /// `set:humidity=<%RH>`, or `rest` for the defaults.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "temperature" => {
                    self.temperature = value.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE)
                }
                "humidity" => self.humidity = value.clamp(0.0, MAX_HUMIDITY),
                _ => {}
            }
        } else if name == "rest" {
            self.temperature = DEFAULT_TEMPERATURE;
            self.humidity = DEFAULT_HUMIDITY;
        }
    }

    /// The guest's I2C transaction. The part is command-driven: a write is a
    /// two-byte command, and a read answers the current measurement.
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
        let command = i2c.write.as_slice();
        if command == TRIGGER_ON_DEMAND {
            // The part model has no conversion time: the measurement is
            // ready at once.
            self.triggered = true;
        } else if command == SOFT_RESET {
            self.reset();
        }
        if i2c.read_len > 0 {
            let bytes = self.measurement();
            reply.data = (0..i2c.read_len as usize)
                .map(|offset| bytes.get(offset).copied().unwrap_or(0xFF))
                .collect();
        }
        // A probe, a command write, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("temperature", self.temperature)
                .set("humidity", self.humidity)
                .set("triggered", u8::from(self.triggered)),
        )
    }
}

impl Hdc302x {
    /// A soft reset: the heater, the alerts and the pending measurement go
    /// back to their power-on values. What the sensor sees stays.
    fn reset(&mut self) {
        self.triggered = false;
    }

    /// The six measurement bytes: temperature, then humidity, big-endian,
    /// each followed by a CRC byte.
    fn measurement(&self) -> [u8; 6] {
        // A fresh part that was never triggered has no answer yet; the bus
        // floats, which is all ones.
        if !self.triggered {
            return [0xFF; 6];
        }
        let temperature = ((self.temperature - CONVERSION_MIN_TEMPERATURE)
            / CONVERSION_TEMPERATURE_SPAN
            * FULL_SCALE)
            .round()
            .clamp(0.0, FULL_SCALE) as u16;
        let humidity = (self.humidity / 100.0 * FULL_SCALE)
            .round()
            .clamp(0.0, FULL_SCALE) as u16;
        // The datasheet sends each 16-bit value with a CRC byte after it. The
        // crate reads with that three-byte stride; the fixture manifest turns
        // the crate's `crc` feature off, so a placeholder is enough.
        [
            (temperature >> 8) as u8,
            temperature as u8,
            0x00,
            (humidity >> 8) as u8,
            humidity as u8,
            0x00,
        ]
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Hdc302x, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the 0x44 strap.
    fn chip() -> Hdc302x {
        let mut chip = Hdc302x::default();
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

    /// One transaction to the sensor's address: the bytes read back, or `None`
    /// for a NACK.
    fn transfer(chip: &mut Hdc302x, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// One on-demand measurement, the way the driver takes it.
    fn measurement(chip: &mut Hdc302x) -> Vec<u8> {
        assert_eq!(transfer(chip, &TRIGGER_ON_DEMAND, 0), Some(vec![]));
        transfer(chip, &[], 6).expect("two three-byte measurement words")
    }

    #[test]
    fn the_default_reading_is_22_5_degrees_and_45_percent() {
        let mut chip = chip();
        assert_eq!(
            measurement(&mut chip),
            [0x62, 0xBE, 0x00, 0x73, 0x33, 0x00],
            "25278 and 29491 counts"
        );
    }

    #[test]
    fn an_untriggered_part_has_no_answer() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[], 6),
            Some(vec![0xFF; 6]),
            "the bus floats until a measurement is triggered"
        );
    }

    #[test]
    fn the_inputs_reencode_the_measurement() {
        let mut chip = chip();
        chip.on_input("set:temperature=30");
        chip.on_input("set:humidity=60");
        assert_eq!(
            measurement(&mut chip),
            [0x6D, 0xB6, 0x00, 0x99, 0x99, 0x00],
            "28086 and 39321 counts"
        );
    }

    #[test]
    fn the_ends_of_the_conversion_are_exact() {
        let mut chip = chip();
        chip.on_input("set:temperature=-45");
        chip.on_input("set:humidity=0");
        // -45 is below the part's -40 range, so the floor is -40: (40 - 45)
        // below zero, i.e. 5 of the 175 degrees, which is 1872 counts.
        assert_eq!(measurement(&mut chip), [0x07, 0x50, 0x00, 0x00, 0x00, 0x00]);
        // 125 °C is short of the conversion's 130 °C full scale: (125 + 45)
        // of the 175 degrees, which is 63663 counts.
        chip.on_input("set:temperature=125");
        chip.on_input("set:humidity=100");
        assert_eq!(measurement(&mut chip), [0xF8, 0xAF, 0x00, 0xFF, 0xFF, 0x00]);
    }

    #[test]
    fn a_soft_reset_drops_the_pending_measurement() {
        let mut chip = chip();
        let _ = measurement(&mut chip);
        assert!(chip.triggered);
        assert_eq!(transfer(&mut chip, &SOFT_RESET, 0), Some(vec![]));
        assert!(!chip.triggered);
        assert_eq!(transfer(&mut chip, &[], 6), Some(vec![0xFF; 6]));
        assert_eq!(
            (chip.temperature, chip.humidity),
            (DEFAULT_TEMPERATURE, DEFAULT_HUMIDITY),
            "what the sensor sees survives a reset"
        );
    }

    #[test]
    fn the_inputs_stop_at_the_datasheet_ranges() {
        let mut chip = chip();
        chip.on_input("set:temperature=-100");
        assert_eq!(chip.temperature, MIN_TEMPERATURE);
        chip.on_input("set:temperature=200");
        assert_eq!(chip.temperature, MAX_TEMPERATURE);
        chip.on_input("set:humidity=200");
        assert_eq!(chip.humidity, MAX_HUMIDITY);
    }

    #[test]
    fn rest_puts_the_defaults_back() {
        let mut chip = chip();
        chip.on_input("set:temperature=30");
        chip.on_input("set:humidity=60");
        chip.on_input("rest");
        assert_eq!(
            measurement(&mut chip),
            [0x62, 0xBE, 0x00, 0x73, 0x33, 0x00]
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x44,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_room_and_the_trigger() {
        let mut chip = chip();
        chip.on_input("set:temperature=30");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("temperature"), shown.get("humidity"), shown.get("triggered")),
            (Some("30"), Some("45"), Some("0"))
        );
        let _ = measurement(&mut chip);
        assert_eq!(chip.state().unwrap().get("triggered"), Some("1"));
    }

    #[test]
    fn an_unknown_command_answers_nothing_new() {
        let mut chip = chip();
        let _ = measurement(&mut chip);
        assert_eq!(
            transfer(&mut chip, &[0x37, 0x81], 0),
            Some(vec![]),
            "a command this part does not model is acknowledged and ignored"
        );
        assert_eq!(
            transfer(&mut chip, &[], 6),
            Some(vec![0x62, 0xBE, 0x00, 0x73, 0x33, 0x00]),
            "the last measurement still stands"
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x45, &TRIGGER_ON_DEMAND, 0)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x45");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x44, &TRIGGER_ON_DEMAND, 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
