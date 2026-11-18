//! A Bosch BME280 temperature, pressure and humidity sensor, written as a
//! virtual part.
//!
//! The firmware talks to it the way it would to the real chip: a register
//! address byte followed by a read or a write, a factory calibration at
//! 0x88..0xA1 and 0xE1..0xE7, and the eight data registers at 0xF7..0xFE.
//!
//! The part keeps what the sensor sees in degrees Celsius, hectopascals and
//! percent relative humidity, and encodes it into the raw ADC registers using
//! its published calibration. The calibration is chosen so the datasheet's
//! integer compensation inverts it exactly:
//!
//! - `dig_T1 = 0`, `dig_T2 = 4096`, so `t_fine = adc_T / 4`,
//! - `dig_P1 = 6250` and the rest zero, so `p = 1048576 - adc_P` pascals,
//! - `dig_H2 = 16384` and the rest zero, so `%RH * 1024 = (adc_H + 1) * 256`.
//!
//! README.md lists what is left out.
//!
//! `Bme280` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The part's address with SDO low; SDO high is 0x77.
const DEFAULT_ADDR: u32 = 0x76;

/// Registers the driver uses.
const CHIP_ID: u8 = 0xD0;
const CHIP_ID_BME280: u8 = 0x60;
const RESET: u8 = 0xE0;
const RESET_COMMAND: u8 = 0xB6;
const CTRL_HUM: u8 = 0xF2;
const STATUS: u8 = 0xF3;
const CTRL_MEAS: u8 = 0xF4;
const CONFIG: u8 = 0xF5;
const CALIBRATION_START: u8 = 0x88;
const CALIBRATION_END: u8 = 0xA1;
const CALIBRATION2_START: u8 = 0xE1;
const CALIBRATION2_END: u8 = 0xE7;
const DATA_START: u8 = 0xF7;
const DATA_END: u8 = 0xFE;

/// The factory calibration this part publishes, as the datasheet lays it out.
const CALIBRATION: [u8; 26] = [
    0x00, 0x00, // dig_T1 = 0
    0x00, 0x10, // dig_T2 = 4096
    0x00, 0x00, // dig_T3 = 0
    0x6A, 0x18, // dig_P1 = 6250
    0x00, 0x00, // dig_P2
    0x00, 0x00, // dig_P3
    0x00, 0x00, // dig_P4
    0x00, 0x00, // dig_P5
    0x00, 0x00, // dig_P6
    0x00, 0x00, // dig_P7
    0x00, 0x00, // dig_P8
    0x00, 0x00, // dig_P9
    0x00, // dig_H1 = 0
    0x00, // 0xA1, reserved
];

const CALIBRATION2: [u8; 7] = [
    0x00, 0x40, // dig_H2 = 16384
    0x00, // dig_H3 = 0
    0x00, // 0xE4
    0x00, // 0xE5: the high nibble is dig_H5's low nibble
    0x00, // 0xE6
    0x00, // dig_H6 = 0
];

/// What the sensor sees when nobody has changed it: a mild room.
const DEFAULT_TEMPERATURE: f64 = 25.0;
const DEFAULT_PRESSURE: f64 = 1013.25;
const DEFAULT_HUMIDITY: f64 = 45.0;

/// The ranges the part accepts, from the datasheet.
const MIN_TEMPERATURE: f64 = -40.0;
const MAX_TEMPERATURE: f64 = 85.0;
const MIN_PRESSURE: f64 = 300.0;
const MAX_PRESSURE: f64 = 1100.0;
const MAX_HUMIDITY: f64 = 100.0;

/// The 20-bit ADC values.
const MAX_ADC: f64 = 1_048_575.0;

struct Bme280 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Register 0xF2: humidity oversampling.
    ctrl_hum: u8,
    /// Register 0xF4: temperature and pressure oversampling, and the mode.
    ctrl_meas: u8,
    /// Register 0xF5: standby time, filter and the SPI mode.
    config: u8,
    /// What the sensor sees.
    temperature: f64,
    pressure: f64,
    humidity: f64,
}

impl Default for Bme280 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            ctrl_hum: 0,
            ctrl_meas: 0,
            config: 0,
            temperature: DEFAULT_TEMPERATURE,
            pressure: DEFAULT_PRESSURE,
            humidity: DEFAULT_HUMIDITY,
        }
    }
}

impl Device for Bme280 {
    const NAME: &'static str = "bmxx80";
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

    /// Change what the sensor sees: `set:temperature=<°C>`,
    /// `set:pressure=<hPa>`, `set:humidity=<%RH>`, or `rest` for the defaults.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "temperature" => {
                    self.temperature = value.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE)
                }
                "pressure" => self.pressure = value.clamp(MIN_PRESSURE, MAX_PRESSURE),
                "humidity" => self.humidity = value.clamp(0.0, MAX_HUMIDITY),
                _ => {}
            }
        } else if name == "rest" {
            self.temperature = DEFAULT_TEMPERATURE;
            self.pressure = DEFAULT_PRESSURE;
            self.humidity = DEFAULT_HUMIDITY;
        }
    }

    /// The guest's I2C transaction: a register address, then a read or a
    /// write.
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
        if i2c.write.len() > 1 {
            self.write_register(register, &i2c.write[1..]);
        }
        if i2c.read_len > 0 {
            let data = self.data_registers();
            let mut bytes = vec![0u8; i2c.read_len as usize];
            for (offset, byte) in bytes.iter_mut().enumerate() {
                *byte = self.register_byte(register.wrapping_add(offset as u8), &data);
            }
            reply.data = bytes;
        }
        // A probe, a register write, or a read with an answer.
        reply.ack = true;
        Some(reply)
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("temperature", self.temperature)
                .set("pressure", self.pressure)
                .set("humidity", self.humidity),
        )
    }
}

impl Bme280 {
    /// One register write. The soft reset clears the control registers; the
    /// calibration and what the sensor sees stay.
    fn write_register(&mut self, register: u8, value: &[u8]) {
        let Some(&value) = value.first() else {
            return;
        };
        match register {
            RESET if value == RESET_COMMAND => {
                self.ctrl_hum = 0;
                self.ctrl_meas = 0;
                self.config = 0;
            }
            CTRL_HUM => self.ctrl_hum = value,
            CTRL_MEAS => self.ctrl_meas = value,
            CONFIG => self.config = value,
            _ => {}
        }
    }

    /// One register's byte. The data registers are encoded afresh, so a
    /// measurement always reflects what the sensor currently sees.
    fn register_byte(&self, register: u8, data: &[u8; 8]) -> u8 {
        match register {
            CHIP_ID => CHIP_ID_BME280,
            RESET => 0x00,
            CTRL_HUM => self.ctrl_hum,
            // Bit 3 is "measuring", bit 0 "im_update"; the part model has no
            // time, so neither is ever set.
            STATUS => 0x00,
            CTRL_MEAS => self.ctrl_meas,
            CONFIG => self.config,
            CALIBRATION_START..=CALIBRATION_END => CALIBRATION[usize::from(register - CALIBRATION_START)],
            CALIBRATION2_START..=CALIBRATION2_END => {
                CALIBRATION2[usize::from(register - CALIBRATION2_START)]
            }
            DATA_START..=DATA_END => data[usize::from(register - DATA_START)],
            _ => 0x00,
        }
    }

    /// The eight data registers, as the datasheet lays them out: pressure,
    /// then temperature, each as a 20-bit value left-aligned in three bytes,
    /// then the 16-bit humidity.
    fn data_registers(&self) -> [u8; 8] {
        // t_fine = adc_T / 4, and the temperature is (t_fine * 5 + 128) >> 8
        // in 0.01 degrees. t_fine is kept even so the inverse stays exact.
        let temperature = (self.temperature * 100.0).round() as i64;
        let t_fine = (((temperature * 256 + 2) / 5) / 2) * 2;
        let adc_t = (t_fine * 4).clamp(0, MAX_ADC as i64) as u32;

        // p = 1048576 - adc_P pascals.
        let adc_p = ((1_048_576.0 - (self.pressure * 100.0).round()) as i64).clamp(0, MAX_ADC as i64)
            as u32;

        // %RH * 1024 = (adc_H + 1) * 256.
        let adc_h = ((self.humidity * 4.0).round() as i64 - 1).clamp(0, 65_535) as u16;

        let pressure = adc_p << 4;
        let temperature = adc_t << 4;
        [
            (pressure >> 16) as u8,
            (pressure >> 8) as u8,
            pressure as u8,
            (temperature >> 16) as u8,
            (temperature >> 8) as u8,
            temperature as u8,
            (adc_h >> 8) as u8,
            adc_h as u8,
        ]
    }
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Bme280, buffer = 768);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the SDO-low address.
    fn chip() -> Bme280 {
        let mut chip = Bme280::default();
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
    fn transfer(chip: &mut Bme280, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// One register's byte, the way the driver reads it.
    fn read_register(chip: &mut Bme280, register: u8) -> u8 {
        transfer(chip, &[register], 1).expect("a register byte")[0]
    }

    /// The eight data registers, as the driver reads them.
    fn data(chip: &mut Bme280) -> Vec<u8> {
        transfer(chip, &[DATA_START], 8).expect("eight data bytes")
    }

    #[test]
    fn it_reports_the_bme280_chip_id() {
        assert_eq!(read_register(&mut chip(), CHIP_ID), 0x60);
    }

    #[test]
    fn the_calibration_is_the_published_one() {
        let mut chip = chip();
        let calibration = transfer(&mut chip, &[CALIBRATION_START], 26).unwrap();
        assert_eq!(calibration, CALIBRATION);
        assert_eq!(calibration[0x8A - 0x88], 0x00);
        assert_eq!(calibration[0x8B - 0x88], 0x10, "dig_T2 = 4096");
        assert_eq!(calibration[0x8E - 0x88], 0x6A);
        assert_eq!(calibration[0x8F - 0x88], 0x18, "dig_P1 = 6250");

        let calibration2 = transfer(&mut chip, &[CALIBRATION2_START], 7).unwrap();
        assert_eq!(calibration2, CALIBRATION2);
        assert_eq!(calibration2[0xE2 - 0xE1], 0x40, "dig_H2 = 16384");
    }

    #[test]
    fn the_default_reading_encodes_25_degrees() {
        let mut chip = chip();
        // 25.00 degrees is t_fine 128000, so adc_T = 512000 = 0x7D000.
        // 1013.25 hPa is 101325 Pa, so adc_P = 1048576 - 101325 = 0xE7433.
        // 45 %RH is adc_H = 179 = 0x00B3.
        assert_eq!(
            data(&mut chip),
            [0xE7, 0x43, 0x30, 0x7D, 0x00, 0x00, 0x00, 0xB3]
        );
    }

    #[test]
    fn the_inputs_reencode_the_data_registers() {
        let mut chip = chip();
        chip.on_input("set:temperature=30");
        // 30.00 degrees is t_fine 153600, so adc_T = 614400 = 0x96000.
        assert_eq!(data(&mut chip)[3..6], [0x96, 0x00, 0x00]);

        chip.on_input("set:humidity=60");
        // 60 %RH is adc_H = 239 = 0x00EF.
        assert_eq!(data(&mut chip)[6..8], [0x00, 0xEF]);

        chip.on_input("set:pressure=1000");
        // 1000 hPa is 100000 Pa, so adc_P = 948576 = 0xE7960.
        assert_eq!(data(&mut chip)[..3], [0xE7, 0x96, 0x00]);
    }

    #[test]
    fn rest_puts_the_defaults_back() {
        let mut chip = chip();
        chip.on_input("set:temperature=30");
        chip.on_input("set:pressure=1000");
        chip.on_input("set:humidity=60");
        chip.on_input("rest");
        assert_eq!(
            data(&mut chip),
            [0xE7, 0x43, 0x30, 0x7D, 0x00, 0x00, 0x00, 0xB3]
        );
    }

    #[test]
    fn the_measurement_registers_are_stored() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[CTRL_HUM, 0x01], 0),
            Some(vec![])
        );
        assert_eq!(transfer(&mut chip, &[CTRL_MEAS, 0x27], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[CONFIG, 0x10], 0), Some(vec![]));
        assert_eq!(read_register(&mut chip, CTRL_HUM), 0x01);
        assert_eq!(read_register(&mut chip, CTRL_MEAS), 0x27);
        assert_eq!(read_register(&mut chip, CONFIG), 0x10);
        assert_eq!(read_register(&mut chip, STATUS), 0x00, "never measuring");
    }

    #[test]
    fn a_soft_reset_clears_the_control_registers() {
        let mut chip = chip();
        let _ = transfer(&mut chip, &[CTRL_HUM, 0x01], 0);
        let _ = transfer(&mut chip, &[CTRL_MEAS, 0x27], 0);
        assert_eq!(transfer(&mut chip, &[RESET, RESET_COMMAND], 0), Some(vec![]));
        assert_eq!(read_register(&mut chip, CTRL_HUM), 0x00);
        assert_eq!(read_register(&mut chip, CTRL_MEAS), 0x00);
        assert_eq!(
            read_register(&mut chip, CHIP_ID),
            0x60,
            "the chip id survives a reset"
        );
    }

    #[test]
    fn the_inputs_are_clamped_to_the_datasheet_ranges() {
        let mut chip = chip();
        chip.on_input("set:temperature=-100");
        assert_eq!(chip.temperature, MIN_TEMPERATURE);
        chip.on_input("set:pressure=5000");
        assert_eq!(chip.pressure, MAX_PRESSURE);
        chip.on_input("set:humidity=200");
        assert_eq!(chip.humidity, MAX_HUMIDITY);
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x76,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_weather() {
        let mut chip = chip();
        chip.on_input("set:temperature=30");
        chip.on_input("set:humidity=60");
        let shown = chip.state().unwrap();
        assert_eq!(
            (
                shown.get("temperature"),
                shown.get("pressure"),
                shown.get("humidity")
            ),
            (Some("30"), Some("1013.25"), Some("60"))
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x77, &[CHIP_ID], 1)).unwrap();
        assert!(!reply.ack, "nothing answers at the SDO-high address");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x76, &[CHIP_ID], 1);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
