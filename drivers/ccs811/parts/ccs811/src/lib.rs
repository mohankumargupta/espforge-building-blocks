//! An AMS CCS811 eCO2 and TVOC air quality sensor, written as a virtual part.
//!
//! The firmware talks to it the way it would to the real chip: a register
//! address byte followed by a read or a write, the status and measurement-mode
//! bytes, and the four result bytes at 0x02.
//!
//! The part starts in boot mode, as a real CCS811 does: `APP_VALID` set,
//! `FW_MODE` clear. Writing `APP_START` runs the application firmware, and a
//! non-idle drive mode in `MEAS_MODE` makes `DATA_READY` set, so a measurement
//! is waiting at once; the part model has no conversion time.
//!
//! README.md lists what is left out.
//!
//! `Ccs811` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with ADDR low; ADDR high is 0x5B.
const DEFAULT_ADDR: u32 = 0x5A;

/// Register 0x00: error, data ready, application valid and firmware mode.
const STATUS: u8 = 0x00;
/// Register 0x01: the drive mode in bits 6:4 and the interrupt configuration.
const MEAS_MODE: u8 = 0x01;
/// Registers 0x02..=0x05: the algorithm's result, four big-endian bytes.
const ECO2_MSB: u8 = 0x02;
const ECO2_LSB: u8 = 0x03;
const TVOC_MSB: u8 = 0x04;
const TVOC_LSB: u8 = 0x05;
/// Registers 0x11..=0x12: the algorithm baseline, plain storage.
const BASELINE_MSB: u8 = 0x11;
const BASELINE_LSB: u8 = 0x12;
/// Register 0x20: the hardware id, 0x81 on every CCS811.
const HW_ID: u8 = 0x20;
const HW_ID_VALUE: u8 = 0x81;
/// Register 0x21: the hardware version of the reference design.
const HW_VERSION: u8 = 0x21;
const HW_VERSION_VALUE: u8 = 0x10;
/// Registers 0x23..=0x24 and 0x25..=0x26: the boot and application versions.
const FW_BOOT_MSB: u8 = 0x23;
const FW_BOOT_LSB: u8 = 0x24;
const FW_APP_MSB: u8 = 0x25;
const FW_APP_LSB: u8 = 0x26;
const FW_BOOT_VERSION: u16 = 0x1000;
const FW_APP_VERSION: u16 = 0x2000;
/// Register 0xE0: the error id, zero while the error bit is clear.
const ERROR_ID: u8 = 0xE0;
/// Register 0xF4: write it, with no data, to run the application firmware.
const APP_START: u8 = 0xF4;
/// Register 0xFF: a soft reset, whose data must be `0x11 0xE5 0x72 0x8A`.
const SW_RESET: u8 = 0xFF;
const SW_RESET_MAGIC: [u8; 4] = [0x11, 0xE5, 0x72, 0x8A];

/// Status bits, as the datasheet lays them out.
const STATUS_ERROR: u8 = 0b0000_0001;
const STATUS_DATA_READY: u8 = 0b0000_1000;
const STATUS_APP_VALID: u8 = 0b0001_0000;
const STATUS_FW_MODE: u8 = 0b1000_0000;

/// Bits 6:4 of `MEAS_MODE` are the drive mode.
const DRIVE_MODE_MASK: u8 = 0b0111_0000;

/// What the sensor sees when nobody has changed it: fresh air.
const DEFAULT_ECO2: u16 = 400;
const DEFAULT_TVOC: u16 = 8;

/// The ranges the datasheet gives for the algorithm's output.
const MIN_ECO2: f64 = 400.0;
const MAX_ECO2: f64 = 8192.0;
const MAX_TVOC: f64 = 1187.0;

struct Ccs811 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Register 0x01: drive mode and interrupt configuration.
    meas_mode: u8,
    /// Whether `APP_START` has been written: the application firmware runs.
    app_started: bool,
    /// Registers 0x11..=0x12: the algorithm baseline.
    baseline: [u8; 2],
    /// Register 0xE0: the error id, zero when the error bit is clear.
    error_id: u8,
    /// What the sensor sees, in ppm and ppb.
    eco2: u16,
    tvoc: u16,
}

impl Default for Ccs811 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            meas_mode: 0,
            app_started: false,
            baseline: [0; 2],
            error_id: 0,
            eco2: DEFAULT_ECO2,
            tvoc: DEFAULT_TVOC,
        }
    }
}

impl Device for Ccs811 {
    const NAME: &'static str = "ccs811";
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
    /// `rest` for fresh air.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "eco2" => self.eco2 = value.clamp(MIN_ECO2, MAX_ECO2) as u16,
                "tvoc" => self.tvoc = value.clamp(0.0, MAX_TVOC) as u16,
                _ => {}
            }
        } else if name == "rest" {
            self.eco2 = DEFAULT_ECO2;
            self.tvoc = DEFAULT_TVOC;
        }
    }

    /// The guest's I2C transaction: a register address, then a read or a
    /// write. A write of several bytes fills the registers that follow, and a
    /// read answers them in order.
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
        let register = i2c.write.first().copied().unwrap_or(0);
        let data = &i2c.write[1..];
        if register == APP_START {
            // APP_START takes no data byte: the register address alone runs
            // the application firmware.
            self.app_started = true;
        } else if register == SW_RESET {
            if data == SW_RESET_MAGIC {
                self.reset();
            }
        } else {
            for (offset, &byte) in data.iter().enumerate() {
                self.write_register(register.wrapping_add(offset as u8), byte);
            }
        }
        if i2c.read_len > 0 {
            reply.data = (0..i2c.read_len as usize)
                .map(|offset| self.register(register.wrapping_add(offset as u8)))
                .collect();
        }
        // A probe, a register write, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("eco2", self.eco2)
                .set("tvoc", self.tvoc)
                .set("status", self.status())
                .set("drive_mode", (self.meas_mode & DRIVE_MODE_MASK) >> 4)
                .set("app_started", u8::from(self.app_started)),
        )
    }
}

impl Ccs811 {
    /// The status byte: a valid application, and, once started and measuring,
    /// the firmware mode and a waiting measurement.
    fn status(&self) -> u8 {
        let mut status = STATUS_APP_VALID;
        if self.app_started {
            status |= STATUS_FW_MODE;
            if self.meas_mode & DRIVE_MODE_MASK != 0 {
                status |= STATUS_DATA_READY;
            }
        }
        if self.error_id != 0 {
            status |= STATUS_ERROR;
        }
        status
    }

    /// A soft reset: the control registers go back to their power-on values.
    /// Whether the application was started, and what the sensor sees, stay.
    fn reset(&mut self) {
        self.meas_mode = 0;
        self.baseline = [0; 2];
        self.error_id = 0;
    }

    /// One register's byte.
    fn register(&self, register: u8) -> u8 {
        match register {
            STATUS => self.status(),
            MEAS_MODE => self.meas_mode,
            ECO2_MSB => (self.eco2 >> 8) as u8,
            ECO2_LSB => self.eco2 as u8,
            TVOC_MSB => (self.tvoc >> 8) as u8,
            TVOC_LSB => self.tvoc as u8,
            BASELINE_MSB => self.baseline[0],
            BASELINE_LSB => self.baseline[1],
            HW_ID => HW_ID_VALUE,
            HW_VERSION => HW_VERSION_VALUE,
            FW_BOOT_MSB => (FW_BOOT_VERSION >> 8) as u8,
            FW_BOOT_LSB => FW_BOOT_VERSION as u8,
            FW_APP_MSB => (FW_APP_VERSION >> 8) as u8,
            FW_APP_LSB => FW_APP_VERSION as u8,
            ERROR_ID => self.error_id,
            _ => 0x00,
        }
    }

    /// One register write. The result registers are read-only; the rest are
    /// plain storage.
    fn write_register(&mut self, register: u8, value: u8) {
        match register {
            MEAS_MODE => self.meas_mode = value,
            BASELINE_MSB => self.baseline[0] = value,
            BASELINE_LSB => self.baseline[1] = value,
            _ => {}
        }
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Ccs811, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the ADDR-low address.
    fn chip() -> Ccs811 {
        let mut chip = Ccs811::default();
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

    /// One transaction to the sensor's address: the bytes read back, or `None`
    /// for a NACK.
    fn transfer(chip: &mut Ccs811, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// One register byte, the way the driver reads it.
    fn read_register(chip: &mut Ccs811, register: u8) -> u8 {
        transfer(chip, &[register], 1).expect("a register byte")[0]
    }

    /// The four result bytes, as the driver reads them.
    fn result(chip: &mut Ccs811) -> [u8; 4] {
        let bytes = transfer(chip, &[ECO2_MSB], 4).expect("four result bytes");
        [bytes[0], bytes[1], bytes[2], bytes[3]]
    }

    #[test]
    fn the_hardware_id_is_0x81() {
        assert_eq!(read_register(&mut chip(), HW_ID), 0x81);
    }

    #[test]
    fn the_part_powers_up_in_boot_mode() {
        let mut chip = chip();
        assert_eq!(
            read_register(&mut chip, STATUS),
            STATUS_APP_VALID,
            "application valid, firmware not running, nothing ready"
        );
        assert_eq!(read_register(&mut chip, MEAS_MODE), 0x00, "idle");
    }

    #[test]
    fn app_start_and_a_drive_mode_start_measuring() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[APP_START], 0), Some(vec![]));
        assert_eq!(read_register(&mut chip, STATUS), 0x90, "FW_MODE, app valid");
        assert_eq!(
            read_register(&mut chip, STATUS) & STATUS_DATA_READY,
            0,
            "still idle, so nothing is ready"
        );
        assert_eq!(transfer(&mut chip, &[MEAS_MODE, 0x10], 0), Some(vec![]));
        assert_eq!(
            read_register(&mut chip, STATUS),
            0x98,
            "now a measurement is waiting"
        );
    }

    #[test]
    fn the_result_is_big_endian_ppm_and_ppb() {
        let mut chip = chip();
        chip.on_input("set:eco2=800");
        chip.on_input("set:tvoc=50");
        assert_eq!(result(&mut chip), [0x03, 0x20, 0x00, 0x32]);
        chip.on_input("set:eco2=5120");
        assert_eq!(result(&mut chip), [0x14, 0x00, 0x00, 0x32]);
    }

    #[test]
    fn the_inputs_set_and_restore_the_air() {
        let mut chip = chip();
        chip.on_input("set:eco2=1000");
        chip.on_input("set:tvoc=100");
        assert_eq!(result(&mut chip), [0x03, 0xE8, 0x00, 0x64]);
        chip.on_input("rest");
        assert_eq!(result(&mut chip), [0x01, 0x90, 0x00, 0x08]);
    }

    #[test]
    fn the_inputs_stop_at_the_algorithm_ranges() {
        let mut chip = chip();
        chip.on_input("set:eco2=100");
        assert_eq!(chip.eco2, MIN_ECO2 as u16, "400 ppm is the floor");
        chip.on_input("set:eco2=9000");
        assert_eq!(chip.eco2, MAX_ECO2 as u16);
        chip.on_input("set:tvoc=5000");
        assert_eq!(chip.tvoc, MAX_TVOC as u16);
    }

    #[test]
    fn a_soft_reset_clears_the_control_registers() {
        let mut chip = chip();
        let _ = transfer(&mut chip, &[APP_START], 0);
        let _ = transfer(&mut chip, &[MEAS_MODE, 0x30], 0);
        let _ = transfer(&mut chip, &[BASELINE_MSB, 0xAA, 0xBB], 0);
        assert_eq!(
            transfer(&mut chip, &[SW_RESET, 0x11, 0xE5, 0x72, 0x8A], 0),
            Some(vec![])
        );
        assert_eq!(read_register(&mut chip, MEAS_MODE), 0x00);
        assert_eq!(transfer(&mut chip, &[BASELINE_MSB], 2), Some(vec![0, 0]));
        assert_eq!(
            read_register(&mut chip, STATUS),
            0x90,
            "the application keeps running, but nothing is measuring"
        );
        assert_eq!(
            result(&mut chip),
            [0x01, 0x90, 0x00, 0x08],
            "what the sensor sees survives"
        );
    }

    #[test]
    fn a_wrong_magic_does_not_reset_the_part() {
        let mut chip = chip();
        let _ = transfer(&mut chip, &[MEAS_MODE, 0x10], 0);
        assert_eq!(
            transfer(&mut chip, &[SW_RESET, 0x00, 0x00, 0x00, 0x00], 0),
            Some(vec![])
        );
        assert_eq!(read_register(&mut chip, MEAS_MODE), 0x10, "still measuring");
    }

    #[test]
    fn the_baseline_is_plain_storage() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[BASELINE_MSB, 0xAA, 0xBB], 0),
            Some(vec![])
        );
        assert_eq!(transfer(&mut chip, &[BASELINE_MSB], 2), Some(vec![0xAA, 0xBB]));
    }

    #[test]
    fn the_versions_are_reported() {
        let mut chip = chip();
        assert_eq!(read_register(&mut chip, HW_VERSION), 0x10);
        assert_eq!(
            transfer(&mut chip, &[FW_BOOT_MSB], 2),
            Some(vec![0x10, 0x00])
        );
        assert_eq!(
            transfer(&mut chip, &[FW_APP_MSB], 2),
            Some(vec![0x20, 0x00])
        );
        assert_eq!(read_register(&mut chip, ERROR_ID), 0x00);
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x5A,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_air_and_the_status() {
        let mut chip = chip();
        chip.on_input("set:eco2=800");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("eco2"), shown.get("tvoc"), shown.get("app_started")),
            (Some("800"), Some("8"), Some("0"))
        );
        let _ = transfer(&mut chip, &[APP_START], 0);
        let _ = transfer(&mut chip, &[MEAS_MODE, 0x10], 0);
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("status"), Some("152"), "0x98: ready and running");
        assert_eq!(shown.get("drive_mode"), Some("1"));
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x5B, &[STATUS], 1)).unwrap();
        assert!(!reply.ack, "nothing answers at the ADDR-high address");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x5A, &[STATUS], 1);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
