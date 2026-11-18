//! An ASAIR AHT20 temperature and relative humidity sensor, written as a
//! virtual part.
//!
//! The chip has no register pointer: the firmware writes a command and reads
//! whatever that command prepared. `0x71` prepares the status byte, `0xBE
//! 0x08 0x00` initializes (calibrates) the part, and `0xAC 0x33 0x00` triggers
//! a measurement whose seven bytes are the status, 20 bits of humidity,
//! 20 bits of temperature and the CRC-8. The part keeps a temperature and a
//! humidity, and encodes them the way the datasheet's formulas invert.
//! README.md lists what is left out.
//!
//! `Aht20` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The AHT20's only strapped address.
const DEFAULT_ADDR: u32 = 0x38;

/// Status read and the two commands, with the datasheet's parameter bytes.
const STATUS_COMMAND: u8 = 0x71;
const INITIALIZE_COMMAND: u8 = 0xBE;
const MEASURE_COMMAND: u8 = 0xAC;

/// Status bit 7: a measurement is still running (the part never is).
const BUSY: u8 = 0x80;
/// Status bit 3: calibrated. Measured and idle is `0x18`, uncalibrated
/// `0x10`.
const CALIBRATED: u8 = 0x08;
const READY_FRAME: u8 = 0x10;
const CALIBRATED_FRAME: u8 = READY_FRAME | CALIBRATED;

/// The status byte, 20 bits of humidity, 20 bits of temperature, CRC.
const MEASUREMENT_LEN: usize = 7;
/// A 20-bit count spans 2^20.
const COUNTS: f64 = 1_048_576.0;

/// What the part reports when nobody has changed it: a mild room.
const DEFAULT_TEMPERATURE_C: f64 = 22.5;
const DEFAULT_HUMIDITY_PERCENT: f64 = 45.0;
/// The chip's operating range.
const MIN_TEMPERATURE_C: f64 = -40.0;
const MAX_TEMPERATURE_C: f64 = 85.0;
const MIN_HUMIDITY_PERCENT: f64 = 0.0;
const MAX_HUMIDITY_PERCENT: f64 = 100.0;

struct Aht20 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The conditions it measures, in °C and %RH.
    temperature_c: f64,
    humidity_percent: f64,
    /// The calibration bit the status byte reports.
    calibrated: bool,
    /// The answer the last command prepared, which the next read clocks out.
    pending: Vec<u8>,
}

impl Default for Aht20 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            temperature_c: DEFAULT_TEMPERATURE_C,
            humidity_percent: DEFAULT_HUMIDITY_PERCENT,
            // A real AHT20 is factory-calibrated, so only an uncalibrated one
            // needs the initialization command.
            calibrated: true,
            pending: Vec::new(),
        }
    }
}

impl Device for Aht20 {
    const NAME: &'static str = "aht20";
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

    /// Change what the part measures. `set:temperature=<C>` and
    /// `set:humidity=<%>` set one at a time; `rest` puts the defaults back.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "temperature" => self.temperature_c = value,
                "humidity" => self.humidity_percent = value,
                _ => {}
            }
        } else if name == "rest" {
            self.temperature_c = DEFAULT_TEMPERATURE_C;
            self.humidity_percent = DEFAULT_HUMIDITY_PERCENT;
        }
        self.temperature_c = self
            .temperature_c
            .clamp(MIN_TEMPERATURE_C, MAX_TEMPERATURE_C);
        self.humidity_percent = self
            .humidity_percent
            .clamp(MIN_HUMIDITY_PERCENT, MAX_HUMIDITY_PERCENT);
    }

    /// The guest's I2C transaction: a write runs the command (leaving the
    /// answer the next read takes), then a read, if there is one, drains it.
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
            match command {
                STATUS_COMMAND => self.pending = vec![self.status()],
                INITIALIZE_COMMAND => {
                    // The parameters' values are fixed by the datasheet.
                    self.calibrated = true;
                    self.pending.clear();
                }
                MEASURE_COMMAND => self.pending = self.measurement(),
                _ => self.pending.clear(),
            }
        }
        if i2c.read_len > 0 {
            // A read takes what the last command prepared and advances past
            // it. Past the answer nothing drives the line, and the master
            // reads 0xFF.
            let n = (i2c.read_len as usize).min(self.pending.len());
            let mut data: Vec<u8> = self.pending.drain(..n).collect();
            data.resize(i2c.read_len as usize, 0xFF);
            reply.data = data;
        }
        // A probe, a write on its own, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("temperature", self.temperature_c)
                .set("humidity", self.humidity_percent),
        )
    }
}

impl Aht20 {
    /// The status byte: bit 3 calibrated, bit 7 clear because a measurement
    /// completes as soon as it starts.
    fn status(&self) -> u8 {
        let _ = BUSY; // The part is never busy between transactions.
        if self.calibrated {
            CALIBRATED_FRAME
        } else {
            READY_FRAME
        }
    }

    /// The seven bytes a measurement answers: status, the two 20-bit counts,
    /// and the CRC-8 over the first six bytes.
    fn measurement(&self) -> Vec<u8> {
        let humidity = humidity_counts(self.humidity_percent);
        let temperature = temperature_counts(self.temperature_c);
        let mut frame = vec![
            self.status(),
            (humidity >> 12) as u8,
            (humidity >> 4) as u8,
            (((humidity & 0x0F) as u8) << 4) | ((temperature >> 16) as u8),
            (temperature >> 8) as u8,
            temperature as u8,
        ];
        frame.push(crc8(&frame));
        debug_assert_eq!(frame.len(), MEASUREMENT_LEN);
        frame
    }
}

/// One humidity reading in the chip's 20-bit count.
fn humidity_counts(humidity_percent: f64) -> u32 {
    let counts = (humidity_percent.clamp(MIN_HUMIDITY_PERCENT, MAX_HUMIDITY_PERCENT)
        / MAX_HUMIDITY_PERCENT
        * COUNTS)
        .round();
    counts.clamp(0.0, COUNTS - 1.0) as u32
}

/// One temperature reading in the chip's 20-bit count: 0 counts is -50 °C.
fn temperature_counts(temperature_c: f64) -> u32 {
    let counts = ((temperature_c.clamp(MIN_TEMPERATURE_C, MAX_TEMPERATURE_C) + 50.0) / 200.0
        * COUNTS)
        .round();
    counts.clamp(0.0, COUNTS - 1.0) as u32
}

/// The datasheet's CRC-8: polynomial 0x31, initial value 0xFF.
fn crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0xFFu8;
    for &byte in bytes {
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
esp_part::export_part!(Aht20, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the part's fixed address.
    fn chip() -> Aht20 {
        let mut chip = Aht20::default();
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
    fn transfer(chip: &mut Aht20, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// The status byte, the way the driver reads it: command 0x71, then one
    /// byte.
    fn status(chip: &mut Aht20) -> u8 {
        transfer(chip, &[STATUS_COMMAND], 1).expect("a status read")[0]
    }

    /// One measurement, the way the driver takes it: the trigger write, then
    /// seven bytes with no command.
    fn measurement(chip: &mut Aht20) -> Vec<u8> {
        assert_eq!(
            transfer(chip, &[MEASURE_COMMAND, 0x33, 0x00], 0),
            Some(vec![]),
            "a trigger is acknowledged"
        );
        transfer(chip, &[], MEASUREMENT_LEN as u32).expect("seven bytes")
    }

    /// The humidity count in a measurement frame.
    fn humidity(frame: &[u8]) -> u32 {
        ((frame[1] as u32) << 12) | ((frame[2] as u32) << 4) | ((frame[3] as u32) >> 4)
    }

    /// The temperature count in a measurement frame.
    fn temperature(frame: &[u8]) -> u32 {
        (((frame[3] & 0x0F) as u32) << 16) | ((frame[4] as u32) << 8) | frame[5] as u32
    }

    #[test]
    fn it_is_calibrated_at_power_on() {
        let mut chip = chip();
        assert_eq!(status(&mut chip), CALIBRATED_FRAME);
        assert_eq!(CALIBRATED_FRAME, 0x18, "the datasheet's idle status");
    }

    #[test]
    fn the_default_room_is_22_5_c_and_45_percent() {
        let mut chip = chip();
        let frame = measurement(&mut chip);
        assert_eq!(frame[0], CALIBRATED_FRAME);
        assert_eq!(humidity(&frame), 471_859, "45 %RH");
        assert_eq!(temperature(&frame), 380_109, "22.5 °C");
        assert_eq!(frame.len(), MEASUREMENT_LEN);
    }

    #[test]
    fn the_crc_covers_the_status_and_the_five_data_bytes() {
        let mut chip = chip();
        let frame = measurement(&mut chip);
        assert_eq!(frame[6], crc8(&frame[..6]));
        let mut moved = frame.clone();
        moved[3] ^= 0x01;
        assert_ne!(crc8(&moved[..6]), frame[6]);
    }

    #[test]
    fn initialization_sets_the_calibration_bit() {
        let mut chip = chip();
        chip.calibrated = false;
        assert_eq!(status(&mut chip), READY_FRAME);
        assert_eq!(
            transfer(&mut chip, &[INITIALIZE_COMMAND, 0x08, 0x00], 0),
            Some(vec![])
        );
        assert_eq!(status(&mut chip), CALIBRATED_FRAME);
    }

    #[test]
    fn a_measurement_survives_the_input_moving_after_it() {
        let mut chip = chip();
        transfer(&mut chip, &[MEASURE_COMMAND, 0x33, 0x00], 0);
        chip.on_input("set:temperature=30");
        let frame = transfer(&mut chip, &[], MEASUREMENT_LEN as u32).unwrap();
        assert_eq!(temperature(&frame), 380_109, "latched at the trigger");
    }

    #[test]
    fn the_inputs_decode_back_to_the_conditions() {
        let mut chip = chip();
        chip.on_input("set:temperature=-40");
        chip.on_input("set:humidity=100");
        let frame = measurement(&mut chip);
        assert_eq!(temperature(&frame), 52_429, "-40 °C is 0.05 of the range");
        assert_eq!(humidity(&frame), 1_048_575, "the top count");

        chip.on_input("rest");
        let frame = measurement(&mut chip);
        assert_eq!(temperature(&frame), 380_109);
        assert_eq!(humidity(&frame), 471_859);
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x38,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_conditions() {
        let mut chip = chip();
        chip.on_input("set:humidity=60");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("temperature"), shown.get("humidity")),
            (Some("22.5"), Some("60"))
        );
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x38, &[STATUS_COMMAND], 1);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
