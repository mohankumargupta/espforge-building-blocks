//! A Sensirion SGP30 air-quality sensor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: the part is
//! command-driven and has no register pointer. A command is a two-byte
//! big-endian word followed by its CRC-8:
//!
//! - `0x2003` (and its CRC) initialises air quality measurement,
//! - `0x2008` (and its CRC) selects the IAQ measurement, and the next read
//!   answers six bytes: eCO2 and TVOC, each 16-bit big-endian followed by its
//!   CRC-8,
//! - every other command is acknowledged and ignored, a command whose CRC does
//!   not match included.
//!
//! The two words are eCO2 in parts per million and TVOC in parts per billion.
//! The part holds them as they are; nothing is scaled.
//!
//! The CRC is the datasheet's: polynomial 0x31, initial value 0xFF, over the
//! two bytes of the word.
//!
//! README.md lists what is left out.
//!
//! `Sgp30` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's only address.
const DEFAULT_ADDR: u32 = 0x58;

/// Initialise air quality measurement.
const IAQ_INIT: [u8; 3] = [0x20, 0x03, 0x0E];
/// Select the IAQ measurement for the next read.
const MEASURE_IAQ: [u8; 3] = [0x20, 0x08, 0xE4];

/// Two words of two bytes and a CRC each.
const MEASUREMENT_LEN: usize = 6;

/// What the sensor sees when nobody has changed it: fresh-ish air in a mild
/// room.
const DEFAULT_ECO2: f64 = 400.0;
const DEFAULT_TVOC: f64 = 0.0;

/// The ranges the datasheet gives.
const MIN_ECO2: f64 = 400.0;
const MAX_ECO2: f64 = 60_000.0;
const MAX_TVOC: f64 = 60_000.0;

struct Sgp30 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Whether air quality measurement has been initialised.
    measuring: bool,
    /// What the sensor sees.
    eco2: f64,
    tvoc: f64,
}

impl Default for Sgp30 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            measuring: false,
            eco2: DEFAULT_ECO2,
            tvoc: DEFAULT_TVOC,
        }
    }
}

impl Device for Sgp30 {
    const NAME: &'static str = "sgp30";
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

    /// Change what the sensor sees: `set:eco2=<ppm>`, `set:tvoc=<ppb>`, or
    /// `rest` for the defaults.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "eco2" => self.eco2 = value.clamp(MIN_ECO2, MAX_ECO2),
                "tvoc" => self.tvoc = value.clamp(0.0, MAX_TVOC),
                _ => {}
            }
        } else if name == "rest" {
            *self = Self {
                scl: self.scl,
                sda: self.sda,
                addr: self.addr,
                ..Self::default()
            };
        }
    }

    /// The guest's I2C transaction. The part is command-driven: a write is a
    /// command word and its CRC, and a read answers the last measurement.
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
        if command == IAQ_INIT {
            // The part model has no measurement interval: once initialised,
            // a measurement is ready at once, initial 15 s included.
            self.measuring = true;
        } else if command == MEASURE_IAQ {
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
                .set("eco2", self.eco2)
                .set("tvoc", self.tvoc)
                .set("measuring", u8::from(self.measuring)),
        )
    }
}

impl Sgp30 {
    /// The six measured bytes: eCO2 and TVOC, each with its CRC.
    fn measurement(&self) -> [u8; MEASUREMENT_LEN] {
        let words = [
            self.eco2.round().clamp(0.0, u16::MAX as f64) as u16,
            self.tvoc.round().clamp(0.0, u16::MAX as f64) as u16,
        ];
        let mut bytes = [0u8; MEASUREMENT_LEN];
        for (index, word) in words.into_iter().enumerate() {
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
esp_part::export_part!(Sgp30, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the 0x58 address.
    fn chip() -> Sgp30 {
        let mut chip = Sgp30::default();
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
    fn transfer(chip: &mut Sgp30, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// Initialise measurement and take one reading, the way the driver does
    /// it.
    fn measurement(chip: &mut Sgp30) -> Vec<u8> {
        assert_eq!(transfer(chip, &IAQ_INIT, 0), Some(vec![]));
        assert_eq!(transfer(chip, &MEASURE_IAQ, 0), Some(vec![]));
        transfer(chip, &[], 6).expect("six measured bytes")
    }

    #[test]
    fn the_default_reading_is_fresh_air() {
        let mut chip = chip();
        assert_eq!(
            measurement(&mut chip),
            [0x01, 0x90, 0x4C, 0x00, 0x00, 0x81],
            "400 ppm of eCO2 and 0 ppb of TVOC"
        );
    }

    #[test]
    fn the_commands_are_crc_protected() {
        assert_eq!(IAQ_INIT, [0x20, 0x03, crc8([0x20, 0x03])]);
        assert_eq!(MEASURE_IAQ, [0x20, 0x08, crc8([0x20, 0x08])]);
    }

    #[test]
    fn the_inputs_reencode_the_measurement() {
        let mut chip = chip();
        chip.on_input("set:eco2=1000");
        chip.on_input("set:tvoc=137");
        assert_eq!(
            measurement(&mut chip),
            [0x03, 0xE8, 0xD4, 0x00, 0x89, 0x73],
            "1000 ppm and 137 ppb"
        );
    }

    #[test]
    fn the_crc_matches_the_datasheet_example() {
        assert_eq!(crc8([0xBE, 0xEF]), 0x92);
    }

    #[test]
    fn the_inputs_stop_at_the_datasheet_ranges() {
        let mut chip = chip();
        chip.on_input("set:eco2=100");
        assert_eq!(chip.eco2, MIN_ECO2);
        chip.on_input("set:eco2=70000");
        assert_eq!(chip.eco2, MAX_ECO2);
        chip.on_input("set:tvoc=-10");
        assert_eq!(chip.tvoc, 0.0);
        chip.on_input("set:tvoc=70000");
        assert_eq!(chip.tvoc, MAX_TVOC);
    }

    #[test]
    fn rest_puts_the_defaults_back() {
        let mut chip = chip();
        chip.on_input("set:eco2=1000");
        chip.on_input("set:tvoc=137");
        chip.on_input("rest");
        assert_eq!(
            measurement(&mut chip),
            [0x01, 0x90, 0x4C, 0x00, 0x00, 0x81]
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x58,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_air_and_the_measurement() {
        let mut chip = chip();
        chip.on_input("set:eco2=1000");
        chip.on_input("set:tvoc=137");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("eco2"), shown.get("tvoc"), shown.get("measuring")),
            (Some("1000"), Some("137"), Some("0"))
        );
        let _ = measurement(&mut chip);
        assert_eq!(chip.state().unwrap().get("measuring"), Some("1"));
    }

    #[test]
    fn an_unknown_command_answers_nothing_new() {
        let mut chip = chip();
        let _ = measurement(&mut chip);
        assert_eq!(
            transfer(&mut chip, &[0x20, 0x2F, 0x00], 0),
            Some(vec![]),
            "a command this part does not model is acknowledged and ignored"
        );
        assert_eq!(
            transfer(&mut chip, &[], 6).map(|bytes| bytes[3..6].to_vec()),
            Some(vec![0x00, 0x00, 0x81]),
            "the last measurement still stands"
        );
    }

    #[test]
    fn a_command_with_a_bad_crc_is_ignored() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[0x20, 0x03, 0x00], 0),
            Some(vec![]),
            "the word is answered but the bad CRC keeps it from counting"
        );
        assert!(!chip.measuring);
        assert_eq!(
            transfer(&mut chip, &MEASURE_IAQ, 0),
            Some(vec![]),
            "this one carries its CRC, so it is a command"
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x59, &IAQ_INIT, 0)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x59");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x58, &IAQ_INIT, 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
