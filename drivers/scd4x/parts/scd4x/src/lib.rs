//! A Sensirion SCD4x CO2, temperature and humidity sensor, written as a
//! virtual part.
//!
//! The firmware talks to it the way it would to the real chip: the part is
//! command-driven and has no register pointer. The driver writes a two-byte
//! command and then either reads the answer or nothing:
//!
//! - `0x21B1` starts periodic measurement,
//! - `0x3F86` stops periodic measurement,
//! - `0xEC05` selects the last measurement, and the next read answers nine
//!   bytes: CO2, temperature and humidity, each 16-bit big-endian followed by
//!   its CRC-8,
//! - every other command is acknowledged and ignored.
//!
//! The CRC is the datasheet's: polynomial 0x31, initial value 0xFF, over the
//! two bytes of the word.
//!
//! The part keeps what the sensor sees in ppm, degrees Celsius and percent
//! relative humidity and encodes it with the datasheet's conversions:
//!
//! - CO2: the value itself,
//! - temperature: `(T + 45) / 175 * 65536`,
//! - humidity: `RH / 100 * 65536`.
//!
//! README.md lists what is left out.
//!
//! `Scd4x` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's only address.
const DEFAULT_ADDR: u32 = 0x62;

/// Start periodic measurement at the power-on five-second interval.
const START_PERIODIC_MEASUREMENT: [u8; 2] = [0x21, 0xB1];
/// Stop periodic measurement.
const STOP_PERIODIC_MEASUREMENT: [u8; 2] = [0x3F, 0x86];
/// Select the last measurement for the next read.
const READ_MEASUREMENT: [u8; 2] = [0xEC, 0x05];

/// What the sensor sees when nobody has changed it: fresh-ish air in a mild
/// room.
const DEFAULT_CO2: f64 = 400.0;
const DEFAULT_TEMPERATURE: f64 = 22.5;
const DEFAULT_HUMIDITY: f64 = 45.0;

/// The ranges the datasheet gives.
const MIN_CO2: f64 = 400.0;
const MAX_CO2: f64 = 40_000.0;
const MIN_TEMPERATURE: f64 = -40.0;
const MAX_TEMPERATURE: f64 = 85.0;
const MAX_HUMIDITY: f64 = 100.0;

/// The two ends of the temperature conversion.
const CONVERSION_MIN_TEMPERATURE: f64 = -45.0;
const CONVERSION_TEMPERATURE_SPAN: f64 = 175.0;
/// Both analog words span the whole 16-bit range.
const FULL_SCALE: f64 = 65_536.0;

struct Scd4x {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Whether periodic measurement is running.
    measuring: bool,
    /// What the sensor sees.
    co2: f64,
    temperature: f64,
    humidity: f64,
}

impl Default for Scd4x {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            measuring: false,
            co2: DEFAULT_CO2,
            temperature: DEFAULT_TEMPERATURE,
            humidity: DEFAULT_HUMIDITY,
        }
    }
}

impl Device for Scd4x {
    const NAME: &'static str = "scd4x";
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

    /// Change what the sensor sees: `set:co2=<ppm>`,
    /// `set:temperature=<°C>`, `set:humidity=<%RH>`, or `rest` for the
    /// defaults.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "co2" => self.co2 = value.clamp(MIN_CO2, MAX_CO2),
                "temperature" => {
                    self.temperature = value.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE)
                }
                "humidity" => self.humidity = value.clamp(0.0, MAX_HUMIDITY),
                _ => {}
            }
        } else if name == "rest" {
            self.co2 = DEFAULT_CO2;
            self.temperature = DEFAULT_TEMPERATURE;
            self.humidity = DEFAULT_HUMIDITY;
        }
    }

    /// The guest's I2C transaction. The part is command-driven: a write is a
    /// two-byte command, and a read answers the last measurement.
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
        if command == START_PERIODIC_MEASUREMENT {
            // The part model has no measurement interval: once started, a
            // measurement is ready at once.
            self.measuring = true;
        } else if command == STOP_PERIODIC_MEASUREMENT {
            self.measuring = false;
        } else if command == READ_MEASUREMENT {
            // The answer is the last measurement, which the read below
            // returns; the command itself changes nothing.
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
                .set("co2", self.co2)
                .set("temperature", self.temperature)
                .set("humidity", self.humidity)
                .set("measuring", u8::from(self.measuring)),
        )
    }
}

impl Scd4x {
    /// The nine measurement bytes: CO2, temperature and humidity, each
    /// 16-bit big-endian followed by its CRC.
    fn measurement(&self) -> [u8; 9] {
        // A part that was never started has no measurement; the bus floats,
        // which is all ones.
        if !self.measuring {
            return [0xFF; 9];
        }
        let co2 = self.co2.round().clamp(0.0, 65_535.0) as u16;
        let temperature = ((self.temperature - CONVERSION_MIN_TEMPERATURE)
            / CONVERSION_TEMPERATURE_SPAN
            * FULL_SCALE)
            .round()
            .clamp(0.0, FULL_SCALE - 1.0) as u16;
        let humidity = (self.humidity / 100.0 * FULL_SCALE)
            .round()
            .clamp(0.0, FULL_SCALE - 1.0) as u16;
        let mut bytes = [0u8; 9];
        for (index, word) in [co2, temperature, humidity].into_iter().enumerate() {
            let [high, low] = word.to_be_bytes();
            bytes[index * 3] = high;
            bytes[index * 3 + 1] = low;
            bytes[index * 3 + 2] = crc8([high, low]);
        }
        bytes
    }
}

/// The datasheet's CRC-8: polynomial 0x31, initial value 0xFF, over two bytes.
fn crc8(bytes: [u8; 2]) -> u8 {
    let mut crc = 0xFFu8;
    for byte in bytes {
        crc ^= byte;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 {
                (crc << 1) ^ 0x31
            } else {
                crc << 1
            };
        }
    }
    crc
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Scd4x, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the 0x62 address.
    fn chip() -> Scd4x {
        let mut chip = Scd4x::default();
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
    fn transfer(chip: &mut Scd4x, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// Start periodic measurement and take one reading, the way the driver
    /// does it.
    fn measurement(chip: &mut Scd4x) -> Vec<u8> {
        assert_eq!(
            transfer(chip, &START_PERIODIC_MEASUREMENT, 0),
            Some(vec![])
        );
        assert_eq!(transfer(chip, &READ_MEASUREMENT, 0), Some(vec![]));
        transfer(chip, &[], 9).expect("nine measurement bytes")
    }

    #[test]
    fn the_default_reading_is_400_ppm_22_5_degrees_and_45_percent() {
        let mut chip = chip();
        assert_eq!(
            measurement(&mut chip),
            [0x01, 0x90, 0x4C, 0x62, 0xBE, 0xAD, 0x73, 0x33, 0x01],
            "400 ppm, 25278 counts and 29491 counts, with their CRCs"
        );
    }

    #[test]
    fn an_unstarted_part_has_no_measurement() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[], 9),
            Some(vec![0xFF; 9]),
            "the bus floats until periodic measurement starts"
        );
    }

    #[test]
    fn stop_ends_the_measurement() {
        let mut chip = chip();
        let _ = measurement(&mut chip);
        assert!(chip.measuring);
        assert_eq!(
            transfer(&mut chip, &STOP_PERIODIC_MEASUREMENT, 0),
            Some(vec![])
        );
        assert!(!chip.measuring);
        assert_eq!(transfer(&mut chip, &[], 9), Some(vec![0xFF; 9]));
    }

    #[test]
    fn the_inputs_reencode_the_measurement() {
        let mut chip = chip();
        chip.on_input("set:co2=1000");
        chip.on_input("set:temperature=30");
        chip.on_input("set:humidity=60");
        assert_eq!(
            measurement(&mut chip),
            [0x03, 0xE8, 0xD4, 0x6D, 0xB7, 0xBC, 0x99, 0x9A, 0xED],
            "1000 ppm, 28087 counts and 39322 counts, with their CRCs"
        );
    }

    #[test]
    fn the_crc_is_the_datasheets() {
        assert_eq!(crc8([0x01, 0x90]), 0x4C);
        assert_eq!(crc8([0x62, 0xBE]), 0xAD);
        assert_eq!(crc8([0x73, 0x33]), 0x01);
    }

    #[test]
    fn the_inputs_stop_at_the_datasheet_ranges() {
        let mut chip = chip();
        chip.on_input("set:co2=100");
        assert_eq!(chip.co2, MIN_CO2);
        chip.on_input("set:co2=50000");
        assert_eq!(chip.co2, MAX_CO2);
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
        chip.on_input("set:co2=1000");
        chip.on_input("set:temperature=30");
        chip.on_input("set:humidity=60");
        chip.on_input("rest");
        assert_eq!(
            measurement(&mut chip),
            [0x01, 0x90, 0x4C, 0x62, 0xBE, 0xAD, 0x73, 0x33, 0x01]
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x62,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_room_and_the_measurement() {
        let mut chip = chip();
        chip.on_input("set:co2=1000");
        let shown = chip.state().unwrap();
        assert_eq!(
            (
                shown.get("co2"),
                shown.get("temperature"),
                shown.get("humidity"),
                shown.get("measuring")
            ),
            (Some("1000"), Some("22.5"), Some("45"), Some("0"))
        );
        let _ = measurement(&mut chip);
        assert_eq!(chip.state().unwrap().get("measuring"), Some("1"));
    }

    #[test]
    fn an_unknown_command_answers_nothing_new() {
        let mut chip = chip();
        let _ = measurement(&mut chip);
        assert_eq!(
            transfer(&mut chip, &[0x36, 0x82], 0),
            Some(vec![]),
            "a command this part does not model is acknowledged and ignored"
        );
        assert_eq!(
            transfer(&mut chip, &[], 9),
            Some(vec![0x01, 0x90, 0x4C, 0x62, 0xBE, 0xAD, 0x73, 0x33, 0x01]),
            "the last measurement still stands"
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip
            .on_bus(&request(0x63, &START_PERIODIC_MEASUREMENT, 0))
            .unwrap();
        assert!(!reply.ack, "nothing answers at 0x63");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x62, &START_PERIODIC_MEASUREMENT, 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
