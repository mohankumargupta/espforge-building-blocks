//! A Texas Instruments ADS1115 16-bit analog-to-digital converter, written as
//! a virtual part.
//!
//! The firmware reads it over I2C the way it would the real chip: a write of
//! the pointer register, then a read (or one combined `write_read`), with the
//! pointer auto-incrementing across the four 16-bit registers. `config`
//! (`0x01`) is writable and starts a single-shot conversion when bit 15 is
//! written; `conversion` (`0x00`) then holds the code for the input and gain
//! that register selects. The four channel voltages are not stored as counts:
//! the part keeps the voltage on each pin and encodes the code a real ADS1115
//! would have produced. README.md lists what is left out.
//!
//! `Ads1x15` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// ADDR tied to GND. VDD, SDA and SCL strap 0x49, 0x4a and 0x4b, which the
/// board gives as `addr`.
const DEFAULT_ADDR: u32 = 0x48;

/// `conversion` (`0x00`) and `config` (`0x01`); `0x02`/`0x03` are the
/// comparator thresholds.
const CONVERSION: u8 = 0x00;
const CONFIG: u8 = 0x01;

/// `OS` (bit 15): written 1 to start a single conversion, reads 1 when idle.
const OS: u16 = 0x8000;
/// `MUX` (bits 14:12) and `PGA` (bits 11:9).
const MUX_SHIFT: u16 = 12;
const PGA_SHIFT: u16 = 9;
/// `MODE` (bit 8): 1 is single-shot.
const MODE_SINGLE: u16 = 0x0100;

/// The power-on configuration: single-shot, AIN0-AIN1, ±2.048 V, 128 SPS,
/// comparator disabled (datasheet reset value).
const DEFAULT_CONFIG: u16 = 0x8583;
/// The comparator thresholds' power-on values.
const DEFAULT_LO_THRESH: u16 = 0x8000;
const DEFAULT_HI_THRESH: u16 = 0x7FFF;

/// The full-scale range for each `PGA` code, in volts. 110 and 111 repeat
/// ±0.256 V, as the datasheet says.
const FULL_SCALE_V: [f64; 8] = [6.144, 4.096, 2.048, 1.024, 0.512, 0.256, 0.256, 0.256];
/// 16-bit two's complement: -32768 counts is negative full scale.
const MAX_COUNTS: f64 = 32_768.0;

/// What the pins carry when nobody has changed them.
const DEFAULT_INPUTS: [f64; 4] = [0.512, 1.024, 1.5, 0.128];
/// The largest input the chip's absolute maximum rating allows.
const MAX_INPUT_V: f64 = 6.144;

struct Ads1x15 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The voltage on AIN0..=AIN3, in volts.
    inputs: [f64; 4],
    /// The configuration register, and the comparator thresholds.
    config: u16,
    lo_thresh: u16,
    hi_thresh: u16,
    /// The last single-shot result, in the chip's 16-bit layout.
    conversion: u16,
    /// The register byte the next read starts at: the last pointer written.
    pointer: u8,
    /// The rest of an answer a read has started, which a request that
    /// `continues` that read clocks out next.
    reading: Vec<u8>,
}

impl Default for Ads1x15 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            inputs: DEFAULT_INPUTS,
            config: DEFAULT_CONFIG,
            lo_thresh: DEFAULT_LO_THRESH,
            hi_thresh: DEFAULT_HI_THRESH,
            conversion: 0,
            pointer: 0,
            reading: Vec::new(),
        }
    }
}

impl Device for Ads1x15 {
    const NAME: &'static str = "ads1x15";
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

    /// The address this ADC answers for, on the two pins it is wired to. The
    /// emulator works out the I2C port from those pins.
    fn claims(&self) -> Vec<Claim> {
        vec![Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: self.addr,
                scl_pin: self.scl,
                sda_pin: self.sda,
            })),
        }]
    }

    /// Change a pin voltage. `set:ain0=<V>` .. `set:ain3=<V>` put an exact
    /// voltage on one input; `rest` puts the defaults back.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            if let Some(channel) = what
                .strip_prefix("ain")
                .and_then(|channel| channel.parse::<usize>().ok())
            {
                if let Some(input) = self.inputs.get_mut(channel) {
                    *input = value;
                }
            }
        } else if name == "rest" {
            self.inputs = DEFAULT_INPUTS;
        }
        for input in &mut self.inputs {
            *input = input.clamp(0.0, MAX_INPUT_V);
        }
    }

    /// The guest's I2C transaction to this part: the write, if there is one,
    /// sets the register pointer (and any value with it), then the read, if
    /// any, is answered from that pointer.
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
        if let Some(&register) = i2c.write.first() {
            self.pointer = register;
            // A write with a value is a register write; the bytes after the
            // first fill that register's two halves, then the next register's,
            // as the chip's pointer does.
            let mut config_written = false;
            for (offset, &value) in i2c.write[1..].iter().enumerate() {
                let target = register.wrapping_add((offset / 2) as u8);
                config_written |= target & 0x03 == CONFIG;
                self.write(target, offset % 2 == 1, value);
            }
            if i2c.write.len() > 1 {
                self.pointer = register.wrapping_add(((i2c.write.len() - 1) / 2) as u8);
            }
            // Writing `OS` in single-shot mode starts a conversion. Nothing is
            // in progress for any measurable time, so it completes at once.
            if config_written && self.config & OS != 0 && self.config & MODE_SINGLE != 0 {
                self.conversion = self.convert();
            }
        }
        if i2c.read_len > 0 {
            if !i2c.continues {
                // A read after a START takes the answer from the pointer. Any
                // earlier part-read answer is dropped.
                let start = self.pointer;
                self.reading = (0..i2c.read_len as usize)
                    .map(|i| self.register_byte(start.wrapping_add((i / 2) as u8), i % 2 == 1))
                    .collect();
                self.pointer = start.wrapping_add(((i2c.read_len as usize + 1) / 2) as u8);
            }
            // A read that continues another, split by the master's driver,
            // carries on where that one stopped. Past the answer nothing
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
                .set("ain0", self.inputs[0])
                .set("ain1", self.inputs[1])
                .set("ain2", self.inputs[2])
                .set("ain3", self.inputs[3]),
        )
    }
}

impl Ads1x15 {
    /// One of the four 16-bit registers.
    fn word(&self, register: u8) -> u16 {
        match register & 0x03 {
            CONVERSION => self.conversion_word(),
            // `OS` reads 1: nothing is ever in progress for a measurable time.
            CONFIG => self.config | OS,
            0x02 => self.lo_thresh,
            _ => self.hi_thresh,
        }
    }

    /// One byte of a register, big-endian.
    fn register_byte(&self, register: u8, low: bool) -> u8 {
        let word = self.word(register);
        if low {
            word as u8
        } else {
            (word >> 8) as u8
        }
    }

    /// A write of one half of a register. `conversion` is read-only; the other
    /// three take big-endian bytes.
    fn write(&mut self, register: u8, low: bool, value: u8) {
        match register & 0x03 {
            CONVERSION => {} // `conversion` ignores writes
            CONFIG => put_byte(&mut self.config, low, value),
            0x02 => put_byte(&mut self.lo_thresh, low, value),
            _ => put_byte(&mut self.hi_thresh, low, value),
        }
    }

    /// The conversion register: the latched single-shot result, or a fresh one
    /// each read in continuous mode.
    fn conversion_word(&self) -> u16 {
        if self.config & MODE_SINGLE != 0 {
            self.conversion
        } else {
            self.convert()
        }
    }

    /// The code the current input and gain produce, in the chip's 16-bit
    /// two's-complement layout.
    fn convert(&self) -> u16 {
        let mux = ((self.config >> MUX_SHIFT) & 0x7) as usize;
        let pga = ((self.config >> PGA_SHIFT) & 0x7) as usize;
        let full_scale = FULL_SCALE_V[pga];
        let (positive, negative) = match mux {
            0 => (self.inputs[0], self.inputs[1]),
            1 => (self.inputs[0], self.inputs[3]),
            2 => (self.inputs[1], self.inputs[3]),
            3 => (self.inputs[2], self.inputs[3]),
            // 100..=111: AIN0..=AIN3 against ground.
            other => (self.inputs[other - 4], 0.0),
        };
        let volts = (positive - negative).clamp(-full_scale, full_scale);
        let counts = (volts / (full_scale / MAX_COUNTS)).round();
        (counts.clamp(-MAX_COUNTS, MAX_COUNTS - 1.0) as i32 as i16) as u16
    }
}

/// Store one big-endian byte of a 16-bit register.
fn put_byte(register: &mut u16, low: bool, value: u8) {
    if low {
        *register = (*register & 0xFF00) | value as u16;
    } else {
        *register = (*register & 0x00FF) | (value as u16) << 8;
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Ads1x15, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// and strapped to `addr`.
    fn wired(addr: u32) -> Ads1x15 {
        let mut chip = Ads1x15::default();
        let pins = [(SCL.to_string(), 6), (SDA.to_string(), 5)].into();
        chip.configure(&PartConfig { pins, addr });
        chip
    }

    fn chip() -> Ads1x15 {
        wired(0)
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

    /// One transaction to the ADC's address: the bytes read back, or `None`
    /// for a NACK.
    fn transfer(chip: &mut Ads1x15, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// A register read the way `embedded-hal`'s `write_read` sends it: the
    /// pointer and the answer in one transaction.
    fn read(chip: &mut Ads1x15, register: u8, len: u32) -> Vec<u8> {
        transfer(chip, &[register], len).expect("a read is answered")
    }

    /// Start one single-shot conversion on `mux` at `pga`, as the driver does.
    fn start(chip: &mut Ads1x15, mux: u16, pga: u16) {
        let config = OS | mux << 12 | pga << 9 | MODE_SINGLE | 0x0080 | 0x0003;
        let bytes = config.to_be_bytes();
        assert_eq!(
            transfer(chip, &[CONFIG, bytes[0], bytes[1]], 0),
            Some(vec![]),
            "a config write is acknowledged"
        );
    }

    /// The conversion register as a signed count.
    fn counts(chip: &mut Ads1x15) -> i16 {
        let raw = read(chip, CONVERSION, 2);
        i16::from_be_bytes([raw[0], raw[1]])
    }

    #[test]
    fn the_power_on_registers_are_the_datasheets() {
        let mut chip = chip();
        let config = read(&mut chip, CONFIG, 2);
        assert_eq!(u16::from_be_bytes([config[0], config[1]]), DEFAULT_CONFIG);
        assert!(config[0] & 0x80 != 0, "OS reads 1 when idle");
        assert_eq!(counts(&mut chip), 0, "no conversion yet");
        let lo = read(&mut chip, 0x02, 2);
        assert_eq!(u16::from_be_bytes([lo[0], lo[1]]), DEFAULT_LO_THRESH);
        let hi = read(&mut chip, 0x03, 2);
        assert_eq!(u16::from_be_bytes([hi[0], hi[1]]), DEFAULT_HI_THRESH);
    }

    #[test]
    fn a_single_ended_conversion_is_the_channel_voltage() {
        let mut chip = chip();
        // ±2.048 V on a 16-bit part: 62.5 µV per count.
        for (channel, expected) in [(4u16, 8192i16), (5, 16_384), (6, 24_000), (7, 2048)] {
            start(&mut chip, channel, 0b010);
            assert_eq!(counts(&mut chip), expected, "AIN{}-GND", channel - 4);
        }
    }

    #[test]
    fn a_differential_conversion_is_the_difference() {
        let mut chip = chip();
        start(&mut chip, 0, 0b010); // AIN0 - AIN1
        assert_eq!(counts(&mut chip), -8_192);
        start(&mut chip, 1, 0b010); // AIN0 - AIN3
        assert_eq!(counts(&mut chip), 6_144);
        start(&mut chip, 2, 0b010); // AIN1 - AIN3
        assert_eq!(counts(&mut chip), 14_336);
        start(&mut chip, 3, 0b010); // AIN2 - AIN3
        assert_eq!(counts(&mut chip), 21_952);
    }

    #[test]
    fn the_gain_sets_the_lsb() {
        let mut chip = chip();
        // ±1.024 V: AIN0's 0.512 V is half the range.
        start(&mut chip, 0b100, 0b011);
        assert_eq!(counts(&mut chip), 16_384);
        // ±0.256 V: AIN3's 0.128 V is half the range, at 8x the resolution.
        start(&mut chip, 0b111, 0b101);
        assert_eq!(counts(&mut chip), 16_384);
        // ±6.144 V: AIN3's 0.128 V is 683 counts.
        start(&mut chip, 0b111, 0b000);
        assert_eq!(counts(&mut chip), 683);
    }

    #[test]
    fn the_inputs_move_one_channel_at_a_time() {
        let mut chip = chip();
        chip.on_input("set:ain0=1.25");
        start(&mut chip, 0b100, 0b010);
        assert_eq!(counts(&mut chip), 20_000);
        chip.on_input("set:ain2=1");
        start(&mut chip, 0b110, 0b010);
        assert_eq!(counts(&mut chip), 16_000);
        chip.on_input("rest");
        start(&mut chip, 0b110, 0b010);
        assert_eq!(counts(&mut chip), 24_000);
    }

    #[test]
    fn a_pointer_write_then_a_read_auto_increments() {
        let mut chip = chip();
        // The default config, both bytes, from one pointer write.
        assert_eq!(transfer(&mut chip, &[CONFIG], 0), Some(vec![]));
        let kept = transfer(&mut chip, &[], 2).expect("a read");
        assert_eq!(kept, [0x85, 0x83]);
        assert_eq!(kept, read(&mut chip, CONFIG, 2), "pointer kept");
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let i2c = |addr| Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [i2c(0x48)]);
        // ADDR strapped high: the board says 0x49.
        assert_eq!(wired(0x49).claims(), [i2c(0x49)]);
    }

    #[test]
    fn it_shows_the_pin_voltages() {
        let mut chip = chip();
        chip.on_input("set:ain3=2.5");
        let shown = chip.state().unwrap();
        assert_eq!(
            (
                shown.get("ain0"),
                shown.get("ain3")
            ),
            (Some("0.512"), Some("2.5"))
        );
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x48, &[CONVERSION], 2);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
