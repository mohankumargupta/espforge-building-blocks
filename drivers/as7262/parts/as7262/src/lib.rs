//! An ams AS7262 six-channel visible light spectral sensor, written as a
//! virtual part.
//!
//! The firmware talks to it the way it would to the real chip: only three
//! physical registers exist, and every other register is virtual. A virtual
//! write is the address with bit 7 set followed by the data byte, both written
//! to the WRITE register; a virtual read is the address with bit 7 clear,
//! followed by one byte read from the READ register once STATUS' `RX_VALID`
//! says it is there.
//!
//! The part keeps six channel values (V, B, G, Y, O, R) and encodes them as
//! the raw data registers at 0x08..0x13. A one-shot conversion (BANK mode 2 or
//! 3) latches them and sets `DATA_RDY`; reading any data register clears it.
//! README.md lists what is left out.
//!
//! `As7262` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The AS7262's only strapped address, 0x49 in 7 bits.
const DEFAULT_ADDR: u32 = 0x49;

/// The three physical registers.
const STATUS_REGISTER: u8 = 0x00;
const WRITE_REGISTER: u8 = 0x01;
const READ_REGISTER: u8 = 0x02;

/// STATUS bit 1: the WRITE register still holds a byte. This part is never
/// busy, so it always reads 0.
const _TX_VALID: u8 = 0x02;
/// STATUS bit 0: a byte is waiting in the READ register.
const RX_VALID: u8 = 0x01;

/// Bit 7 of a virtual address marks a write; clear means a read.
const WRITE_MARKER: u8 = 0x80;

/// Virtual registers this part serves.
const HW_VERSION_HI: u8 = 0x00;
const HW_VERSION_LO: u8 = 0x01;
const FW_VERSION_HI: u8 = 0x02;
const FW_VERSION_LO: u8 = 0x03;
const CONTROL_SETUP: u8 = 0x04;
const INTEGRATION_TIME: u8 = 0x05;
const DEVICE_TEMP: u8 = 0x06;
const LED_CONTROL: u8 = 0x07;
const DATA_START: u8 = 0x08;
const DATA_END: u8 = 0x13;

/// Control_Setup: soft reset, gain in bits 5:4, bank mode in bits 3:2, data
/// ready in bit 1.
const SOFT_RESET: u8 = 0x80;
const MODE_MASK: u8 = 0x0C;
const DATA_READY: u8 = 0x02;

/// The power-on control value: 1x gain, BANK mode `0b10`.
const POWER_ON_CONTROL: u8 = 0b10 << 2;
/// The power-on integration time: 255 * 2.8 ms.
const POWER_ON_INTEGRATION_TIME: u8 = 0xFF;
/// A one-shot conversion is modes 2 and 3.
const FIRST_ONE_SHOT_MODE: u8 = 0b10;
/// The part's case temperature, in whole degrees.
const DEVICE_TEMP_C: u8 = 25;

/// What the sensor sees when nobody has changed it: a white-ish spectrum.
const DEFAULT_CHANNELS: [u16; CHANNELS] = [1200, 2400, 3200, 3100, 2800, 2200];

/// Six channels, two big-endian bytes each.
const CHANNELS: usize = 6;
const CHANNEL_BYTES: usize = CHANNELS * 2;
/// The raw channel values are 16 bits.
const MAX_CHANNEL: f64 = 65_535.0;

struct As7262 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Virtual register 0x04: reset, gain, bank mode and data-ready bits.
    control: u8,
    /// Virtual register 0x05: integration time in units of 2.8 ms.
    integration_time: u8,
    /// Virtual register 0x07: the LED driver's current limit and enables.
    led: u8,
    /// What the sensor sees, in V, B, G, Y, O, R order.
    channels: [u16; CHANNELS],
    /// The last conversion's raw bytes, which the data registers hold.
    latched: [u8; CHANNEL_BYTES],
    /// A virtual write's address, waiting for its data byte.
    write_address: Option<u8>,
    /// A virtual read's address, whose byte the next READ takes.
    read_address: Option<u8>,
}

impl Default for As7262 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            control: POWER_ON_CONTROL,
            integration_time: POWER_ON_INTEGRATION_TIME,
            led: 0,
            channels: DEFAULT_CHANNELS,
            latched: [0; CHANNEL_BYTES],
            write_address: None,
            read_address: None,
        }
    }
}

impl Device for As7262 {
    const NAME: &'static str = "as7262";
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

    /// Change what the sensor sees. `set:violet=<raw>` and the other five
    /// channel names set one at a time; `dark` zeroes all six and `rest` puts
    /// the defaults back.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            let channel = match what {
                "violet" => Some(0),
                "blue" => Some(1),
                "green" => Some(2),
                "yellow" => Some(3),
                "orange" => Some(4),
                "red" => Some(5),
                _ => None,
            };
            if let Some(index) = channel {
                self.channels[index] = value.clamp(0.0, MAX_CHANNEL) as u16;
            }
        } else if name == "dark" {
            self.channels = [0; CHANNELS];
        } else if name == "rest" {
            self.channels = DEFAULT_CHANNELS;
        }
    }

    /// The guest's I2C transaction. A write is either a virtual address or the
    /// data byte that follows it; a read answers whichever physical register
    /// was addressed.
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
        let physical = i2c.write.first().copied();
        if physical == Some(WRITE_REGISTER) {
            for &byte in &i2c.write[1..] {
                self.write_byte(byte);
            }
        }
        if i2c.read_len > 0 {
            let byte = match physical {
                Some(STATUS_REGISTER) => {
                    if self.read_address.is_some() {
                        RX_VALID
                    } else {
                        0
                    }
                }
                Some(READ_REGISTER) => self.take_read_byte(),
                _ => 0xFF,
            };
            let mut data = vec![0xFF; i2c.read_len as usize];
            if let Some(first) = data.first_mut() {
                *first = byte;
            }
            reply.data = data;
        }
        // A probe, a write on its own, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("violet", f64::from(self.channels[0]))
                .set("blue", f64::from(self.channels[1]))
                .set("green", f64::from(self.channels[2]))
                .set("yellow", f64::from(self.channels[3]))
                .set("orange", f64::from(self.channels[4]))
                .set("red", f64::from(self.channels[5])),
        )
    }
}

impl As7262 {
    /// One byte written to the WRITE register: a virtual address, then its
    /// data, or a virtual address for the next read.
    fn write_byte(&mut self, byte: u8) {
        if let Some(address) = self.write_address.take() {
            self.write_register(address, byte);
        } else if byte & WRITE_MARKER != 0 {
            self.write_address = Some(byte & !WRITE_MARKER);
        } else {
            self.read_address = Some(byte & !WRITE_MARKER);
        }
    }

    /// The byte a virtual read of `read_address` answers, consuming the
    /// address. Reading a data register clears `DATA_RDY`, as the datasheet
    /// says.
    fn take_read_byte(&mut self) -> u8 {
        let Some(address) = self.read_address.take() else {
            return 0xFF;
        };
        let byte = self.register_byte(address);
        if (DATA_START..=DATA_END).contains(&address) {
            self.control &= !DATA_READY;
        }
        byte
    }

    /// One virtual register's byte.
    fn register_byte(&self, address: u8) -> u8 {
        match address {
            HW_VERSION_HI => 0x00,
            HW_VERSION_LO => 0x01,
            FW_VERSION_HI => 0x00,
            FW_VERSION_LO => 0x21,
            CONTROL_SETUP => self.control,
            INTEGRATION_TIME => self.integration_time,
            DEVICE_TEMP => DEVICE_TEMP_C,
            LED_CONTROL => self.led,
            DATA_START..=DATA_END => self.latched[(address - DATA_START) as usize],
            _ => 0x00,
        }
    }

    /// One virtual register write: reset, the control bits, the integration
    /// time or the LED control.
    fn write_register(&mut self, address: u8, value: u8) {
        match address {
            CONTROL_SETUP => {
                if value & SOFT_RESET != 0 {
                    // The reset restores the power-on registers and drops the
                    // previous conversion; what the sensor sees stays.
                    self.control = POWER_ON_CONTROL;
                    self.integration_time = POWER_ON_INTEGRATION_TIME;
                    self.led = 0;
                    self.latched = [0; CHANNEL_BYTES];
                } else {
                    self.control = value & !DATA_READY;
                    // A one-shot conversion latches the channels at once; the
                    // part model has no conversion time.
                    if (self.control & MODE_MASK) >> 2 >= FIRST_ONE_SHOT_MODE {
                        self.latch();
                        self.control |= DATA_READY;
                    }
                }
            }
            INTEGRATION_TIME => self.integration_time = value,
            LED_CONTROL => self.led = value,
            _ => {}
        }
    }

    /// Copy the channels into the raw data registers, big-endian.
    fn latch(&mut self) {
        for (index, channel) in self.channels.iter().enumerate() {
            self.latched[index * 2] = (channel >> 8) as u8;
            self.latched[index * 2 + 1] = *channel as u8;
        }
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(As7262, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the part's fixed address.
    fn chip() -> As7262 {
        let mut chip = As7262::default();
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
    fn transfer(chip: &mut As7262, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// One virtual register byte the way the driver reads it: address it, read
    /// STATUS, then read READ.
    fn read_register(chip: &mut As7262, register: u8) -> u8 {
        assert_eq!(transfer(chip, &[WRITE_REGISTER, register], 0), Some(vec![]));
        let status = transfer(chip, &[STATUS_REGISTER], 1).expect("a status byte")[0];
        assert_eq!(status & RX_VALID, RX_VALID, "the answer is waiting");
        transfer(chip, &[READ_REGISTER], 1).expect("a register byte")[0]
    }

    /// One virtual register write: the address with bit 7 set, then the data.
    fn write_register(chip: &mut As7262, register: u8, value: u8) {
        assert_eq!(
            transfer(chip, &[WRITE_REGISTER, register | WRITE_MARKER], 0),
            Some(vec![])
        );
        assert_eq!(transfer(chip, &[WRITE_REGISTER, value], 0), Some(vec![]));
    }

    /// One one-shot measurement, the way the driver takes it.
    fn measurement(chip: &mut As7262) -> [u16; CHANNELS] {
        write_register(chip, CONTROL_SETUP, 0x30 | (0b11 << 2));
        assert_eq!(
            read_register(chip, CONTROL_SETUP) & DATA_READY,
            DATA_READY,
            "the conversion is ready at once"
        );
        let mut channels = [0u16; CHANNELS];
        for (index, channel) in channels.iter_mut().enumerate() {
            let address = DATA_START + (index * 2) as u8;
            let high = read_register(chip, address);
            let low = read_register(chip, address + 1);
            *channel = u16::from_be_bytes([high, low]);
        }
        channels
    }

    #[test]
    fn the_power_on_part_reports_its_versions() {
        let mut chip = chip();
        assert_eq!(read_register(&mut chip, HW_VERSION_LO), 0x01);
        assert_eq!(read_register(&mut chip, FW_VERSION_LO), 0x21);
        assert_eq!(read_register(&mut chip, DEVICE_TEMP), 25);
        assert_eq!(
            chip.control & 0x3C,
            POWER_ON_CONTROL,
            "1x gain and BANK mode 2"
        );
    }

    #[test]
    fn a_soft_reset_clears_the_data_ready_bit() {
        let mut chip = chip();
        write_register(&mut chip, CONTROL_SETUP, 0x30 | (0b11 << 2));
        assert_eq!(chip.control & DATA_READY, DATA_READY);
        write_register(&mut chip, CONTROL_SETUP, SOFT_RESET);
        assert_eq!(chip.control, POWER_ON_CONTROL);
        assert_eq!(chip.integration_time, POWER_ON_INTEGRATION_TIME);
        assert_eq!(chip.led, 0);
    }

    #[test]
    fn the_default_spectrum_is_the_ramp() {
        let mut chip = chip();
        assert_eq!(
            measurement(&mut chip),
            [1200, 2400, 3200, 3100, 2800, 2200]
        );
    }

    #[test]
    fn the_channels_are_big_endian() {
        let mut chip = chip();
        chip.on_input("set:violet=4660"); // 0x1234
        write_register(&mut chip, CONTROL_SETUP, 0b11 << 2);
        assert_eq!(read_register(&mut chip, DATA_START), 0x12);
        assert_eq!(read_register(&mut chip, DATA_START + 1), 0x34);
    }

    #[test]
    fn reading_a_data_register_clears_data_ready() {
        let mut chip = chip();
        let _ = measurement(&mut chip);
        assert_eq!(
            chip.control & DATA_READY,
            0,
            "the driver read all six channels"
        );
    }

    #[test]
    fn a_measurement_survives_the_input_moving_after_it() {
        let mut chip = chip();
        write_register(&mut chip, CONTROL_SETUP, 0b11 << 2);
        chip.on_input("set:red=1");
        assert_eq!(read_register(&mut chip, DATA_START + 10), 0x08);
        assert_eq!(read_register(&mut chip, DATA_START + 11), 0x98);
    }

    #[test]
    fn the_inputs_set_and_restore_the_channels() {
        let mut chip = chip();
        chip.on_input("set:red=999");
        assert_eq!(measurement(&mut chip)[5], 999);

        chip.on_input("dark");
        assert_eq!(measurement(&mut chip), [0; CHANNELS]);

        chip.on_input("rest");
        assert_eq!(
            measurement(&mut chip),
            [1200, 2400, 3200, 3100, 2800, 2200]
        );
    }

    #[test]
    fn a_write_address_takes_the_next_byte_as_its_data() {
        let mut chip = chip();
        // 0xFF is both a data byte and an address with bit 7 set; after an
        // address it must be data.
        write_register(&mut chip, INTEGRATION_TIME, 0xFF);
        assert_eq!(chip.integration_time, 0xFF);
        assert_eq!(read_register(&mut chip, INTEGRATION_TIME), 0xFF);
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x49,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_channels() {
        let mut chip = chip();
        chip.on_input("set:green=42");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("violet"), shown.get("green"), shown.get("red")),
            (Some("1200"), Some("42"), Some("2200"))
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip
            .on_bus(&request(0x48, &[STATUS_REGISTER], 1))
            .unwrap();
        assert!(!reply.ack, "nothing answers at 0x48");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_status_byte_reports_a_waiting_answer() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[STATUS_REGISTER], 1), Some(vec![0]));
        assert_eq!(
            transfer(&mut chip, &[WRITE_REGISTER, CONTROL_SETUP], 0),
            Some(vec![])
        );
        assert_eq!(
            transfer(&mut chip, &[STATUS_REGISTER], 1),
            Some(vec![RX_VALID])
        );
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x49, &[STATUS_REGISTER], 1);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
