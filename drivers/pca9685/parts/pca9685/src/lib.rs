//! An NXP PCA9685 16-channel PWM controller, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: the part is
//! register-based, and one transaction is a register index followed by the
//! bytes written to it:
//!
//! - `0x00` is MODE1: the sleep bit (`0x10`) and the auto-increment bit
//!   (`0x20`). With auto-increment set a multi-byte write walks the registers;
//!   with it clear, every byte lands in the same one,
//! - `0x06` starts the channel registers, four per channel: on low, on high,
//!   off low, off high. The off high byte's bit 4 is full on,
//! - `0xFE` is the prescaler.
//!
//! The register file is the part's whole state: what it drives follows from
//! it. A read answers from the register after the last written one, since the
//! driver only ever writes.
//!
//! README.md lists what is left out.
//!
//! `Pca9685` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with every address pin low.
const DEFAULT_ADDR: u32 = 0x40;

/// The mode register: sleep and auto-increment live here.
const MODE1: u8 = 0x00;
/// The first of the channel registers; each channel is four bytes after it.
const LED0_ON_L: u8 = 0x06;
/// The prescaler.
const PRESCALE: u8 = 0xFE;

/// The auto-increment bit of MODE1.
const AUTO_INCREMENT: u8 = 0x20;
/// The off high byte's bit 4: the channel is fully on whatever the counts say.
const FULL_ON: u8 = 0x10;

/// The part's power-on register values: asleep, all call enabled, 200 Hz.
const POWER_ON_MODE1: u8 = 0x11;
const POWER_ON_MODE2: u8 = 0x04;
const POWER_ON_PRESCALE: u8 = 0x1E;

/// The part's internal oscillator, for the frequency it reports.
const OSCILLATOR_HZ: f64 = 25_000_000.0;
/// 12-bit duty resolution.
const STEPS: f64 = 4096.0;
/// How many channels the part drives.
const CHANNELS: u8 = 16;

struct Pca9685 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The register file: MODE1, MODE2, the channel registers, the prescaler.
    regs: [u8; 0x100],
    /// Where the next read starts: the register the last write addressed.
    pointer: u8,
}

impl Default for Pca9685 {
    fn default() -> Self {
        let mut regs = [0u8; 0x100];
        regs[MODE1 as usize] = POWER_ON_MODE1;
        regs[0x01] = POWER_ON_MODE2;
        regs[PRESCALE as usize] = POWER_ON_PRESCALE;
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            regs,
            pointer: MODE1,
        }
    }
}

impl Device for Pca9685 {
    const NAME: &'static str = "pca9685";
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

    /// `rest` puts the register file back to its power-on values. Nothing else
    /// is an input: the controller is driven by the firmware, not by the
    /// world.
    fn on_input(&mut self, name: &str) {
        if name == "rest" {
            let (scl, sda, addr) = (self.scl, self.sda, self.addr);
            *self = Self {
                scl,
                sda,
                addr,
                ..Self::default()
            };
        }
    }

    /// The guest's I2C transaction: a register index, then the bytes for it.
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
        if let Some((&register, data)) = i2c.write.split_first() {
            let auto_increment = self.regs[MODE1 as usize] & AUTO_INCREMENT != 0;
            let mut index = register;
            for byte in data {
                self.regs[index as usize] = *byte;
                if auto_increment {
                    index = index.wrapping_add(1);
                }
            }
            self.pointer = register;
        }
        if i2c.read_len > 0 {
            reply.data = (0..i2c.read_len)
                .map(|_| {
                    let byte = self.regs[self.pointer as usize];
                    self.pointer = self.pointer.wrapping_add(1);
                    byte
                })
                .collect();
        }
        // A probe, a register write, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        let mut state = State::new()
            .set("mode1", self.regs[MODE1 as usize])
            .set("prescale", self.regs[PRESCALE as usize])
            .set("frequency", self.frequency_hz());
        for channel in 0..CHANNELS {
            state = state.set(&format!("ch{channel}"), self.duty_percent(channel));
        }
        Some(state)
    }
}

impl Pca9685 {
    /// The frame rate the prescaler asks for.
    fn frequency_hz(&self) -> f64 {
        OSCILLATOR_HZ / (STEPS * (f64::from(self.regs[PRESCALE as usize]) + 1.0))
    }

    /// The channel's four duty registers: on low, on high, off low, off high.
    fn channel_registers(&self, channel: u8) -> [u8; 4] {
        let first = (LED0_ON_L + 4 * channel) as usize;
        [
            self.regs[first],
            self.regs[first + 1],
            self.regs[first + 2],
            self.regs[first + 3],
        ]
    }

    /// One channel's duty cycle in percent: full on, or its off count of the
    /// 4096 steps.
    fn duty_percent(&self, channel: u8) -> f64 {
        let registers = self.channel_registers(channel);
        if registers[3] & FULL_ON != 0 {
            return 100.0;
        }
        let off = u16::from(registers[2]) | (u16::from(registers[3] & 0x0F) << 8);
        f64::from(off) / STEPS * 100.0
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Pca9685, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the default 0x40 address.
    fn chip() -> Pca9685 {
        let mut chip = Pca9685::default();
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
    fn transfer(chip: &mut Pca9685, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// The three writes the driver's constructor sends.
    fn initialise(chip: &mut Pca9685) {
        assert_eq!(transfer(chip, &[MODE1, SLEEP], 0), Some(vec![]));
        assert_eq!(transfer(chip, &[PRESCALE, 0x79], 0), Some(vec![]));
        assert_eq!(transfer(chip, &[MODE1, AUTO_INCREMENT], 0), Some(vec![]));
    }

    #[test]
    fn a_fresh_part_is_asleep_and_all_call() {
        let chip = chip();
        assert_eq!((chip.regs[0], chip.regs[1], chip.regs[0xFE]), (0x11, 0x04, 0x1E));
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("mode1"), shown.get("prescale")),
            (Some("17"), Some("30"))
        );
    }

    #[test]
    fn the_initialisation_programs_the_frame_rate() {
        let mut chip = chip();
        initialise(&mut chip);
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("mode1"), shown.get("prescale")),
            (Some("32"), Some("121")),
            "awake with auto-increment, 50 Hz"
        );
        assert!(chip.frequency_hz() > 49.0 && chip.frequency_hz() < 51.0);
    }

    #[test]
    fn a_channel_write_sets_its_duty() {
        let mut chip = chip();
        initialise(&mut chip);
        // Channel 0 at 25 %: on 0, off 1024.
        assert_eq!(
            transfer(&mut chip, &[LED0_ON_L, 0x00, 0x00, 0x00, 0x04], 0),
            Some(vec![])
        );
        assert_eq!(chip.duty_percent(0), 25.0);
        assert_eq!(chip.state().unwrap().get("ch0"), Some("25"));
        assert_eq!(chip.state().unwrap().get("ch1"), Some("0"));
    }

    #[test]
    fn the_full_on_bit_wins_over_the_counts() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(
            transfer(&mut chip, &[LED0_ON_L, 0x00, 0x00, 0xFF, 0x1F], 0),
            Some(vec![])
        );
        assert_eq!(
            chip.duty_percent(0),
            100.0,
            "off high bit 4 means the channel never turns off"
        );
    }

    #[test]
    fn every_channel_has_its_own_four_registers() {
        let mut chip = chip();
        initialise(&mut chip);
        // Channel 15 at half: on 0, off 2048.
        assert_eq!(
            transfer(&mut chip, &[LED0_ON_L + 60, 0x00, 0x00, 0x00, 0x08], 0),
            Some(vec![])
        );
        assert_eq!(chip.duty_percent(15), 50.0);
        assert_eq!(chip.state().unwrap().get("ch0"), Some("0"));
    }

    #[test]
    fn without_auto_increment_every_byte_lands_in_one_register() {
        let mut chip = chip();
        // MODE1 is still 0x11, so the auto-increment bit is clear.
        assert_eq!(
            transfer(&mut chip, &[0x01, 0x01, 0x02, 0x03], 0),
            Some(vec![])
        );
        assert_eq!(chip.regs[0x01], 0x03, "the last byte wins");
        assert_eq!(chip.regs[0x02], 0x00, "the next register is untouched");
    }

    #[test]
    fn a_read_answers_from_the_register_file() {
        let mut chip = chip();
        initialise(&mut chip);
        assert_eq!(
            transfer(&mut chip, &[PRESCALE], 1),
            Some(vec![0x79]),
            "the prescaler reads back"
        );
        // The next write moves the pointer, and the read follows it.
        assert_eq!(transfer(&mut chip, &[MODE1], 1), Some(vec![0x20]));
    }

    #[test]
    fn rest_puts_the_power_on_values_back() {
        let mut chip = chip();
        initialise(&mut chip);
        let _ = transfer(&mut chip, &[LED0_ON_L, 0x00, 0x00, 0x00, 0x04], 0);
        chip.on_input("rest");
        assert_eq!(
            (chip.regs[0], chip.regs[0xFE], chip.duty_percent(0)),
            (0x11, 0x1E, 0.0)
        );
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x40,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x40, &[MODE1, SLEEP], 0);
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
        let reply = chip.on_bus(&request(0x41, &[MODE1, SLEEP], 0)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x41");
        assert!(reply.data.is_empty());
    }
}
