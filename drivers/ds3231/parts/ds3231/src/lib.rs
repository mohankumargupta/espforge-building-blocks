//! A Maxim DS3231 temperature-compensated real-time clock, written as a
//! virtual part.
//!
//! The firmware talks to it the way it would to the real chip: a register
//! address byte followed by a read or a write, seven binary-coded-decimal time
//! registers from 0x00, the alarm registers, the control and status bytes, and
//! the die temperature in quarter degrees at 0x11.
//!
//! The part holds one date and time, the alarms, the control and status bytes
//! and the temperature. It does not count seconds: a test that wants the clock
//! to move tells it the new time. README.md lists what is left out.
//!
//! `Ds3231` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as `{ pins: { SCL: 6, SDA: 5 } }`.
const SCL: &str = "SCL";
const SDA: &str = "SDA";

/// The DS3231's only I2C address.
const DEFAULT_ADDR: u32 = 0x68;

/// The seven time registers.
const SECONDS: u8 = 0x00;
const MINUTES: u8 = 0x01;
const HOURS: u8 = 0x02;
const WEEKDAY: u8 = 0x03;
const DATE: u8 = 0x04;
const MONTH: u8 = 0x05;
const YEAR: u8 = 0x06;

/// The alarm registers: alarm 1 at 0x07..=0x0A, alarm 2 at 0x0B..=0x0D.
const ALARMS: u8 = 0x07;
const ALARMS_LEN: usize = 7;

/// The control byte, which powers up at 0x1C: a 1 Hz square wave on INT/SQW
/// and the interrupt output enabled.
const CONTROL: u8 = 0x0E;
const CONTROL_POWER_ON: u8 = 0x1C;

/// The status byte: bit 7 is the oscillator-stop flag, bit 3 the 32 kHz
/// output, and the alarm flags in bits 1:0. Power-on is 0x88.
const STATUS: u8 = 0x0F;
const OSF: u8 = 0x80;
const STATUS_POWER_ON: u8 = 0x88;

/// The aging offset, which powers up at 0.
const AGING: u8 = 0x10;

/// The die temperature: the integer part and, in bits 7:6, the quarter
/// degrees.
const TEMP_MSB: u8 = 0x11;
const TEMP_LSB: u8 = 0x12;

/// Bit 7 of the month register: the year's century.
const CENTURY: u8 = 0x80;

/// The part's power-on time, 2026-01-01 00:00:00. A real chip powers up with
/// every register cleared, which the driver would still read as a time; a
/// fixed, readable one makes the fixture's output say what it is.
const POWER_ON_YEAR: u8 = 26;
const POWER_ON_MONTH: u8 = 1;
const POWER_ON_DATE: u8 = 1;
const POWER_ON_WEEKDAY: u8 = 1;

/// The die temperature when nobody has changed it, in degrees Celsius.
const DEFAULT_TEMPERATURE: f32 = 25.0;

/// The field ranges the datasheet gives.
const MIN_TEMPERATURE: f64 = -40.0;
const MAX_TEMPERATURE: f64 = 85.0;

struct Ds3231 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    scl: Option<u32>,
    sda: Option<u32>,
    /// The 7-bit address it answers at.
    addr: u32,
    /// Seconds, minutes, hours, date (day of month), month and year in BCD
    /// form, as the registers hold them.
    seconds: u8,
    minutes: u8,
    hours: u8,
    weekday: u8,
    date: u8,
    month: u8,
    year: u8,
    /// Bit 7 of the month register: the 2100s.
    century: bool,
    /// Registers 0x07..=0x0D: both alarms, as written.
    alarms: [u8; ALARMS_LEN],
    /// Register 0x0E.
    control: u8,
    /// Register 0x0F.
    status: u8,
    /// Register 0x10.
    aging: u8,
    /// What the die sees, in degrees Celsius.
    temperature: f32,
}

impl Default for Ds3231 {
    fn default() -> Self {
        Self {
            scl: None,
            sda: None,
            addr: DEFAULT_ADDR,
            seconds: 0,
            minutes: 0,
            hours: 0,
            weekday: POWER_ON_WEEKDAY,
            date: to_bcd(POWER_ON_DATE),
            month: to_bcd(POWER_ON_MONTH),
            year: to_bcd(POWER_ON_YEAR),
            century: false,
            alarms: [0; ALARMS_LEN],
            control: CONTROL_POWER_ON,
            status: STATUS_POWER_ON,
            aging: 0,
            temperature: DEFAULT_TEMPERATURE,
        }
    }
}

impl Device for Ds3231 {
    const NAME: &'static str = "ds3231";
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

    /// The address this clock answers for, on the two pins it is wired to.
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

    /// Move the clock or the die temperature: `set:year=<2000..2199>`,
    /// `set:month=<1..12>`, `set:day=<1..31>`, `set:weekday=<1..7>`,
    /// `set:hour=<0..23>`, `set:minute=<0..59>`, `set:second=<0..59>`,
    /// `set:temperature=<°C>`, or `rest` for the power-on values.
    fn on_input(&mut self, name: &str) {
        if let Some((what, value)) = set_value(name) {
            match what {
                "year" => {
                    let year = value.clamp(2000.0, 2199.0) as u16;
                    self.century = year >= 2100;
                    self.year = to_bcd((year % 100) as u8);
                }
                "month" => self.month = to_bcd(value.clamp(1.0, 12.0) as u8),
                "day" => self.date = to_bcd(value.clamp(1.0, 31.0) as u8),
                "weekday" => self.weekday = value.clamp(1.0, 7.0) as u8,
                "hour" => self.hours = to_bcd(value.clamp(0.0, 23.0) as u8),
                "minute" => self.minutes = to_bcd(value.clamp(0.0, 59.0) as u8),
                "second" => self.seconds = to_bcd(value.clamp(0.0, 59.0) as u8),
                "temperature" => {
                    self.temperature = value.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE) as f32
                }
                _ => {}
            }
        } else if name == "rest" {
            let defaults = Ds3231::default();
            self.seconds = defaults.seconds;
            self.minutes = defaults.minutes;
            self.hours = defaults.hours;
            self.weekday = defaults.weekday;
            self.date = defaults.date;
            self.month = defaults.month;
            self.year = defaults.year;
            self.century = defaults.century;
            self.temperature = defaults.temperature;
        }
    }

    /// The guest's I2C transaction: a register address, then a read or a
    /// write. A write of several bytes stores them in the registers that
    /// follow, and a read answers them in order.
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
        for (offset, &byte) in i2c.write[1..].iter().enumerate() {
            self.write_register(register.wrapping_add(offset as u8), byte);
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
                .set("datetime", self.datetime())
                .set("weekday", self.weekday)
                .set("temperature", self.temperature)
                .set("osf", u8::from(self.status & OSF != 0))
                .set("control", self.control)
                .set("status", self.status),
        )
    }
}

impl Ds3231 {
    /// The date and time as `YYYY-MM-DD HH:MM:SS`.
    fn datetime(&self) -> String {
        let year = 2000
            + u16::from(from_bcd(self.year))
            + if self.century { 100 } else { 0 };
        format!(
            "{year}-{:02}-{:02} {:02}:{:02}:{:02}",
            from_bcd(self.month),
            from_bcd(self.date),
            from_bcd(self.hours),
            from_bcd(self.minutes),
            from_bcd(self.seconds)
        )
    }

    /// One register's byte.
    fn register(&self, register: u8) -> u8 {
        match register {
            SECONDS => self.seconds,
            MINUTES => self.minutes,
            // Bit 6 clear: 24-hour mode.
            HOURS => self.hours,
            WEEKDAY => self.weekday,
            DATE => self.date,
            MONTH => self.month | if self.century { CENTURY } else { 0 },
            YEAR => self.year,
            ALARMS..=0x0D => self.alarms[usize::from(register - ALARMS)],
            CONTROL => self.control,
            STATUS => self.status,
            AGING => self.aging,
            // The fraction is added to the signed whole degrees, so the
            // quarters are the low two bits of the floor and the MSB is the
            // floor itself: -5.25 is -6 and 0b11.
            TEMP_MSB => (((self.temperature * 4.0).round() as i32) >> 2) as i8 as u8,
            TEMP_LSB => ((((self.temperature * 4.0).round() as i32) & 0b11) as u8) << 6,
            _ => 0x00,
        }
    }

    /// One register write. The time registers stay in BCD form, as written;
    /// the temperature registers are read-only.
    fn write_register(&mut self, register: u8, value: u8) {
        match register {
            SECONDS => self.seconds = value & 0x7F,
            MINUTES => self.minutes = value & 0x7F,
            // The part keeps 24-hour time; a 12-hour value is taken for bits
            // 4:0 and kept as written.
            HOURS => self.hours = value & 0x3F,
            WEEKDAY => self.weekday = value & 0x07,
            DATE => self.date = value & 0x3F,
            MONTH => {
                self.month = value & 0x1F;
                self.century = value & CENTURY != 0;
            }
            YEAR => self.year = value,
            ALARMS..=0x0D => self.alarms[usize::from(register - ALARMS)] = value,
            CONTROL => self.control = value,
            // OSF can be cleared by writing a 0 and cannot be set by writing a
            // 1: the datasheet's one read-only-ish bit.
            STATUS => {
                if value & OSF == 0 {
                    self.status &= !OSF;
                }
                self.status = (self.status & OSF) | (value & !OSF);
            }
            AGING => self.aging = value,
            _ => {}
        }
    }
}

/// A binary-coded-decimal byte.
fn to_bcd(value: u8) -> u8 {
    ((value / 10) << 4) | (value % 10)
}

/// The binary value of a binary-coded-decimal byte.
fn from_bcd(value: u8) -> u8 {
    (value >> 4) * 10 + (value & 0x0F)
}

// Big enough for the driver's requests and the host's frames.
esp_part::export_part!(Ds3231, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: SCL on GPIO 6, SDA on GPIO 5,
    /// at the part's only address.
    fn chip() -> Ds3231 {
        let mut chip = Ds3231::default();
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

    /// One transaction to the clock's address: the bytes read back, or `None`
    /// for a NACK.
    fn transfer(chip: &mut Ds3231, write: &[u8], read_len: u32) -> Option<Vec<u8>> {
        let reply = chip.on_bus(&request(chip.addr, write, read_len)).unwrap();
        reply.ack.then_some(reply.data)
    }

    /// The seven time registers, the way the driver reads them.
    fn time(chip: &mut Ds3231) -> Vec<u8> {
        transfer(chip, &[SECONDS], 7).expect("seven time bytes")
    }

    #[test]
    fn the_power_on_time_is_bcd() {
        let mut chip = chip();
        assert_eq!(
            time(&mut chip),
            [0x00, 0x00, 0x00, 0x01, 0x01, 0x01, 0x26],
            "seconds, minutes, hours, day, date, month, year"
        );
        assert_eq!(chip.datetime(), "2026-01-01 00:00:00");
    }

    #[test]
    fn a_write_of_several_bytes_stores_the_registers_that_follow() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[SECONDS, 0x56, 0x34, 0x12, 0x03, 0x18, 0x11, 0x26], 0),
            Some(vec![]),
            "one transaction of seven bytes"
        );
        assert_eq!(
            time(&mut chip),
            [0x56, 0x34, 0x12, 0x03, 0x18, 0x11, 0x26]
        );
        assert_eq!(chip.datetime(), "2026-11-18 12:34:56");
    }

    #[test]
    fn the_century_bit_is_part_of_the_month_register() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[MONTH, 0x05 | CENTURY], 0),
            Some(vec![])
        );
        assert_eq!(
            transfer(&mut chip, &[MONTH], 1),
            Some(vec![0x05 | CENTURY]),
            "it reads back set"
        );
        // 0x01 with the century bit is 2101, not 2001.
        chip.century = true;
        chip.year = 0x01;
        chip.date = 0x03;
        assert_eq!(chip.datetime(), "2101-05-03 00:00:00");
    }

    #[test]
    fn the_status_flag_can_be_cleared_but_not_set() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[STATUS], 1), Some(vec![0x88]));
        // Writing a 1 to OSF leaves it set; writing a 0 clears it.
        assert_eq!(transfer(&mut chip, &[STATUS, 0x88], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[STATUS], 1), Some(vec![0x88]));
        assert_eq!(transfer(&mut chip, &[STATUS, 0x08], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[STATUS], 1), Some(vec![0x08]));
    }

    #[test]
    fn the_control_byte_powers_up_at_0x1c() {
        let mut chip = chip();
        assert_eq!(transfer(&mut chip, &[CONTROL], 1), Some(vec![0x1C]));
        assert_eq!(
            transfer(&mut chip, &[CONTROL, 0x00], 0),
            Some(vec![]),
            "the driver may turn the square wave off"
        );
        assert_eq!(transfer(&mut chip, &[CONTROL], 1), Some(vec![0x00]));
    }

    #[test]
    fn the_alarms_and_the_aging_offset_are_plain_storage() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[ALARMS, 1, 2, 3, 4, 5, 6, 7], 0),
            Some(vec![])
        );
        assert_eq!(transfer(&mut chip, &[ALARMS], 7), Some(vec![1, 2, 3, 4, 5, 6, 7]));
        assert_eq!(transfer(&mut chip, &[AGING, 0x12], 0), Some(vec![]));
        assert_eq!(transfer(&mut chip, &[AGING], 1), Some(vec![0x12]));
    }

    #[test]
    fn the_temperature_is_quarter_degrees() {
        let mut chip = chip();
        assert_eq!(
            transfer(&mut chip, &[TEMP_MSB], 2),
            Some(vec![0x19, 0x00]),
            "25.00 degrees"
        );
        chip.on_input("set:temperature=21.5");
        assert_eq!(
            transfer(&mut chip, &[TEMP_MSB], 2),
            Some(vec![0x15, 0x80]),
            "21.50 degrees is 21 and two quarters"
        );
        chip.on_input("set:temperature=-5.25");
        assert_eq!(
            transfer(&mut chip, &[TEMP_MSB], 2),
            Some(vec![0xFA, 0xC0]),
            "-5.25 degrees is -6 whole degrees and three quarters"
        );
    }

    #[test]
    fn the_inputs_move_the_clock() {
        let mut chip = chip();
        chip.on_input("set:year=2030");
        chip.on_input("set:month=6");
        chip.on_input("set:day=15");
        chip.on_input("set:weekday=3");
        chip.on_input("set:hour=8");
        chip.on_input("set:minute=30");
        chip.on_input("set:second=0");
        assert_eq!(chip.datetime(), "2030-06-15 08:30:00");
        assert_eq!(chip.weekday, 3);
        assert_eq!(
            time(&mut chip),
            [0x00, 0x30, 0x08, 0x03, 0x15, 0x06, 0x30]
        );
    }

    #[test]
    fn the_inputs_stop_at_the_field_ranges() {
        let mut chip = chip();
        chip.on_input("set:year=1999");
        chip.on_input("set:month=0");
        chip.on_input("set:day=32");
        chip.on_input("set:weekday=9");
        chip.on_input("set:hour=24");
        chip.on_input("set:minute=-5");
        chip.on_input("set:second=99");
        chip.on_input("set:temperature=-100");
        assert_eq!(chip.datetime(), "2000-01-31 23:00:59");
        assert_eq!(chip.weekday, 7);
        assert_eq!(chip.temperature, MIN_TEMPERATURE as f32);
    }

    #[test]
    fn rest_puts_the_power_on_time_back() {
        let mut chip = chip();
        chip.on_input("set:year=2030");
        chip.on_input("set:hour=8");
        chip.on_input("set:temperature=30");
        chip.on_input("rest");
        assert_eq!(chip.datetime(), "2026-01-01 00:00:00");
        assert_eq!(chip.temperature, DEFAULT_TEMPERATURE);
    }

    #[test]
    fn it_claims_its_address_on_the_pins_it_is_wired_to() {
        let claim = Claim {
            what: Some(claim::What::I2c(I2cClaim {
                addr: 0x68,
                scl_pin: Some(6),
                sda_pin: Some(5),
            })),
        };
        assert_eq!(chip().claims(), [claim]);
    }

    #[test]
    fn it_shows_the_clock_and_the_oscillator_flag() {
        let mut chip = chip();
        assert_eq!(chip.state().unwrap().get("datetime"), Some("2026-01-01 00:00:00"));
        assert_eq!(chip.state().unwrap().get("osf"), Some("1"));
        let _ = transfer(&mut chip, &[STATUS, 0x08], 0);
        assert_eq!(
            chip.state().unwrap().get("osf"),
            Some("0"),
            "the driver cleared it at construction"
        );
    }

    #[test]
    fn a_transaction_at_another_address_is_nacked() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(0x69, &[SECONDS], 1)).unwrap();
        assert!(!reply.ack, "nothing answers at 0x69");
        assert!(reply.data.is_empty());
    }

    #[test]
    fn the_reply_echoes_the_correlation_id_and_names_i2c() {
        let mut chip = chip();
        let mut request = request(0x68, &[SECONDS], 1);
        request.correlation_id = 1;
        let reply = chip.on_bus(&request).unwrap();
        assert_eq!(
            (reply.correlation_id, reply.op, reply.ack),
            (1, BusOp::I2c as i32, true)
        );
    }
}
