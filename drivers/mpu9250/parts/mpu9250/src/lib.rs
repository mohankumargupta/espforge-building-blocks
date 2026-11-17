//! An InvenSense MPU-9250 with its accelerometer and gyro die, written as a
//! virtual part.
//!
//! The firmware talks to it over SPI the way it would the real chip: one
//! address byte per transfer, bit 7 set for a read and bits 6:0 the register,
//! and **no dummy byte**. The address increments across a read while CS stays
//! low, which is how the driver takes all fourteen measurement bytes in one
//! transaction. `WHO_AM_I` (0x75) answers 0x71.
//!
//! The part keeps the physical values — `accel_g`, `gyro_dps`,
//! `temperature_c` — and encodes them into the big-endian 16-bit pairs of the
//! data block as each byte is read, so an interaction is visible in the very
//! next reading. The scales are the power-on ones: 16384 LSB/g (±2 g),
//! 131 LSB/dps (±250 dps) and 333.87 LSB/°C about 21 °C. `PWR_MGMT_1` powers up
//! asleep (0x40) and a write with bit 7 set is a device reset that clears
//! itself. README.md in the fixture folder lists what is left out.
//!
//! `Mpu9250` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its pin table). The board says which chip pins
/// they are wired to, as `{ pins: { CS: 7, SCK: 6, MOSI: 3, MISO: 2 } }`.
const CS: &str = "CS";
const SCK: &str = "SCK";
const MOSI: &str = "MOSI";
const MISO: &str = "MISO";

/// Read bit: set for a read, clear for a write. Bits 6:0 of the wire address
/// are the register itself, so there is no shift.
const READ: u8 = 0x80;

/// The registers this part answers specially, by datasheet address. The
/// configuration registers are plain storage, so they need no constant here;
/// the tests name them.
const ACCEL_XOUT_H: usize = 0x3B;
const GYRO_ZOUT_L: usize = 0x48;
const PWR_MGMT_1: usize = 0x6B;
const WHO_AM_I: usize = 0x75;

/// `WHO_AM_I` on an MPU-9250 (0x70 is an MPU-6500, 0x73 an MPU-9255).
const WHO_AM_I_VALUE: u8 = 0x71;

/// `PWR_MGMT_1` powers up with `SLEEP` set and the internal 20 MHz clock.
const PWR_MGMT_1_POWER_ON: u8 = 0x40;
/// `PWR_MGMT_1`'s `DEVICE_RESET`: set by a write, cleared by the part.
const DEVICE_RESET: u8 = 0x80;

/// The power-on scales, which the part always encodes with.
const ACCEL_LSB_PER_G: f32 = 16384.0;
const GYRO_LSB_PER_DPS: f32 = 131.0;
const TEMP_LSB_PER_C: f32 = 333.87;
const TEMP_OFFSET_C: f32 = 21.0;

/// The power-on ranges, which the inputs clamp to.
const ACCEL_LIMIT_G: f32 = 2.0;
const GYRO_LIMIT_DPS: f32 = 250.0;
const TEMP_MIN_C: f32 = -40.0;
const TEMP_MAX_C: f32 = 85.0;
/// A 16-bit count cannot hold 2 g at 16384 LSB/g; the part saturates instead of
/// wrapping.
const COUNTS_LIMIT: f32 = 32767.0;

struct Mpu9250 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    cs: Option<u32>,
    sck: Option<u32>,
    mosi: Option<u32>,
    miso: Option<u32>,
    /// Registers 0x00..=0x7F. `who_am_i` and the data block are answered from
    /// the fields below; every other register is plain storage.
    regs: [u8; 128],
    /// The last wire address byte, for a read the emulator delivers as its own
    /// transfer with the address already gone by.
    command: u8,
    /// An address phase arrived and its answer phase is still to come.
    pending: bool,

    /// What the die measures, in physical units.
    accel_g: [f32; 3],
    gyro_dps: [f32; 3],
    temperature_c: f32,
}

impl Default for Mpu9250 {
    fn default() -> Self {
        Self {
            cs: None,
            sck: None,
            mosi: None,
            miso: None,
            regs: power_on_registers(),
            command: 0,
            pending: false,
            accel_g: [0.0, 0.0, 1.0], // flat on the table: 1 g on Z
            gyro_dps: [0.0, 0.0, 0.0], // still
            temperature_c: TEMP_OFFSET_C,
        }
    }
}

impl Device for Mpu9250 {
    const NAME: &'static str = "mpu9250";
    const ON_BUS: bool = true;

    /// Where the board wired the part.
    fn configure(&mut self, config: &PartConfig) {
        self.cs = config.pins.get(CS).copied();
        self.sck = config.pins.get(SCK).copied();
        self.mosi = config.pins.get(MOSI).copied();
        self.miso = config.pins.get(MISO).copied();
    }

    /// The chip select this part is selected by, plus the clock and data pads.
    /// espforge drives CS as a plain GPIO, so the pad carries no controller
    /// chip select; the emulator takes the bus from whichever of `SCK`,
    /// `MOSI` and `MISO` the firmware put a bus line on.
    fn claims(&self) -> Vec<Claim> {
        let spi = SpiClaim {
            cs_pin: self.cs,
            clk_pin: self.sck,
            mosi_pin: self.mosi,
            miso_pin: self.miso,
            ..Default::default()
        };
        vec![Claim { what: Some(claim::What::Spi(spi)) }]
    }

    /// Move the die. `set:ax=<g>`, `set:ay=<g>`, `set:az=<g>` move one
    /// accelerometer axis (clamped to ±2 g), `set:gx=<dps>`, `set:gy=<dps>`,
    /// `set:gz=<dps>` turn one gyro axis (clamped to ±250 dps),
    /// `set:temperature=<°C>` sets the die temperature (clamped to -40..85 °C)
    /// and `rest` puts it back flat, still and at 21 °C.
    fn on_input(&mut self, name: &str) {
        if name == "rest" {
            self.accel_g = [0.0, 0.0, 1.0];
            self.gyro_dps = [0.0, 0.0, 0.0];
            self.temperature_c = TEMP_OFFSET_C;
            return;
        }
        let Some((what, value)) = name.split_once('=') else { return };
        let Ok(value) = value.parse::<f32>() else { return };
        // `NaN` parses, but is not a measurement.
        if !value.is_finite() {
            return;
        }
        match what {
            "set:ax" => self.accel_g[0] = value.clamp(-ACCEL_LIMIT_G, ACCEL_LIMIT_G),
            "set:ay" => self.accel_g[1] = value.clamp(-ACCEL_LIMIT_G, ACCEL_LIMIT_G),
            "set:az" => self.accel_g[2] = value.clamp(-ACCEL_LIMIT_G, ACCEL_LIMIT_G),
            "set:gx" => self.gyro_dps[0] = value.clamp(-GYRO_LIMIT_DPS, GYRO_LIMIT_DPS),
            "set:gy" => self.gyro_dps[1] = value.clamp(-GYRO_LIMIT_DPS, GYRO_LIMIT_DPS),
            "set:gz" => self.gyro_dps[2] = value.clamp(-GYRO_LIMIT_DPS, GYRO_LIMIT_DPS),
            "set:temperature" => self.temperature_c = value.clamp(TEMP_MIN_C, TEMP_MAX_C),
            _ => {} // Not an interaction this part has.
        }
    }

    /// The guest's SPI transfer. A write stores a register; a read answers
    /// from the address byte, incrementing the register for every byte, so one
    /// transfer can take the whole data block.
    fn on_bus(&mut self, request: &BusRequest) -> Option<BusReply> {
        let Some(bus_request::Op::Spi(spi)) = &request.op else { return None };
        let reply = |data: Vec<u8>| BusReply {
            correlation_id: request.correlation_id,
            ack: true,
            data,
            op: BusOp::Spi as i32,
        };
        // A transfer that reads nothing is a write: the address byte, then the
        // value.
        if spi.rx_len == 0 {
            if let Some(&first) = spi.tx.first() {
                if first & READ != 0 {
                    // A command-only read: its answer phase is still to come.
                    self.command = first;
                    self.pending = true;
                } else if let Some(&value) = spi.tx.get(1) {
                    self.pending = false;
                    self.write((first & 0x7F) as usize, value);
                }
            }
            return None;
        }
        if let Some(&first) = spi.tx.first() {
            if first & READ != 0 {
                // The address. A transfer longer than its own write carries the
                // answer with it; a full-duplex address phase is answered with
                // bytes the master clocks in and ignores, and the answer phase
                // follows.
                self.command = first;
                if spi.rx_len as usize > spi.tx.len() {
                    self.pending = false;
                    let data = self.serve((first & 0x7F) as usize, spi.rx_len);
                    return Some(reply(data));
                }
                self.pending = true;
                return Some(reply(vec![0u8; spi.rx_len as usize]));
            }
            // The answer phase after an address that came in its own transfer
            // carries the controller's buffer, which is no command here.
            if self.pending {
                self.pending = false;
                let data = self.serve((self.command & 0x7F) as usize, spi.rx_len);
                return Some(reply(data));
            }
            if let Some(&value) = spi.tx.get(1) {
                // A full-duplex write: the frame is the address and the value,
                // and the read half is what the master ignores.
                self.write((first & 0x7F) as usize, value);
                return Some(reply(vec![0u8; spi.rx_len as usize]));
            }
        }
        // A read with no address of its own, answered from the last one.
        self.pending = false;
        let data = self.serve((self.command & 0x7F) as usize, spi.rx_len);
        Some(reply(data))
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("ax", format!("{:.2}", self.accel_g[0]))
                .set("ay", format!("{:.2}", self.accel_g[1]))
                .set("az", format!("{:.2}", self.accel_g[2]))
                .set("gx", format!("{:.2}", self.gyro_dps[0]))
                .set("gy", format!("{:.2}", self.gyro_dps[1]))
                .set("gz", format!("{:.2}", self.gyro_dps[2]))
                .set("temperature", format!("{:.2}", self.temperature_c)),
        )
    }
}

impl Mpu9250 {
    /// Register `register` as one byte of a read answers it. The data block is
    /// encoded here, from the physical values, so an interaction shows up in
    /// the next reading.
    fn read(&mut self, register: usize) -> u8 {
        match register & 0x7F {
            WHO_AM_I => WHO_AM_I_VALUE,
            register @ ACCEL_XOUT_H..=GYRO_ZOUT_L => {
                self.data_block()[register - ACCEL_XOUT_H]
            }
            register => self.regs[register],
        }
    }

    /// A register write. `PWR_MGMT_1` with the reset bit set clears the
    /// register file and reads back asleep; `WHO_AM_I` and the data block are
    /// read-only.
    fn write(&mut self, register: usize, value: u8) {
        match register & 0x7F {
            PWR_MGMT_1 => {
                if value & DEVICE_RESET != 0 {
                    // The reset clears itself: the register reads back `SLEEP`.
                    self.regs = power_on_registers();
                    self.pending = false;
                } else {
                    self.regs[PWR_MGMT_1] = value;
                }
            }
            WHO_AM_I | ACCEL_XOUT_H..=GYRO_ZOUT_L => {}
            register => self.regs[register] = value,
        }
    }

    /// `len` registers from `start`, which is one read: the address increments
    /// across the answer.
    fn serve(&mut self, start: usize, len: u32) -> Vec<u8> {
        (0..len as usize).map(|i| self.read(start + i)).collect()
    }

    /// The fourteen bytes from `ACCEL_XOUT_H`: acceleration, temperature and
    /// rotation, each as a big-endian 16-bit pair.
    fn data_block(&self) -> [u8; 14] {
        let mut block = [0u8; 14];
        for (axis, g) in self.accel_g.iter().enumerate() {
            block[axis * 2..axis * 2 + 2].copy_from_slice(&accel_counts(*g).to_be_bytes());
        }
        block[6..8].copy_from_slice(&temperature_counts(self.temperature_c).to_be_bytes());
        for (axis, dps) in self.gyro_dps.iter().enumerate() {
            block[8 + axis * 2..8 + axis * 2 + 2].copy_from_slice(&gyro_counts(*dps).to_be_bytes());
        }
        block
    }
}

/// The power-on register file: asleep on the internal clock, everything else 0.
fn power_on_registers() -> [u8; 128] {
    let mut regs = [0u8; 128];
    regs[PWR_MGMT_1] = PWR_MGMT_1_POWER_ON;
    regs
}

/// Acceleration in counts at the power-on ±2 g range.
fn accel_counts(g: f32) -> i16 {
    (g * ACCEL_LSB_PER_G)
        .round()
        .clamp(-COUNTS_LIMIT, COUNTS_LIMIT) as i16
}

/// Rotation in counts at the power-on ±250 dps range.
fn gyro_counts(dps: f32) -> i16 {
    (dps * GYRO_LSB_PER_DPS)
        .round()
        .clamp(-COUNTS_LIMIT, COUNTS_LIMIT) as i16
}

/// Die temperature in counts, zero at 21 °C.
fn temperature_counts(celsius: f32) -> i16 {
    ((celsius - TEMP_OFFSET_C) * TEMP_LSB_PER_C)
        .round()
        .clamp(-COUNTS_LIMIT, COUNTS_LIMIT) as i16
}

// Big enough for a read of the register file and the frame a host hands over.
esp_part::export_part!(Mpu9250, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// The configuration registers, which the part stores and powers up at 0.
    const SMPLRT_DIV: usize = 0x19;
    const CONFIG: usize = 0x1A;
    const GYRO_CONFIG: usize = 0x1B;
    const ACCEL_CONFIG: usize = 0x1C;
    const ACCEL_CONFIG2: usize = 0x1D;

    /// Wired as the fixture's board wires it: CS on GPIO 7, SCK on 6, MOSI on
    /// 3, MISO on 2.
    fn chip() -> Mpu9250 {
        let mut chip = Mpu9250::default();
        let pins = [
            (CS.to_string(), 7),
            (SCK.to_string(), 6),
            (MOSI.to_string(), 3),
            (MISO.to_string(), 2),
        ]
        .into();
        chip.configure(&PartConfig { pins, ..Default::default() });
        chip
    }

    fn request(tx: &[u8], rx_len: u32) -> BusRequest {
        let spi = SpiTransfer { bus: 2, tx: tx.to_vec(), rx_len, ..Default::default() };
        BusRequest { correlation_id: 7, op: Some(bus_request::Op::Spi(spi)) }
    }

    /// A register write: the address byte, then the value, and nothing read.
    fn write(chip: &mut Mpu9250, register: usize, value: u8) {
        assert_eq!(
            chip.on_bus(&request(&[register as u8, value], 0)),
            None,
            "a write needs no answer"
        );
    }

    /// A register read, in the two transfers the runtime makes of it: the
    /// address on its own, then an answer phase whose `tx` is the controller's
    /// buffer.
    fn read(chip: &mut Mpu9250, register: usize, rx_len: usize) -> Vec<u8> {
        let command = (register as u8) | READ;
        let address = chip
            .on_bus(&request(&[command], 1))
            .expect("the address phase is answered");
        assert_eq!(address.data.len(), 1, "one byte the master ignores");
        let zeros = vec![0u8; rx_len];
        chip.on_bus(&request(&zeros, rx_len as u32))
            .expect("the answer phase is answered")
            .data
    }

    /// The fourteen bytes from `ACCEL_XOUT_H`, which is what the driver reads.
    fn data(chip: &mut Mpu9250) -> Vec<u8> {
        read(chip, ACCEL_XOUT_H, 14)
    }

    #[test]
    fn the_identity_is_answered_before_any_command() {
        // The wire address of 0x75 is 0xF5: the register with the read bit.
        let mut chip = chip();
        assert_eq!(read(&mut chip, WHO_AM_I, 1), [0x71]);
        assert_eq!((WHO_AM_I as u8) | READ, 0xF5);
    }

    #[test]
    fn the_power_on_register_values_are_the_datasheets() {
        let mut chip = chip();
        assert_eq!(read(&mut chip, SMPLRT_DIV, 1), [0x00]);
        assert_eq!(read(&mut chip, CONFIG, 1), [0x00]);
        assert_eq!(read(&mut chip, GYRO_CONFIG, 1), [0x00], "±250 dps");
        assert_eq!(read(&mut chip, ACCEL_CONFIG, 1), [0x00], "±2 g");
        assert_eq!(read(&mut chip, ACCEL_CONFIG2, 1), [0x00]);
        assert_eq!(read(&mut chip, PWR_MGMT_1, 1), [0x40], "asleep, internal clock");
    }

    #[test]
    fn the_data_block_is_fourteen_big_endian_bytes() {
        let mut chip = chip();
        assert_eq!(
            data(&mut chip),
            [
                0x00, 0x00, // ax = 0 g
                0x00, 0x00, // ay = 0 g
                0x40, 0x00, // az = 1 g = 16384
                0x00, 0x00, // 21 °C
                0x00, 0x00, // gx = 0 dps
                0x00, 0x00, // gy = 0 dps
                0x00, 0x00, // gz = 0 dps
            ],
            "flat, still and at 21 °C"
        );
    }

    #[test]
    fn the_pairs_are_big_endian() {
        let mut chip = chip();
        chip.on_input("set:ax=0.5"); // 8192 = 0x2000
        chip.on_input("set:ay=-1"); // -16384 = 0xC000
        chip.on_input("set:gz=90"); // 11790 = 0x2E0E
        let block = data(&mut chip);
        assert_eq!(&block[0..2], [0x20, 0x00], "0.5 g, high byte first");
        assert_eq!(&block[2..4], [0xC0, 0x00], "-1 g");
        assert_eq!(&block[12..14], [0x2E, 0x0E], "90 dps");
    }

    #[test]
    fn the_reset_write_clears_the_register_file_and_self_clears() {
        let mut chip = chip();
        write(&mut chip, CONFIG, 0x03);
        write(&mut chip, ACCEL_CONFIG, 0x18);
        write(&mut chip, PWR_MGMT_1, DEVICE_RESET);
        assert_eq!(read(&mut chip, CONFIG, 1), [0x00], "the reset cleared it");
        assert_eq!(read(&mut chip, ACCEL_CONFIG, 1), [0x00], "the reset cleared it");
        assert_eq!(
            read(&mut chip, PWR_MGMT_1, 1),
            [0x40],
            "the reset bit is gone"
        );
        write(&mut chip, PWR_MGMT_1, 0x01);
        assert_eq!(read(&mut chip, PWR_MGMT_1, 1), [0x01], "awake on the X-gyro PLL");
    }

    #[test]
    fn the_data_registers_are_read_only() {
        let mut chip = chip();
        write(&mut chip, ACCEL_XOUT_H, 0xFF);
        write(&mut chip, GYRO_ZOUT_L, 0xFF);
        write(&mut chip, WHO_AM_I, 0x00);
        assert_eq!(data(&mut chip)[4], 0x40, "az is still 1 g");
        assert_eq!(read(&mut chip, WHO_AM_I, 1), [0x71]);
    }

    #[test]
    fn the_inputs_move_the_axes_and_the_temperature() {
        let mut chip = chip();
        chip.on_input("set:az=0");
        chip.on_input("set:ay=1");
        chip.on_input("set:gx=-10"); // -1310 = 0xFAE2
        chip.on_input("set:temperature=25"); // (25 - 21) * 333.87 = 1335 = 0x0537
        let block = data(&mut chip);
        assert_eq!(&block[2..4], [0x40, 0x00], "ay is 1 g");
        assert_eq!(&block[4..6], [0x00, 0x00], "az is 0 g");
        assert_eq!(&block[6..8], [0x05, 0x37], "25 °C");
        assert_eq!(&block[8..10], [0xFA, 0xE2], "-10 dps");

        chip.on_input("rest");
        assert_eq!(
            data(&mut chip),
            [0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
            "flat, still and at 21 °C again"
        );
    }

    #[test]
    fn the_inputs_clamp_to_the_power_on_ranges() {
        let mut chip = chip();
        chip.on_input("set:ax=5");
        chip.on_input("set:gx=1000");
        assert_eq!(&data(&mut chip)[0..2], [0x7F, 0xFF], "4 g saturates at 32767");
        assert_eq!(
            &data(&mut chip)[8..10],
            [0x7F, 0xEE],
            "1000 dps clamps to 250 dps = 32750 counts"
        );

        chip.on_input("set:temperature=100");
        assert_eq!(&data(&mut chip)[6..8], [0x53, 0x78], "85 °C is the top");

        chip.on_input("set:temperature=-100");
        assert_eq!(&data(&mut chip)[6..8], [0xB0, 0x72], "-40 °C is the bottom");
    }

    #[test]
    fn a_malformed_input_is_ignored() {
        let mut chip = chip();
        for name in [
            "set:ax=abc",
            "set:ax=",
            "set:ax=NaN",
            "set:ax=inf",
            "set:q=3",
            "set:az",
            "bogus",
            "",
        ] {
            chip.on_input(name);
        }
        assert_eq!(data(&mut chip)[4], 0x40, "az is still 1 g");
        assert_eq!(chip.temperature_c, TEMP_OFFSET_C);
    }

    #[test]
    fn a_combined_read_answers_in_one_transfer() {
        let mut chip = chip();
        write(&mut chip, SMPLRT_DIV, 0x07);
        let reply = chip.on_bus(&request(&[SMPLRT_DIV as u8 | READ], 2)).unwrap();
        assert_eq!(reply.data, [0x07, 0x00], "0x19, then the register after it");
    }

    #[test]
    fn a_full_duplex_write_still_stores_the_register() {
        // The runtime may deliver a write with a read side the master ignores.
        let mut chip = chip();
        let reply = chip.on_bus(&request(&[SMPLRT_DIV as u8, 0x07], 2)).unwrap();
        assert_eq!(reply.data, [0x00, 0x00]);
        assert_eq!(read(&mut chip, SMPLRT_DIV, 1), [0x07]);
    }

    #[test]
    fn it_claims_the_chip_select_and_the_bus_pads() {
        let spi = SpiClaim {
            cs_pin: Some(7),
            clk_pin: Some(6),
            mosi_pin: Some(3),
            miso_pin: Some(2),
            ..Default::default()
        };
        assert_eq!(chip().claims(), [Claim { what: Some(claim::What::Spi(spi)) }]);
        // Unwired: claimed without a chip select, which the handshake refuses.
        assert_eq!(
            Mpu9250::default().claims(),
            [Claim { what: Some(claim::What::Spi(SpiClaim::default())) }]
        );
    }

    #[test]
    fn it_shows_the_measurement() {
        let mut chip = chip();
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("ax"), shown.get("az"), shown.get("gz"), shown.get("temperature")),
            (Some("0.00"), Some("1.00"), Some("0.00"), Some("21.00"))
        );
        chip.on_input("set:ax=0.5");
        chip.on_input("set:gz=90");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("ax"), shown.get("gz")),
            (Some("0.50"), Some("90.00"))
        );
    }
}
