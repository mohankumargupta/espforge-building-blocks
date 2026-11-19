//! A TI TMP102 temperature sensor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: a write's first
//! byte is the register pointer, and the read that follows starts there and
//! walks the register file, wrapping at its end:
//!
//! | Pointer | Register | Bytes |
//! |---|---|---|
//! | `0x00` | temperature (read-only) | signed 12-bit, left justified |
//! | `0x01` | configuration | the power-up `0x60A0` |
//! | `0x02` | T_LOW | `0x4B00`, -75 C |
//! | `0x03` | T_HIGH | `0x5000`, 80 C |
//!
//! The part converts continuously at its power-up configuration: the
//! temperature it was set to is what register 0x00 answers with, in the 12-bit
//! format (0.0625 °C per count, two's complement). Configuration writes are
//! accepted and not modelled, so the configuration register keeps its power-up
//! value.
//!
//! README.md lists what is left out.
//!
//! `Tmp102` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// ADD0 tied to ground. The other straps are 0x49 (V+), 0x4A (SDA) and 0x4B
/// (SCL), which the part does not model.
const DEFAULT_ADDR: u32 = 0x48;

/// The register file: four registers of two bytes, temperature first.
const REGISTER_BYTES: usize = 8;
/// The temperature register's first byte.
const TEMPERATURE: usize = 0x00;
/// The configuration register: its power-up value, 12-bit and continuous.
const CONFIG: [u8; 2] = [0x60, 0xA0];
/// T_LOW's power-up value: -75 C.
const T_LOW: [u8; 2] = [0x4B, 0x00];
/// T_HIGH's power-up value: 80 C.
const T_HIGH: [u8; 2] = [0x50, 0x00];

/// The temperature one count stands for, in degrees Celsius.
const TEMPERATURE_LSB_C: f32 = 0.0625;
/// The range the part is specified over.
const TEMPERATURE_MIN_C: f32 = -40.0;
const TEMPERATURE_MAX_C: f32 = 125.0;

struct Tmp102 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The temperature around the sensor, in degrees Celsius.
    temp_c: f32,
    /// Where a read starts: a byte in the register file. A write sets it;
    /// reads advance it and wrap at the end of the file.
    pointer: usize,
}

impl Default for Tmp102 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            // Room temperature: a fresh part reads plausibly.
            temp_c: 25.0,
            pointer: TEMPERATURE,
        }
    }
}

impl Device for Tmp102 {
    const NAME: &'static str = "tmp102";
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

    /// The world warms or cools the part: `set:temperature=` in degrees
    /// Celsius.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            if what == "temperature" {
                self.temp_c = value as f32;
            }
        }
        self.temp_c = self.temp_c.clamp(TEMPERATURE_MIN_C, TEMPERATURE_MAX_C);
    }

    /// The guest's I2C transaction: a write sets the register pointer, then
    /// the read, if any, is answered from there.
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
        if let Some((&register, _configuration)) = i2c.write.split_first() {
            if usize::from(register) * 2 >= REGISTER_BYTES {
                // Not a register the part has.
                return Some(reply);
            }
            self.pointer = usize::from(register) * 2;
        }
        if i2c.read_len > 0 {
            let file = self.registers();
            let start = self.pointer;
            reply.data = (0..i2c.read_len as usize)
                .map(|i| file[(start + i) % REGISTER_BYTES])
                .collect();
            // The pointer walks on, wrapping at the end of the register file.
            self.pointer = (start + i2c.read_len as usize) % REGISTER_BYTES;
        }
        // A probe, a write on its own, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(State::new().set("temperature", self.temp_c))
    }
}

impl Tmp102 {
    /// The register file, temperature first.
    fn registers(&self) -> [u8; REGISTER_BYTES] {
        let [top, bottom] = temperature_raw(self.temp_c).to_be_bytes();
        let mut file = [0u8; REGISTER_BYTES];
        file[TEMPERATURE] = top;
        file[TEMPERATURE + 1] = bottom;
        file[2..4].copy_from_slice(&CONFIG);
        file[4..6].copy_from_slice(&T_LOW);
        file[6..8].copy_from_slice(&T_HIGH);
        file
    }
}

/// A temperature as the part measures it: a signed 12-bit count, left
/// justified in sixteen bits, so 0.0625 °C per count.
fn temperature_raw(celsius: f32) -> u16 {
    let counts = (celsius / TEMPERATURE_LSB_C).round() as i32;
    ((counts.clamp(-2048, 2047) as i16) << 4) as u16
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Tmp102, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the default 0x48 address.
    fn chip() -> Tmp102 {
        let mut chip = Tmp102::default();
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
    fn transfer(chip: &mut Tmp102, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// The temperature register as a read of it answers: two bytes, 25 C being
    /// count 400 (0x190).
    const ROOM: [u8; 2] = [0x19, 0x00];

    #[test]
    fn a_fresh_part_reads_25_c() {
        let chip = chip();
        assert_eq!(chip.state().unwrap().get("temperature"), Some("25"));
    }

    #[test]
    fn a_read_of_the_temperature_register_is_two_left_justified_bytes() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x00], 2), Some(ROOM.to_vec()));
    }

    #[test]
    fn the_temperature_goes_out_as_a_signed_twelve_bit_value() {
        let mut chip = chip();
        // 26.5 C is count 424 (0x1A8); -5 C is count -80 (0xFB0).
        for (input, bytes) in [
            ("set:temperature=26.5", [0x1A, 0x80]),
            ("set:temperature=-5", [0xFB, 0x00]),
        ] {
            chip.on_input(input);
            assert_eq!(
                transfer(&mut chip, &[0x00], 2),
                Some(bytes.to_vec()),
                "{input}"
            );
        }
    }

    #[test]
    fn the_pointer_walks_the_register_file_and_wraps() {
        let mut chip = chip();
        // Four registers of two bytes, temperature first: reading on without a
        // write walks into the configuration, T_LOW, T_HIGH and back round.
        assert_eq!(
            transfer(&mut chip, &[0x00], 8),
            Some([&ROOM[..], &CONFIG, &T_LOW, &T_HIGH].concat())
        );
        // A read past the end wraps to the temperature instead of padding.
        assert_eq!(
            transfer(&mut chip, &[0x00], 10),
            Some([&ROOM[..], &CONFIG, &T_LOW, &T_HIGH, &ROOM[..]].concat())
        );
    }

    #[test]
    fn a_write_to_the_configuration_is_accepted_and_ignored() {
        let mut chip = chip();
        // 13-bit extended mode and one-shot, which the part does not model.
        assert_eq!(
            transfer(&mut chip, &[0x01, 0x6C, 0xA0], 0),
            Some(Vec::new())
        );
        assert_eq!(
            transfer(&mut chip, &[0x00], 2),
            Some(ROOM.to_vec()),
            "still 12-bit continuous at 25 C"
        );
        // The configuration register keeps its power-up value.
        assert_eq!(
            transfer(&mut chip, &[0x01], 2),
            Some(CONFIG.to_vec())
        );
    }

    #[test]
    fn a_write_to_a_register_the_part_lacks_is_nacked() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x04], 0), None);
        assert_eq!(transfer(&mut chip, &[0xFF], 0), None);
    }

    #[test]
    fn a_read_split_by_the_driver_continues_where_it_stopped() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x00], 1), Some(vec![0x19]));
        let mut rest = request(0x48, &[], 1);
        if let Some(bus_request::Op::I2c(i2c)) = &mut rest.op {
            i2c.continues = true;
        }
        let reply = chip.on_bus(&rest).unwrap();
        assert_eq!((reply.ack, reply.data), (true, vec![0x00]));
    }

    #[test]
    fn the_input_stops_at_the_range() {
        let mut chip = chip();
        chip.on_input("set:temperature=500");
        assert_eq!(chip.temp_c, TEMPERATURE_MAX_C);
        chip.on_input("set:temperature=-273");
        assert_eq!(chip.temp_c, TEMPERATURE_MIN_C);
    }

    #[test]
    fn it_shows_what_it_was_set_to() {
        let mut chip = chip();
        chip.on_input("set:temperature=26.5");
        assert_eq!(chip.state().unwrap().get("temperature"), Some("26.5"));
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x48,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x48, &[0x00], 2);
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
        let reply = chip.on_bus(&request(0x49, &[], 2)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x49");
        assert!(reply.data.is_empty());
    }
}
