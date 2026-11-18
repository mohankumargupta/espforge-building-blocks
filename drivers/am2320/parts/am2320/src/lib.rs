//! An AOSONG AM2320 temperature and relative humidity sensor, written as a
//! virtual part.
//!
//! The firmware talks to it the way it would to the real chip: a write of
//! `0x03, 0x00, 0x04` asks for the humidity and temperature registers, and the
//! next read takes eight bytes — the function code, the byte count, the two
//! 16-bit big-endian registers and their CRC-16 (Modbus, low byte first).
//! Each register is a tenth of a unit and the temperature's top bit is its
//! sign. The part keeps a temperature and a humidity, and encodes them the way
//! the datasheet's formulas invert. README.md lists what is left out.
//!
//! `Am2320` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The AM2320's only strapped address.
const DEFAULT_ADDR: u32 = 0x5C;

/// Function code for a register read, and the register window the driver asks
/// for: humidity at 0x0000, temperature at 0x0001.
const FUNCTION_READ: u8 = 0x03;
const START_REGISTER: u8 = 0x00;
const REGISTER_COUNT: u8 = 0x04;

/// Function code, byte count, humidity, temperature, CRC-16.
const FRAME_LEN: usize = 8;

/// What the part reports when nobody has changed it: a mild room.
const DEFAULT_TEMPERATURE_C: f64 = 22.5;
const DEFAULT_HUMIDITY_PERCENT: f64 = 45.0;
/// The chip's operating range.
const MIN_TEMPERATURE_C: f64 = -40.0;
const MAX_TEMPERATURE_C: f64 = 80.0;
const MIN_HUMIDITY_PERCENT: f64 = 0.0;
const MAX_HUMIDITY_PERCENT: f64 = 99.9;

struct Am2320 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The conditions it measures, in °C and %RH.
    temperature_c: f64,
    humidity_percent: f64,
    /// The answer the last command prepared, which the next read clocks out.
    pending: Vec<u8>,
}

impl Default for Am2320 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            temperature_c: DEFAULT_TEMPERATURE_C,
            humidity_percent: DEFAULT_HUMIDITY_PERCENT,
            pending: Vec::new(),
        }
    }
}

impl Device for Am2320 {
    const NAME: &'static str = "am2320";
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

    /// The guest's I2C transaction: a matching write latches the four
    /// registers, then a read, if there is one, drains the answer.
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
        // A read on its own takes what the last command prepared; only a
        // write changes it.
        if !i2c.write.is_empty() {
            match i2c.write.as_slice() {
                [FUNCTION_READ, START_REGISTER, REGISTER_COUNT] => {
                    self.pending = self.registers();
                }
                // Anything else: a wake-up probe, or a window this part does
                // not serve. Nothing is left for the next read.
                _ => self.pending.clear(),
            }
        }
        if i2c.read_len > 0 {
            // A read takes what the last command prepared. Past the answer
            // nothing drives the line, and the master reads 0xFF.
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

impl Am2320 {
    /// The eight bytes a read answers: the function code, the byte count, the
    /// two registers and the CRC-16 over the first six bytes, low byte first.
    fn registers(&self) -> Vec<u8> {
        let humidity = humidity_tenths(self.humidity_percent);
        let temperature = temperature_tenths(self.temperature_c);
        let mut frame = vec![
            FUNCTION_READ,
            REGISTER_COUNT,
            (humidity >> 8) as u8,
            humidity as u8,
            (temperature >> 8) as u8,
            temperature as u8,
        ];
        let crc = crc16(&frame);
        frame.push(crc as u8);
        frame.push((crc >> 8) as u8);
        debug_assert_eq!(frame.len(), FRAME_LEN);
        frame
    }
}

/// One humidity reading in the chip's tenths of a percent.
fn humidity_tenths(humidity_percent: f64) -> u16 {
    (humidity_percent.clamp(MIN_HUMIDITY_PERCENT, MAX_HUMIDITY_PERCENT) * 10.0).round() as u16
}

/// One temperature reading in the chip's tenths of a degree, with the sign in
/// the top bit.
fn temperature_tenths(temperature_c: f64) -> u16 {
    let tenths = (temperature_c
        .clamp(MIN_TEMPERATURE_C, MAX_TEMPERATURE_C)
        .abs()
        * 10.0)
        .round() as u16;
    if temperature_c < 0.0 {
        tenths | 0x8000
    } else {
        tenths
    }
}

/// The datasheet's CRC-16: Modbus, polynomial 0xA001, initial value 0xFFFF.
fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for &byte in bytes {
        crc ^= byte as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Am2320, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the part's fixed address.
    fn chip() -> Am2320 {
        let mut chip = Am2320::default();
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
    fn transfer(chip: &mut Am2320, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// One reading, the way the driver takes it: the read command, then eight
    /// bytes with no command.
    fn measurement(chip: &mut Am2320) -> Vec<u8> {
        assert_eq!(
            transfer(chip, &[FUNCTION_READ, START_REGISTER, REGISTER_COUNT], 0),
            Some(vec![]),
            "the read command is acknowledged"
        );
        transfer(chip, &[], FRAME_LEN as u32).expect("eight bytes")
    }

    /// The humidity register in an answer.
    fn humidity(frame: &[u8]) -> u16 {
        u16::from_be_bytes([frame[2], frame[3]])
    }

    /// The temperature register in an answer.
    fn temperature(frame: &[u8]) -> u16 {
        u16::from_be_bytes([frame[4], frame[5]])
    }

    #[test]
    fn the_default_room_is_22_5_c_and_45_percent() {
        let mut chip = chip();
        let frame = measurement(&mut chip);
        assert_eq!((frame[0], frame[1]), (FUNCTION_READ, REGISTER_COUNT));
        assert_eq!(humidity(&frame), 450, "45 %RH in tenths");
        assert_eq!(temperature(&frame), 225, "22.5 °C in tenths");
        assert_eq!(frame.len(), FRAME_LEN);
    }

    #[test]
    fn the_crc_is_modbus_and_low_byte_first() {
        let mut chip = chip();
        let frame = measurement(&mut chip);
        assert_eq!(u16::from_le_bytes([frame[6], frame[7]]), crc16(&frame[..6]));
        let mut moved = frame.clone();
        moved[2] ^= 0x01;
        assert_ne!(crc16(&moved[..6]), u16::from_le_bytes([frame[6], frame[7]]));
    }

    #[test]
    fn a_negative_temperature_sets_the_top_bit() {
        let mut chip = chip();
        chip.on_input("set:temperature=-5");
        let frame = measurement(&mut chip);
        assert_eq!(temperature(&frame), 0x8000 | 50);
    }

    #[test]
    fn a_measurement_survives_the_input_moving_after_it() {
        let mut chip = chip();
        transfer(&mut chip, &[FUNCTION_READ, START_REGISTER, REGISTER_COUNT], 0);
        chip.on_input("set:temperature=30");
        let frame = transfer(&mut chip, &[], FRAME_LEN as u32).unwrap();
        assert_eq!(temperature(&frame), 225, "latched at the command");
    }

    #[test]
    fn the_inputs_decode_back_to_the_conditions() {
        let mut chip = chip();
        chip.on_input("set:temperature=-40");
        chip.on_input("set:humidity=99.9");
        let frame = measurement(&mut chip);
        assert_eq!(temperature(&frame), 0x8000 | 400);
        assert_eq!(humidity(&frame), 999);

        chip.on_input("rest");
        let frame = measurement(&mut chip);
        assert_eq!(temperature(&frame), 225);
        assert_eq!(humidity(&frame), 450);
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x5C,
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
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip
            .on_bus(&request(0x5D, &[FUNCTION_READ, START_REGISTER, REGISTER_COUNT], 0))
            .unwrap();
        assert!(!reply.ack, "nothing answers at 0x5D");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x5C, &[FUNCTION_READ, START_REGISTER, REGISTER_COUNT], 0);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
