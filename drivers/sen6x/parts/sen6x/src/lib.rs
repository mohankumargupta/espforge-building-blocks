//! A Sensirion SEN6x air-quality sensor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: the part is
//! command-driven and has no register pointer. A command is two bytes,
//! big-endian, and is not CRC-protected:
//!
//! - `0x0021` starts continuous measurement,
//! - `0x0104` stops continuous measurement,
//! - `0x0300` selects the SEN66 measured values, and the next read answers 27
//!   bytes: nine 16-bit big-endian words, each followed by its CRC-8,
//! - every other command is acknowledged and ignored.
//!
//! The nine words are PM1.0, PM2.5, PM4.0 and PM10.0 (tenths of µg/m³),
//! ambient temperature (two-hundredths of °C), ambient humidity (hundredths
//! of %), VOC index and NOx index (tenths), and CO2 (ppm).
//!
//! The CRC is the datasheet's: polynomial 0x31, initial value 0xFF, over the
//! two bytes of the word. Before measurement starts, every value reads at its
//! "unknown" upper limit (0xFFFF for the unsigned words, 0x7FFF for the
//! signed ones).
//!
//! README.md lists what is left out.
//!
//! `Sen6x` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's only address.
const DEFAULT_ADDR: u32 = 0x6B;

/// Start continuous measurement in the SEN66 layout.
const START_CONTINUOUS_MEASUREMENT: [u8; 2] = [0x00, 0x21];
/// Stop continuous measurement.
const STOP_MEASUREMENT: [u8; 2] = [0x01, 0x04];
/// Select the SEN66 measured values for the next read.
const READ_MEASURED_VALUES: [u8; 2] = [0x03, 0x00];

/// Nine words of two bytes and a CRC each.
const MEASUREMENT_LEN: usize = 27;

/// What the sensor sees when nobody has changed it: indoor air in a mild
/// room.
const DEFAULT_PM1: f64 = 8.0;
const DEFAULT_PM2_5: f64 = 12.5;
const DEFAULT_PM4: f64 = 15.0;
const DEFAULT_PM10: f64 = 20.0;
const DEFAULT_TEMPERATURE: f64 = 22.5;
const DEFAULT_HUMIDITY: f64 = 45.0;
const DEFAULT_VOC: f64 = 100.0;
const DEFAULT_NOX: f64 = 1.0;
const DEFAULT_CO2: f64 = 400.0;

/// The ranges the datasheet gives.
const MAX_PM: f64 = 1_000.0;
const MIN_TEMPERATURE: f64 = -10.0;
const MAX_TEMPERATURE: f64 = 60.0;
const MAX_HUMIDITY: f64 = 100.0;
const MIN_INDEX: f64 = 1.0;
const MAX_INDEX: f64 = 500.0;
const MAX_CO2: f64 = 40_000.0;

/// The value the datasheet reports when a measurement is unknown.
const UNKNOWN_UNSIGNED: u16 = 0xFFFF;
const UNKNOWN_SIGNED: u16 = 0x7FFF;

struct Sen6x {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Whether continuous measurement is running.
    measuring: bool,
    /// What the sensor sees.
    pm1: f64,
    pm2_5: f64,
    pm4: f64,
    pm10: f64,
    temperature: f64,
    humidity: f64,
    voc: f64,
    nox: f64,
    co2: f64,
}

impl Default for Sen6x {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            measuring: false,
            pm1: DEFAULT_PM1,
            pm2_5: DEFAULT_PM2_5,
            pm4: DEFAULT_PM4,
            pm10: DEFAULT_PM10,
            temperature: DEFAULT_TEMPERATURE,
            humidity: DEFAULT_HUMIDITY,
            voc: DEFAULT_VOC,
            nox: DEFAULT_NOX,
            co2: DEFAULT_CO2,
        }
    }
}

impl Device for Sen6x {
    const NAME: &'static str = "sen6x";
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

    /// Change what the sensor sees: `set:pm1=<µg/m³>`, `set:pm2-5=<µg/m³>`,
    /// `set:pm4=<µg/m³>`, `set:pm10=<µg/m³>`, `set:temperature=<°C>`,
    /// `set:humidity=<%RH>`, `set:voc=<index>`, `set:nox=<index>`,
    /// `set:co2=<ppm>`, or `rest` for the defaults.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "pm1" => self.pm1 = value.clamp(0.0, MAX_PM),
                "pm2-5" => self.pm2_5 = value.clamp(0.0, MAX_PM),
                "pm4" => self.pm4 = value.clamp(0.0, MAX_PM),
                "pm10" => self.pm10 = value.clamp(0.0, MAX_PM),
                "temperature" => {
                    self.temperature = value.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE)
                }
                "humidity" => self.humidity = value.clamp(0.0, MAX_HUMIDITY),
                "voc" => self.voc = value.clamp(MIN_INDEX, MAX_INDEX),
                "nox" => self.nox = value.clamp(MIN_INDEX, MAX_INDEX),
                "co2" => self.co2 = value.clamp(0.0, MAX_CO2),
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
    /// two-byte command, and a read answers the last measured values.
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
        if command == START_CONTINUOUS_MEASUREMENT {
            // The part model has no measurement interval: once started, a
            // measurement is ready at once.
            self.measuring = true;
        } else if command == STOP_MEASUREMENT {
            self.measuring = false;
        } else if command == READ_MEASURED_VALUES {
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
                .set("pm2-5", self.pm2_5)
                .set("co2", self.co2)
                .set("temperature", self.temperature)
                .set("humidity", self.humidity)
                .set("voc", self.voc)
                .set("nox", self.nox)
                .set("measuring", u8::from(self.measuring)),
        )
    }
}

impl Sen6x {
    /// The 27 measured bytes: nine words, each with its CRC.
    fn measurement(&self) -> [u8; MEASUREMENT_LEN] {
        let mut words = [UNKNOWN_UNSIGNED; 9];
        if self.measuring {
            words[0] = scaled(self.pm1, 10.0, u16::MAX - 1);
            words[1] = scaled(self.pm2_5, 10.0, u16::MAX - 1);
            words[2] = scaled(self.pm4, 10.0, u16::MAX - 1);
            words[3] = scaled(self.pm10, 10.0, u16::MAX - 1);
            words[4] = (self.temperature * 200.0)
                .round()
                .clamp(-32_768.0, 32_767.0) as i32 as u16;
            words[5] = (self.humidity * 100.0).round().clamp(0.0, 32_767.0) as u16;
            words[6] = scaled(self.voc, 10.0, i16::MAX as u16);
            words[7] = scaled(self.nox, 10.0, i16::MAX as u16);
            words[8] = self.co2.round().clamp(0.0, u16::MAX as f64) as u16;
        } else {
            for word in &mut words[4..8] {
                *word = UNKNOWN_SIGNED;
            }
        }
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

/// `value * factor`, rounded and held to `max`.
fn scaled(value: f64, factor: f64, max: u16) -> u16 {
    (value * factor).round().clamp(0.0, f64::from(max)) as u16
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
esp_part::export_part!(Sen6x, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the 0x6B address.
    fn chip() -> Sen6x {
        let mut chip = Sen6x::default();
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
    fn transfer(chip: &mut Sen6x, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// Start continuous measurement and take one reading, the way the driver
    /// does it.
    fn measurement(chip: &mut Sen6x) -> Vec<u8> {
        assert_eq!(
            transfer(chip, &START_CONTINUOUS_MEASUREMENT, 0),
            Some(vec![])
        );
        assert_eq!(transfer(chip, &READ_MEASURED_VALUES, 0), Some(vec![]));
        transfer(chip, &[], 27).expect("27 measured bytes")
    }

    #[test]
    fn the_default_reading_is_indoor_air() {
        let mut chip = chip();
        assert_eq!(
            measurement(&mut chip),
            [
                0x00, 0x50, 0xFF, 0x00, 0x7D, 0x35, 0x00, 0x96, 0x1E, 0x00, 0xC8, 0x7F, 0x11,
                0x94, 0xE6, 0x11, 0x94, 0xE6, 0x03, 0xE8, 0xD4, 0x00, 0x0A, 0x5A, 0x01, 0x90,
                0x4C,
            ],
            "8.0, 12.5, 15.0 and 20.0 ug/m3, 22.5 C, 45 %RH, 100 and 1 index, 400 ppm"
        );
    }

    #[test]
    fn before_start_every_value_is_unknown() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[], 27),
            Some(vec![
                0xFF, 0xFF, 0xAC, 0xFF, 0xFF, 0xAC, 0xFF, 0xFF, 0xAC, 0xFF, 0xFF, 0xAC, 0x7F,
                0xFF, 0x8F, 0x7F, 0xFF, 0x8F, 0x7F, 0xFF, 0x8F, 0x7F, 0xFF, 0x8F, 0xFF, 0xFF,
                0xAC,
            ]),
            "0xFFFF for the unsigned words and 0x7FFF for the signed ones"
        );
    }

    #[test]
    fn stop_ends_the_measurement() {
        let mut chip = chip();
        let _ = measurement(&mut chip);
        assert!(chip.measuring);
        assert_eq!(transfer(&mut chip, &STOP_MEASUREMENT, 0), Some(vec![]));
        assert!(!chip.measuring);
        assert_eq!(
            transfer(&mut chip, &[], 27).map(|bytes| bytes[0..3].to_vec()),
            Some(vec![0xFF, 0xFF, 0xAC])
        );
    }

    #[test]
    fn the_inputs_reencode_the_measurement() {
        let mut chip = chip();
        chip.on_input("set:pm2-5=50");
        chip.on_input("set:temperature=30");
        chip.on_input("set:humidity=60");
        chip.on_input("set:voc=20");
        chip.on_input("set:nox=3");
        chip.on_input("set:co2=1000");
        let frame = measurement(&mut chip);
        assert_eq!(&frame[0..3], &[0x00, 0x50, 0xFF], "PM1 is unchanged");
        assert_eq!(&frame[3..6], &[0x01, 0xF4, 0x33], "50.0 ug/m3");
        assert_eq!(&frame[12..15], &[0x17, 0x70, 0xB9], "30.00 C");
        assert_eq!(&frame[15..18], &[0x17, 0x70, 0xB9], "60.00 %RH");
        assert_eq!(&frame[18..21], &[0x00, 0xC8, 0x7F], "VOC 20.0");
        assert_eq!(&frame[21..24], &[0x00, 0x1E, 0xDD], "NOx 3.0");
        assert_eq!(&frame[24..27], &[0x03, 0xE8, 0xD4], "1000 ppm");
    }

    #[test]
    fn the_crc_matches_the_datasheet_example() {
        assert_eq!(crc8([0xBE, 0xEF]), 0x92);
    }

    #[test]
    fn the_inputs_stop_at_the_datasheet_ranges() {
        let mut chip = chip();
        chip.on_input("set:pm2-5=5000");
        assert_eq!(chip.pm2_5, MAX_PM);
        chip.on_input("set:temperature=-100");
        assert_eq!(chip.temperature, MIN_TEMPERATURE);
        chip.on_input("set:temperature=200");
        assert_eq!(chip.temperature, MAX_TEMPERATURE);
        chip.on_input("set:humidity=200");
        assert_eq!(chip.humidity, MAX_HUMIDITY);
        chip.on_input("set:voc=0");
        assert_eq!(chip.voc, MIN_INDEX);
        chip.on_input("set:nox=1000");
        assert_eq!(chip.nox, MAX_INDEX);
        chip.on_input("set:co2=50000");
        assert_eq!(chip.co2, MAX_CO2);
    }

    #[test]
    fn rest_puts_the_defaults_back() {
        let mut chip = chip();
        chip.on_input("set:pm2-5=50");
        chip.on_input("set:co2=1000");
        chip.on_input("set:temperature=30");
        chip.on_input("rest");
        assert_eq!(
            measurement(&mut chip),
            [
                0x00, 0x50, 0xFF, 0x00, 0x7D, 0x35, 0x00, 0x96, 0x1E, 0x00, 0xC8, 0x7F, 0x11,
                0x94, 0xE6, 0x11, 0x94, 0xE6, 0x03, 0xE8, 0xD4, 0x00, 0x0A, 0x5A, 0x01, 0x90,
                0x4C,
            ]
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x6B,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_air_and_the_measurement() {
        let mut chip = chip();
        chip.on_input("set:pm2-5=50");
        chip.on_input("set:co2=1000");
        let shown = chip.state().unwrap();
        assert_eq!(
            (
                shown.get("pm2-5"),
                shown.get("co2"),
                shown.get("measuring")
            ),
            (Some("50"), Some("1000"), Some("0"))
        );
        let _ = measurement(&mut chip);
        assert_eq!(chip.state().unwrap().get("measuring"), Some("1"));
    }

    #[test]
    fn an_unknown_command_answers_nothing_new() {
        let mut chip = chip();
        let _ = measurement(&mut chip);
        assert_eq!(
            transfer(&mut chip, &[0xD0, 0x14], 0),
            Some(vec![]),
            "a command this part does not model is acknowledged and ignored"
        );
        assert_eq!(
            transfer(&mut chip, &[], 27).map(|bytes| bytes[24..27].to_vec()),
            Some(vec![0x01, 0x90, 0x4C]),
            "the last measurement still stands"
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip
            .on_bus(&request(0x6A, &START_CONTINUOUS_MEASUREMENT, 0))
            .unwrap();
        assert!(!reply.ack, "nothing answers at 0x6A");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x6B, &START_CONTINUOUS_MEASUREMENT, 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
