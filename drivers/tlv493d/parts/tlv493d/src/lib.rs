//! An Infineon TLV493D-A1B6 3D magnetic sensor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: one read
//! clocks out the ten-byte frame the part holds, starting at the register its
//! address pointer carries (0x00 after power-up) and walking the pointer as
//! the master clocks:
//!
//! | Register | Meaning |
//! |---|---|
//! | `0x00` | Bx, bits 11:4 |
//! | `0x01` | By, bits 11:4 |
//! | `0x02` | Bz, bits 11:4 |
//! | `0x03` | temperature bits 11:8, then the frame counter (3:2) and channel (1:0) |
//! | `0x04` | Bx bits 3:0 in the high nibble, By bits 3:0 in the low nibble |
//! | `0x05` | Bz bits 3:0 in the low nibble; bit 4 is the power-down flag |
//! | `0x06` | temperature bits 7:0 |
//! | `0x07` to `0x09` | the part's factory values |
//!
//! A write's first byte is the register the next read starts at; the bytes
//! after it are the MOD1/MOD2 configuration, which the part accepts and does
//! not model. A read the master splits continues from where the last piece
//! stopped.
//!
//! The part measures continuously: every frame carries the field and the
//! temperature it was set to, and the frame counter advances by one per read.
//!
//! README.md lists what is left out.
//!
//! `Tlv493d` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The address with the ADDR strap low. The high strap, 0x1F, is only reached
/// with a reset sequence that holds SDA high, which the part does not model.
const DEFAULT_ADDR: u32 = 0x5E;

/// The read frame: ten registers from 0x00.
const FRAME_LEN: usize = 10;

/// The register the frame starts at: Bx's top eight bits.
const BX1: usize = 0x00;
/// The temperature's top four bits, the frame counter and the channel.
const TEMP1: usize = 0x03;
/// Bx's low nibble in bits 7:4, By's in bits 3:0.
const BX2: usize = 0x04;
/// Bz's low nibble, and the power-down flag in bit 4.
const BZ2: usize = 0x05;
/// The temperature's low eight bits.
const TEMP2: usize = 0x06;

/// What registers 0x07 to 0x09 hold: the factory values a driver copies back
/// into its configuration before writing it.
const FACTORY: [u8; 3] = [0x18, 0xFF, 0x1F];

/// The field one signed 12-bit count stands for, in milliteslas.
const RESOLUTION_MT: f32 = 0.098;
/// The temperature one count stands for, in degrees Celsius.
const TEMPERATURE_LSB_C: f32 = 1.1;
/// The unsigned count that reads 25 °C.
const TEMPERATURE_ZERO_RAW: i32 = 340;
/// What that count reads.
const TEMPERATURE_ZERO_C: f32 = 25.0;

/// The datasheet's usable linear range on each axis.
const FIELD_LIMIT_MT: f32 = 130.0;
/// The temperature range the sensor is specified over.
const TEMPERATURE_MIN_C: f32 = -40.0;
const TEMPERATURE_MAX_C: f32 = 125.0;

struct Tlv493d {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The magnetic field around the sensor, in milliteslas.
    bx_mt: f32,
    by_mt: f32,
    bz_mt: f32,
    /// The temperature, in degrees Celsius.
    temp_c: f32,
    /// The frame counter the next frame carries: 0 to 3, wrapping.
    frame_counter: u8,
    /// Where a read starts. A write sets it; reads leave it where it is.
    pointer: u8,
    /// The rest of the frame a read has started, which a request that
    /// `continues` that read clocks out next.
    reading: Vec<u8>,
}

impl Default for Tlv493d {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            // No magnet nearby, at room temperature: a fresh part reads
            // plausibly.
            bx_mt: 0.0,
            by_mt: 0.0,
            bz_mt: 0.0,
            temp_c: TEMPERATURE_ZERO_C,
            frame_counter: 0,
            pointer: BX1 as u8,
            reading: Vec::new(),
        }
    }
}

impl Device for Tlv493d {
    const NAME: &'static str = "tlv493d";
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

    /// The world moves a magnet, or warms the part: `set:bx=`, `set:by=`,
    /// `set:bz=` in milliteslas and `set:temperature=` in degrees Celsius.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            let value = value as f32;
            match what {
                "bx" => self.bx_mt = value,
                "by" => self.by_mt = value,
                "bz" => self.bz_mt = value,
                "temperature" => self.temp_c = value,
                _ => {} // Not a value this part has.
            }
        }
        // Kept to what a frame can carry: the linear range on each axis, and
        // the temperature the part is specified over.
        self.bx_mt = self.bx_mt.clamp(-FIELD_LIMIT_MT, FIELD_LIMIT_MT);
        self.by_mt = self.by_mt.clamp(-FIELD_LIMIT_MT, FIELD_LIMIT_MT);
        self.bz_mt = self.bz_mt.clamp(-FIELD_LIMIT_MT, FIELD_LIMIT_MT);
        self.temp_c = self.temp_c.clamp(TEMPERATURE_MIN_C, TEMPERATURE_MAX_C);
    }

    /// The guest's I2C transaction: a write sets the register the next read
    /// starts at, then the read, if any, is answered from there.
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
        if let Some((&register, _config)) = i2c.write.split_first() {
            if usize::from(register) >= FRAME_LEN {
                // Not a register the part has.
                return Some(reply);
            }
            self.pointer = register;
        }
        if i2c.read_len > 0 {
            if !i2c.continues {
                self.reading = self.frame();
            }
            // A read that continues another, split by the master's driver,
            // carries on where that one stopped. Past the frame nothing
            // drives the line, and the master reads 0xFF.
            let n = (i2c.read_len as usize).min(self.reading.len());
            let mut data: Vec<u8> = self.reading.drain(..n).collect();
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
                .set("bx", self.bx_mt)
                .set("by", self.by_mt)
                .set("bz", self.bz_mt)
                .set("temperature", self.temp_c),
        )
    }
}

impl Tlv493d {
    /// The ten frame bytes, starting at the pointer and wrapping at the end of
    /// the frame, with the frame counter advanced for the next one.
    fn frame(&mut self) -> Vec<u8> {
        let bx = field_raw(self.bx_mt);
        let by = field_raw(self.by_mt);
        let bz = field_raw(self.bz_mt);
        let temperature = temperature_raw(self.temp_c);
        let counter = self.frame_counter << 2;
        self.frame_counter = (self.frame_counter + 1) & 0x03;
        let mut frame = vec![0u8; FRAME_LEN];
        frame[BX1] = (bx >> 4) as u8;
        frame[BX1 + 1] = (by >> 4) as u8;
        frame[BX1 + 2] = (bz >> 4) as u8;
        // Channel 0 says the whole frame is from one measurement, and the
        // power-down flag stays clear: the part keeps measuring.
        frame[TEMP1] = ((temperature >> 8) as u8 & 0x0F) << 4 | counter;
        frame[BX2] = ((bx & 0x0F) as u8) << 4 | (by & 0x0F) as u8;
        frame[BZ2] = (bz & 0x0F) as u8;
        frame[TEMP2] = temperature as u8;
        frame[0x07..].copy_from_slice(&FACTORY);
        let start = usize::from(self.pointer);
        frame[start..].iter().chain(&frame[..start]).copied().collect()
    }
}

/// A field as the sensor measures it: a signed 12-bit count.
fn field_raw(mt: f32) -> u16 {
    (((mt / RESOLUTION_MT).round() as i32).clamp(-2048, 2047) as u16) & 0x0FFF
}

/// A temperature as the sensor measures it: an unsigned 12-bit count, 340 at
/// 25 °C and 1.1 °C per count.
fn temperature_raw(celsius: f32) -> u16 {
    let raw = TEMPERATURE_ZERO_RAW as f32 + (celsius - TEMPERATURE_ZERO_C) / TEMPERATURE_LSB_C;
    (raw.round() as i32).clamp(0, 0xFFF) as u16
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Tlv493d, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the default 0x5E address.
    fn chip() -> Tlv493d {
        let mut chip = Tlv493d::default();
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
    fn transfer(chip: &mut Tlv493d, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// A frame of no field at 25 °C, with the given frame counter.
    fn room(counter: u8) -> Vec<u8> {
        vec![0x00, 0x00, 0x00, 0x10 | counter << 2, 0x00, 0x00, 0x54, 0x18, 0xFF, 0x1F]
    }

    #[test]
    fn a_fresh_part_reads_no_field_at_25_c() {
        let chip = chip();
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("bx"), shown.get("by"), shown.get("bz")),
            (Some("0"), Some("0"), Some("0"))
        );
        assert_eq!(shown.get("temperature"), Some("25"));
    }

    #[test]
    fn a_read_clocks_out_the_whole_frame() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[], 10), Some(room(0)));
    }

    #[test]
    fn the_frame_counter_advances_once_per_read_and_wraps() {
        let mut chip = chip();
        for counter in [0u8, 1, 2, 3, 0] {
            assert_eq!(
                transfer(&mut chip, &[], 10).map(|frame| frame[TEMP1]),
                Some(0x10 | counter << 2),
                "frame counter {counter}"
            );
        }
    }

    #[test]
    fn a_field_goes_out_as_signed_twelve_bit_nibbles() {
        let mut chip = chip();
        chip.on_input("set:bx=0.98");
        chip.on_input("set:bz=-0.49");
        let frame = transfer(&mut chip, &[], 10).unwrap();
        // Bx is count 10: 0x00A across registers 0x00 and 0x04.
        assert_eq!((frame[0x00], frame[0x04] >> 4), (0x00, 0x0A));
        // Bz is count -5: 0xFFB across registers 0x02 and 0x05.
        assert_eq!((frame[0x02], frame[0x05] & 0x0F), (0xFF, 0x0B));
        assert_eq!(frame[0x01], 0x00, "By is untouched");
    }

    #[test]
    fn the_temperature_is_twelve_bits_over_two_registers() {
        let mut chip = chip();
        chip.on_input("set:temperature=26.1");
        let frame = transfer(&mut chip, &[], 10).unwrap();
        // 341 is 0x155: its top nibble is in 0x03, the rest in 0x06.
        assert_eq!((frame[0x03] & 0xF0, frame[0x06]), (0x10, 0x55));
        chip.on_input("set:temperature=23.9");
        let frame = transfer(&mut chip, &[], 10).unwrap();
        // 339 is 0x153.
        assert_eq!((frame[0x03] & 0xF0, frame[0x06]), (0x10, 0x53));
    }

    #[test]
    fn the_factory_registers_are_the_parts_own() {
        let mut chip = chip();
        let frame = transfer(&mut chip, &[], 10).unwrap();
        assert_eq!(&frame[0x07..], &FACTORY);
    }

    #[test]
    fn a_write_points_the_next_read_at_its_register() {
        let mut chip = chip();
        // Register 0x06 first: a read wraps at the end of the frame.
        assert_eq!(transfer(&mut chip, &[0x06], 0), Some(vec![]));
        assert_eq!(
            transfer(&mut chip, &[], 10),
            Some(vec![0x54, 0x18, 0xFF, 0x1F, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00])
        );
        // And back to the frame start.
        assert_eq!(transfer(&mut chip, &[0x00], 10), Some(room(1)));
    }

    #[test]
    fn the_configuration_bytes_after_the_register_are_ignored() {
        let mut chip = chip();
        // MOD1 in low-power mode, with its parity bit, and MOD2 as it is.
        assert_eq!(transfer(&mut chip, &[0x00, 0x81, 0x00, 0x00], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[], 10), Some(room(0)));
    }

    #[test]
    fn a_write_to_a_register_the_part_lacks_is_nacked() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[0x0A], 0), None);
    }

    #[test]
    fn a_read_split_by_the_driver_continues_where_it_stopped() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[], 5), Some(room(0)[..5].to_vec()));
        let mut rest = request(0x5E, &[], 5);
        if let Some(bus_request::Op::I2c(i2c)) = &mut rest.op {
            i2c.continues = true;
        }
        let reply = chip.on_bus(&rest).unwrap();
        assert_eq!((reply.ack, reply.data), (true, room(0)[5..].to_vec()));
    }

    #[test]
    fn a_read_past_the_frame_is_padded() {
        let mut chip = chip();
        let padded = [&room(0)[..], &[0xFF, 0xFF]].concat();
        assert_eq!(transfer(&mut chip, &[], 12), Some(padded));
    }

    #[test]
    fn the_inputs_stop_at_the_range() {
        let mut chip = chip();
        chip.on_input("set:bx=1000");
        chip.on_input("set:by=-1000");
        chip.on_input("set:temperature=500");
        assert_eq!(
            (chip.bx_mt, chip.by_mt, chip.temp_c),
            (FIELD_LIMIT_MT, -FIELD_LIMIT_MT, TEMPERATURE_MAX_C)
        );
        chip.on_input("set:temperature=-273");
        assert_eq!(chip.temp_c, TEMPERATURE_MIN_C);
    }

    #[test]
    fn it_shows_what_it_was_set_to() {
        let mut chip = chip();
        chip.on_input("set:bx=0.98");
        chip.on_input("set:temperature=26.1");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("bx"), shown.get("temperature")),
            (Some("0.98"), Some("26.1"))
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x5E,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x5E, &[], 10);
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
        let reply = chip.on_bus(&request(0x1F, &[], 10)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x1F");
        assert!(reply.data.is_empty());
    }
}
