//! A Bosch BME280 temperature, humidity and pressure sensor, written as a
//! virtual part.
//!
//! The firmware reads it over I2C the way it would the real chip: a write of
//! the register pointer, then a read (or one combined `write_read`), with the
//! pointer auto-incrementing. The first data byte is `press_msb` at 0xF7 and
//! the last is `hum_lsb` at 0xFE. `ID` (0xD0) answers 0x60 for a BME280. The
//! temperature, pressure and humidity are not stored as raw counts: the part
//! keeps the air around the sensor and inverts Bosch's compensation formulas
//! so a register read encodes the raw count a real chip would have produced.
//! README.md lists what is left out.
//!
//! `Bme280I2c` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate exports
//! every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The address with the SDO pin low. With it high the address is 0x77, which
/// the board gives as `addr`.
const DEFAULT_ADDR: u32 = 0x76;

/// `ID`: the fixed identity register, 0x60 on a BME280 (0x58 is a BMP280).
const ID: u8 = 0xD0;
const ID_VALUE: u8 = 0x60;
/// `reset`: write 0xB6 to run the power-on sequence.
const RESET: u8 = 0xE0;
const RESET_VALUE: u8 = 0xB6;
/// `ctrl_hum`, `status`, `ctrl_meas` and `config`.
const CTRL_HUM: u8 = 0xF2;
const STATUS: u8 = 0xF3;
const CTRL_MEAS: u8 = 0xF4;
const CONFIG: u8 = 0xF5;
/// `press_msb` through `hum_lsb`: the eight data bytes, pressure first.
const PRESS_MSB: u8 = 0xF7;
const HUM_LSB: u8 = 0xFE;

/// The raw counts are 20 bits for pressure and temperature, 16 for humidity.
const MAX_ADC: f64 = 1_048_575.0;
const MAX_HUMIDITY_ADC: f64 = 65_535.0;

/// What the air is like when nobody has changed it: room conditions.
const DEFAULT_TEMP_C: f64 = 22.5;
const DEFAULT_PRESSURE_HPA: f64 = 1013.25;
const DEFAULT_HUMIDITY: f64 = 45.0;

/// A reading can carry this much: the sensor's operating range and the
/// humidity the compensation formula returns.
const MIN_TEMP_C: f64 = -40.0;
const MAX_TEMP_C: f64 = 85.0;
const MIN_PRESSURE_HPA: f64 = 300.0;
const MAX_PRESSURE_HPA: f64 = 1100.0;
const MIN_HUMIDITY: f64 = 0.0;
const MAX_HUMIDITY: f64 = 100.0;

/// The sensor's calibration constants, as a real chip's one-time-programmed
/// registers hold them. These are a Bosch BME280 part's values; they make the
/// compensation formulas below produce sensible readings.
const DIG_T1: u16 = 27_504;
const DIG_T2: i16 = 26_435;
const DIG_T3: i16 = -1_000;
const DIG_P1: u16 = 36_477;
const DIG_P2: i16 = -10_685;
const DIG_P3: i16 = 3_024;
const DIG_P4: i16 = 2_855;
const DIG_P5: i16 = 140;
const DIG_P6: i16 = -7;
const DIG_P7: i16 = 15_500;
const DIG_P8: i16 = -14_600;
const DIG_P9: i16 = 6_000;
const DIG_H1: u8 = 75;
const DIG_H2: i16 = 370;
const DIG_H3: u8 = 0;
const DIG_H4: i16 = 313;
const DIG_H5: i16 = 50;
const DIG_H6: i8 = 30;

struct Bme280I2c {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// The air around the sensor, in °C, hPa and %RH.
    temp_c: f64,
    pressure_hpa: f64,
    humidity: f64,
    /// Registers 0x00..=0xFF. Calibration and control registers are plain
    /// storage; `ID`, `status`, `reset` and the data block are answered from
    /// the fields above.
    regs: [u8; 256],
    /// The register the next read starts at: the last pointer written.
    pointer: u8,
    /// The rest of an answer a read has started, which a request that
    /// `continues` that read clocks out next.
    reading: Vec<u8>,
}

impl Default for Bme280I2c {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            temp_c: DEFAULT_TEMP_C,
            pressure_hpa: DEFAULT_PRESSURE_HPA,
            humidity: DEFAULT_HUMIDITY,
            regs: power_on_registers(),
            pointer: 0,
            reading: Vec::new(),
        }
    }
}

impl Device for Bme280I2c {
    const NAME: &'static str = "bme280_i2c";
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
    /// The emulator works out the I2C port from those pins. A sensor the
    /// board wired to one pin, or to none (a pin named `scl` rather than
    /// `SCL` is not one of its pins), still claims, with the missing pins
    /// left out, so the emulator refuses the claim and names what the board
    /// missed.
    fn claims(&self) -> Vec<Claim> {
        let i2c = I2cClaim {
            addr: self.addr,
            scl_pin: self.scl,
            sda_pin: self.sda,
        };
        vec![Claim {
            what: Some(claim::What::I2c(i2c)),
        }]
    }

    /// Change the air. `set:temperature=<°C>`, `set:pressure=<hPa>` and
    /// `set:humidity=<%RH>` put an exact value; the named steps move one.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "temperature" => self.temp_c = value,
                "pressure" => self.pressure_hpa = value,
                "humidity" => self.humidity = value,
                _ => {} // Not a value this part has.
            }
        } else {
            match name {
                "warmer" => self.temp_c += 1.0,
                "cooler" => self.temp_c -= 1.0,
                "higher" => self.pressure_hpa += 1.0,
                "lower" => self.pressure_hpa -= 1.0,
                "wetter" => self.humidity += 5.0,
                "drier" => self.humidity -= 5.0,
                "rest" => {
                    self.temp_c = DEFAULT_TEMP_C;
                    self.pressure_hpa = DEFAULT_PRESSURE_HPA;
                    self.humidity = DEFAULT_HUMIDITY;
                }
                _ => {} // Not an interaction this part has.
            }
        }
        self.temp_c = self.temp_c.clamp(MIN_TEMP_C, MAX_TEMP_C);
        self.pressure_hpa = self.pressure_hpa.clamp(MIN_PRESSURE_HPA, MAX_PRESSURE_HPA);
        self.humidity = self.humidity.clamp(MIN_HUMIDITY, MAX_HUMIDITY);
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
            // A write with a value is a register write; the ones after the
            // first auto-increment, as the chip's pointer does.
            for (offset, &value) in i2c.write[1..].iter().enumerate() {
                self.write(register.wrapping_add(offset as u8) as usize, value);
            }
            if i2c.write.len() > 1 {
                self.pointer = register.wrapping_add((i2c.write.len() - 1) as u8);
            }
        }
        if i2c.read_len > 0 {
            if !i2c.continues {
                // A read after a START takes the answer from the pointer. Any
                // earlier part-read answer is dropped.
                let start = self.pointer as usize;
                self.reading = (0..i2c.read_len as usize)
                    .map(|i| self.register((start + i) & 0xFF))
                    .collect();
                self.pointer = self.pointer.wrapping_add(i2c.read_len as u8);
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
                .set("temperature", self.temp_c)
                .set("pressure", self.pressure_hpa)
                .set("humidity", self.humidity),
        )
    }
}

impl Bme280I2c {
    /// Register `reg`. `ID`, `status`, `reset` and the eight data bytes are
    /// live; everything else is the stored register file.
    fn register(&self, reg: usize) -> u8 {
        match reg {
            r if r == ID as usize => ID_VALUE,
            // Nothing is in progress: the part always has a reading ready.
            r if r == STATUS as usize => 0,
            r if r == RESET as usize => 0,
            r if (PRESS_MSB as usize..=HUM_LSB as usize).contains(&r) => {
                self.data_bytes()[r - PRESS_MSB as usize]
            }
            _ => self.regs[reg],
        }
    }

    /// A write. Only the three control registers and the reset register are
    /// writable, as on the chip; the calibration and data registers ignore it.
    fn write(&mut self, reg: usize, value: u8) {
        match reg {
            r if r == RESET as usize => {
                if value == RESET_VALUE {
                    self.soft_reset();
                }
            }
            r if r == CTRL_HUM as usize || r == CTRL_MEAS as usize || r == CONFIG as usize => {
                self.regs[r] = value;
            }
            _ => {}
        }
    }

    /// The power-on register values: calibration in place, control registers
    /// at 0x00, which is sleep mode with no oversampling.
    fn soft_reset(&mut self) {
        self.regs[CTRL_HUM as usize] = 0;
        self.regs[CTRL_MEAS as usize] = 0;
        self.regs[CONFIG as usize] = 0;
    }

    /// The eight data bytes, pressure then temperature then humidity, each as
    /// the raw count a real chip would have converted from the air.
    fn data_bytes(&self) -> [u8; 8] {
        let calibration = Calibration::from_regs(&self.regs);
        let adc_t = calibration.adc_for_temperature(self.temp_c);
        let (_, t_fine) = calibration.temperature(adc_t as f64);
        let adc_p = calibration.adc_for_pressure(self.pressure_hpa, t_fine);
        let adc_h = calibration.adc_for_humidity(self.humidity, t_fine);
        encode(adc_p, adc_t, adc_h)
    }
}

/// The chip's calibration constants, decoded from the one-time-programmed
/// registers.
#[derive(Clone, Copy, Default)]
struct Calibration {
    t1: f64,
    t2: f64,
    t3: f64,
    p1: f64,
    p2: f64,
    p3: f64,
    p4: f64,
    p5: f64,
    p6: f64,
    p7: f64,
    p8: f64,
    p9: f64,
    h1: f64,
    h2: f64,
    h3: f64,
    h4: f64,
    h5: f64,
    h6: f64,
}

impl Calibration {
    /// 0x88..=0xA1 holds T1..T3, P1..P9 and H1; 0xE1..=0xE7 holds H2..H6.
    fn from_regs(regs: &[u8; 256]) -> Self {
        let h4 = ((regs[0xE4] as i16) << 4) | ((regs[0xE5] & 0x0F) as i16);
        let h5 = ((regs[0xE6] as i16) << 4) | ((regs[0xE5] >> 4) as i16);
        Self {
            t1: u16_at(regs, 0x88) as f64,
            t2: i16_at(regs, 0x8A) as f64,
            t3: i16_at(regs, 0x8C) as f64,
            p1: u16_at(regs, 0x8E) as f64,
            p2: i16_at(regs, 0x90) as f64,
            p3: i16_at(regs, 0x92) as f64,
            p4: i16_at(regs, 0x94) as f64,
            p5: i16_at(regs, 0x96) as f64,
            p6: i16_at(regs, 0x98) as f64,
            p7: i16_at(regs, 0x9A) as f64,
            p8: i16_at(regs, 0x9C) as f64,
            p9: i16_at(regs, 0x9E) as f64,
            h1: regs[0xA1] as f64,
            h2: i16_at(regs, 0xE1) as f64,
            h3: regs[0xE3] as f64,
            h4: sign_12(h4) as f64,
            h5: sign_12(h5) as f64,
            h6: regs[0xE7] as i8 as f64,
        }
    }

    /// Bosch's temperature compensation (BME280 datasheet §4.2.3), returning
    /// °C and the `t_fine` the other two formulas need.
    fn temperature(&self, adc: f64) -> (f64, f64) {
        let var1 = (adc / 16_384.0 - self.t1 / 1_024.0) * self.t2;
        let var2 = (adc / 131_072.0 - self.t1 / 8_192.0).powi(2) * self.t3;
        let t_fine = var1 + var2;
        (t_fine / 5_120.0, t_fine)
    }

    /// Bosch's pressure compensation, in pascals.
    fn pressure_pa(&self, adc: f64, t_fine: f64) -> f64 {
        let mut var1 = t_fine / 2.0 - 64_000.0;
        let mut var2 = var1 * var1 * self.p6 / 32_768.0;
        var2 += var1 * self.p5 * 2.0;
        var2 = var2 / 4.0 + self.p4 * 65_536.0;
        var1 = (self.p3 * var1 * var1 / 524_288.0 + self.p2 * var1) / 524_288.0;
        var1 = (1.0 + var1 / 32_768.0) * self.p1;
        if var1 == 0.0 {
            return 0.0;
        }
        let mut p = 1_048_576.0 - adc;
        p = (p - var2 / 4_096.0) * 6_250.0 / var1;
        let var1 = self.p9 * p * p / 2_147_483_648.0;
        let var2 = p * self.p8 / 32_768.0;
        p + (var1 + var2 + self.p7) / 16.0
    }

    /// Bosch's humidity compensation, in %RH, held to 0..=100.
    fn humidity_percent(&self, adc: f64, t_fine: f64) -> f64 {
        let temp = t_fine - 76_800.0;
        let mut h = adc - (self.h4 * 64.0 + self.h5 / 16_384.0 * temp);
        h *= self.h2 / 65_536.0
            * (1.0
                + self.h6 / 67_108_864.0
                    * temp
                    * (1.0 + self.h3 / 67_108_864.0 * temp));
        h *= 1.0 - self.h1 * h / 524_288.0;
        h.clamp(0.0, 100.0)
    }

    /// The raw temperature count that reads back as `celsius`: Bosch's
    /// quadratic, solved for the ADC value.
    fn adc_for_temperature(&self, celsius: f64) -> u32 {
        let target = celsius * 5_120.0;
        let a = self.t3 / (131_072.0 * 131_072.0);
        let b = self.t2 / 16_384.0 - 2.0 * self.t3 * self.t1 / (131_072.0 * 8_192.0);
        let c = self.t3 * (self.t1 / 8_192.0).powi(2) - self.t2 * self.t1 / 1_024.0 - target;
        let mut roots = [0.0f64; 2];
        let count = if a.abs() < 1e-18 {
            if b == 0.0 {
                return 0;
            }
            roots[0] = -c / b;
            1
        } else {
            let discriminant = b * b - 4.0 * a * c;
            if discriminant < 0.0 {
                return 0;
            }
            let root = discriminant.sqrt();
            roots[0] = (-b + root) / (2.0 * a);
            roots[1] = (-b - root) / (2.0 * a);
            2
        };
        let mut best = 0.0;
        let mut best_error = f64::MAX;
        for &root in &roots[..count] {
            if (0.0..=MAX_ADC).contains(&root) {
                let error = (self.temperature(root).0 - celsius).abs();
                if error < best_error {
                    best = root;
                    best_error = error;
                }
            }
        }
        best.round().clamp(0.0, MAX_ADC) as u32
    }

    /// The raw pressure count that reads back as `hpa`. The pressure falls as
    /// the count rises, so the binary search is the other way round from the
    /// temperature solver.
    fn adc_for_pressure(&self, hpa: f64, t_fine: f64) -> u32 {
        let target = hpa * 100.0;
        let (mut low, mut high) = (0u32, MAX_ADC as u32);
        while low < high {
            let middle = (low + high) / 2;
            if self.pressure_pa(middle as f64, t_fine) > target {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let mut best = low;
        let mut best_error = f64::MAX;
        for candidate in [low.saturating_sub(1), low, (low + 1).min(MAX_ADC as u32)] {
            let error = (self.pressure_pa(candidate as f64, t_fine) - target).abs();
            if error < best_error {
                best = candidate;
                best_error = error;
            }
        }
        best
    }

    /// The raw humidity count that reads back as `percent`.
    fn adc_for_humidity(&self, percent: f64, t_fine: f64) -> u32 {
        let (mut low, mut high) = (0u32, MAX_HUMIDITY_ADC as u32);
        while low < high {
            let middle = (low + high) / 2;
            if self.humidity_percent(middle as f64, t_fine) < percent {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let mut best = low;
        let mut best_error = f64::MAX;
        for candidate in [low.saturating_sub(1), low, (low + 1).min(MAX_HUMIDITY_ADC as u32)] {
            let error = (self.humidity_percent(candidate as f64, t_fine) - percent).abs();
            if error < best_error {
                best = candidate;
                best_error = error;
            }
        }
        best
    }
}

/// The power-on register file: calibration programmed, control registers 0.
fn power_on_registers() -> [u8; 256] {
    let mut regs = [0u8; 256];
    put_u16(&mut regs, 0x88, DIG_T1);
    put_i16(&mut regs, 0x8A, DIG_T2);
    put_i16(&mut regs, 0x8C, DIG_T3);
    put_u16(&mut regs, 0x8E, DIG_P1);
    put_i16(&mut regs, 0x90, DIG_P2);
    put_i16(&mut regs, 0x92, DIG_P3);
    put_i16(&mut regs, 0x94, DIG_P4);
    put_i16(&mut regs, 0x96, DIG_P5);
    put_i16(&mut regs, 0x98, DIG_P6);
    put_i16(&mut regs, 0x9A, DIG_P7);
    put_i16(&mut regs, 0x9C, DIG_P8);
    put_i16(&mut regs, 0x9E, DIG_P9);
    regs[0xA1] = DIG_H1;
    put_i16(&mut regs, 0xE1, DIG_H2);
    regs[0xE3] = DIG_H3;
    regs[0xE4] = (DIG_H4 >> 4) as u8;
    regs[0xE5] = ((DIG_H4 & 0x0F) as u8) | (((DIG_H5 & 0x0F) as u8) << 4);
    regs[0xE6] = (DIG_H5 >> 4) as u8;
    regs[0xE7] = DIG_H6 as u8;
    regs
}

fn put_u16(regs: &mut [u8; 256], reg: usize, value: u16) {
    regs[reg..reg + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_i16(regs: &mut [u8; 256], reg: usize, value: i16) {
    regs[reg..reg + 2].copy_from_slice(&value.to_le_bytes());
}

fn u16_at(regs: &[u8; 256], reg: usize) -> u16 {
    u16::from_le_bytes([regs[reg], regs[reg + 1]])
}

fn i16_at(regs: &[u8; 256], reg: usize) -> i16 {
    i16::from_le_bytes([regs[reg], regs[reg + 1]])
}

/// The H4/H5 nibbles are 12-bit two's complement.
fn sign_12(value: i16) -> i16 {
    (value << 4) >> 4
}

/// The eight data bytes, pressure then temperature then humidity, in the
/// chip's 20/20/16-bit layout.
fn encode(adc_p: u32, adc_t: u32, adc_h: u32) -> [u8; 8] {
    [
        (adc_p >> 12) as u8,
        (adc_p >> 4) as u8,
        (adc_p << 4) as u8,
        (adc_t >> 12) as u8,
        (adc_t >> 4) as u8,
        (adc_t << 4) as u8,
        (adc_h >> 8) as u8,
        adc_h as u8,
    ]
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Bme280I2c, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// and strapped to `addr`.
    fn wired(addr: u32) -> Bme280I2c {
        let mut chip = Bme280I2c::default();
        let pins = [(SCL.to_string(), 6), (SDA.to_string(), 5)].into();
        chip.configure(&PartConfig { pins, addr });
        chip
    }

    fn chip() -> Bme280I2c {
        wired(0)
    }

    fn request(addr: u32, write: &[u8], read_len: u32) -> BusRequest {
        let i2c = I2cTransaction {
            bus: 0,
            addr,
            write: write.to_vec(),
            read_len,
            continues: false,
        };
        BusRequest {
            correlation_id: 7,
            op: Some(bus_request::Op::I2c(i2c)),
        }
    }

    /// One transaction to the sensor's address: the bytes read back, or `None`
    /// for a NACK.
    fn transfer(chip: &mut Bme280I2c, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// A register read the way `embedded-hal`'s `write_read` sends it: the
    /// pointer and the answer in one transaction.
    fn read(chip: &mut Bme280I2c, reg: u8, len: u32) -> Vec<u8> {
        transfer(chip, &[reg], len).expect("a read is answered")
    }

    /// Decode the eight data bytes the way the driver does.
    fn decoded(chip: &mut Bme280I2c) -> (f64, f64, f64) {
        let raw = read(chip, PRESS_MSB, 8);
        let adc_p = (raw[0] as u32) << 12 | (raw[1] as u32) << 4 | (raw[2] as u32) >> 4;
        let adc_t = (raw[3] as u32) << 12 | (raw[4] as u32) << 4 | (raw[5] as u32) >> 4;
        let adc_h = (raw[6] as u32) << 8 | raw[7] as u32;
        let calibration = Calibration::from_regs(&chip.regs);
        let (temperature, t_fine) = calibration.temperature(adc_t as f64);
        (
            temperature,
            calibration.pressure_pa(adc_p as f64, t_fine) / 100.0,
            calibration.humidity_percent(adc_h as f64, t_fine),
        )
    }

    fn close(actual: f64, expected: f64) -> bool {
        (actual - expected).abs() < 0.05
    }

    #[test]
    fn the_id_register_is_0x60_for_a_bme280() {
        assert_eq!(read(&mut chip(), ID, 1), [0x60]);
    }

    #[test]
    fn a_read_auto_increments_from_the_pointer() {
        let mut chip = chip();
        // The calibration block starts at 0x88: T1 is 27504, little-endian.
        let calibration = read(&mut chip, 0x88, 26);
        assert_eq!(calibration[..2], [0x70, 0x6B], "dig_T1");
        assert_eq!(calibration[25], DIG_H1, "dig_H1 at 0xA1");
        // A pointer write on its own sets where the next read starts, so a
        // read with no pointer of its own answers from 0xE1.
        assert_eq!(transfer(&mut chip, &[0xE1], 0), Some(vec![]));
        let kept = transfer(&mut chip, &[], 2).expect("a read");
        assert_eq!(kept, read(&mut chip, 0xE1, 2), "pointer kept");
    }

    #[test]
    fn room_conditions_encode_to_the_datasheets_counts() {
        // 22.5 °C, 1013.25 hPa and 45 %RH invert to 0x7CEA0, 0x63D61 and
        // 0x6D84, packed pressure first.
        let mut chip = chip();
        assert_eq!(
            read(&mut chip, PRESS_MSB, 8),
            [0x63, 0xD6, 0x10, 0x7C, 0xEA, 0x00, 0x6D, 0x84]
        );
        let (temperature, pressure, humidity) = decoded(&mut chip);
        assert!(close(temperature, 22.5), "{temperature}");
        assert!(close(pressure, 1013.25), "{pressure}");
        assert!(close(humidity, 45.0), "{humidity}");
    }

    #[test]
    fn the_inputs_move_one_value_at_a_time() {
        let mut chip = chip();
        chip.on_input("warmer");
        let (temperature, pressure, humidity) = decoded(&mut chip);
        assert!(close(temperature, 23.5), "{temperature}");
        assert!(close(pressure, 1013.25), "{pressure}");
        assert!(close(humidity, 45.0), "{humidity}");

        chip.on_input("set:pressure=1000");
        let (temperature, pressure, _) = decoded(&mut chip);
        assert!(close(temperature, 23.5), "pressure does not move temperature");
        assert!(close(pressure, 1000.0), "{pressure}");

        chip.on_input("set:humidity=62");
        let (_, pressure, humidity) = decoded(&mut chip);
        assert!(close(pressure, 1000.0), "humidity does not move pressure");
        assert!(close(humidity, 62.0), "{humidity}");

        chip.on_input("set:temperature=31.5");
        let (temperature, _, _) = decoded(&mut chip);
        assert!(close(temperature, 31.5), "{temperature}");
    }

    #[test]
    fn a_set_of_every_value_reads_back_at_that_value() {
        let mut chip = chip();
        chip.on_input("set:temperature=31.5");
        chip.on_input("set:pressure=1020");
        chip.on_input("set:humidity=62");
        let (temperature, pressure, humidity) = decoded(&mut chip);
        assert!(close(temperature, 31.5), "{temperature}");
        assert!(close(pressure, 1020.0), "{pressure}");
        assert!(close(humidity, 62.0), "{humidity}");
    }

    #[test]
    fn the_inputs_stop_at_the_sensors_range() {
        let mut chip = chip();
        chip.on_input("set:temperature=200");
        chip.on_input("set:pressure=5000");
        chip.on_input("set:humidity=-5");
        assert_eq!(
            (chip.temp_c, chip.pressure_hpa, chip.humidity),
            (85.0, 1100.0, 0.0)
        );
        chip.on_input("set:temperature=-100");
        chip.on_input("set:pressure=0");
        chip.on_input("set:humidity=150");
        assert_eq!(
            (chip.temp_c, chip.pressure_hpa, chip.humidity),
            (-40.0, 300.0, 100.0)
        );
        chip.on_input("rest");
        assert_eq!(
            (chip.temp_c, chip.pressure_hpa, chip.humidity),
            (DEFAULT_TEMP_C, DEFAULT_PRESSURE_HPA, DEFAULT_HUMIDITY)
        );
    }

    #[test]
    fn a_write_stores_a_control_register_and_reset_clears_it() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[CTRL_MEAS, 0x27], 0),
            Some(vec![]),
            "a write is acknowledged"
        );
        assert_eq!(chip.register(CTRL_MEAS as usize), 0x27);
        assert_eq!(transfer(&mut chip, &[RESET, RESET_VALUE], 0), Some(vec![]));
        assert_eq!(chip.register(CTRL_MEAS as usize), 0x00, "back to sleep");
    }

    #[test]
    fn a_probe_and_a_read_of_another_address_are_nacked() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[], 0), Some(vec![]), "a probe");
        let wrong = chip.on_bus(&request(0x77, &[], 0)).unwrap();
        assert!(!wrong.ack, "0x77 is not its address");
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
        assert_eq!(chip().claims(), [i2c(0x76)]);

        // The SDO pin high: the board says 0x77.
        let mut chip = wired(0x77);
        assert_eq!(chip.claims(), [i2c(0x77)]);
        assert!(chip.on_bus(&request(0x77, &[], 0)).unwrap().ack);
        assert!(!chip.on_bus(&request(0x76, &[], 0)).unwrap().ack);

        // Unwired, or one pin wired: claimed with what is missing left out,
        // so the handshake refuses it and names the missing pins.
        let none = I2cClaim {
            addr: 0x76,
            scl_pin: None,
            sda_pin: None,
        };
        assert_eq!(
            Bme280I2c::default().claims(),
            [Claim {
                what: Some(claim::What::I2c(none))
            }]
        );
        let mut one_pin = Bme280I2c::default();
        one_pin.configure(&PartConfig {
            pins: [(SCL.to_string(), 6)].into(),
            ..Default::default()
        });
        let half = I2cClaim {
            addr: 0x76,
            scl_pin: Some(6),
            sda_pin: None,
        };
        assert_eq!(
            one_pin.claims(),
            [Claim {
                what: Some(claim::What::I2c(half))
            }]
        );
    }

    #[test]
    fn it_shows_the_air_around_it() {
        let mut chip = chip();
        chip.on_input("set:temperature=31.5");
        chip.on_input("set:pressure=1000");
        let shown = chip.state().unwrap();
        assert_eq!(
            (
                shown.get("temperature"),
                shown.get("pressure"),
                shown.get("humidity")
            ),
            (Some("31.5"), Some("1000"), Some("45"))
        );
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut req = request(0x76, &[ID], 1);
        req.correlation_id = 1;
        let reply = chip.on_bus(&req).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
