//! An InvenSense MPU-9250 accelerometer and gyro over any `embedded-hal` SPI
//! device.
//!
//! The chip's SPI protocol is one address byte per transfer: bit 7 set for a
//! read and bits 6:0 the register, with **no dummy byte**. The address
//! increments across a read while CS stays low, so one transaction takes all
//! fourteen measurement bytes. `WHO_AM_I` (0x75) answers 0x71;
//! [`Mpu9250::new`] refuses anything else with
//! [`Mpu9250Error::WrongDevice`] so the boot log names the part.
//!
//! [`Mpu9250::read`] decodes the big-endian 16-bit pairs into `f32` g, dps and
//! °C at the power-on ranges: 16384 LSB/g (±2 g), 131 LSB/dps (±250 dps) and
//! 333.87 LSB/°C about 21 °C.
//!
//! The magnetometer is not here: the AK8963 is a second die on the MPU-9250's
//! auxiliary I2C bus, reachable over SPI only through the chip's internal I2C
//! master or a bypassed second bus from the MCU. This driver owns one SPI
//! endpoint and the accelerometer/gyro die behind it.
//!
//! The endpoint is generic, so espforge's `ConfiguredSpiDevice`, the emulator's
//! virtual part and a host test double all take the same code.

#![no_std]

use embedded_hal::spi::{Operation, SpiDevice};

// The crate is `no_std`, but its tests run on the host and keep the register
// file and the frames in vectors.
#[cfg(test)]
extern crate std;

/// The registers the driver touches, by datasheet address.
const SMPLRT_DIV: u8 = 0x19;
const CONFIG: u8 = 0x1A;
const GYRO_CONFIG: u8 = 0x1B;
const ACCEL_CONFIG: u8 = 0x1C;
const ACCEL_CONFIG2: u8 = 0x1D;
const ACCEL_XOUT_H: u8 = 0x3B;
const GYRO_ZOUT_L: u8 = 0x48;
const PWR_MGMT_1: u8 = 0x6B;
const WHO_AM_I: u8 = 0x75;

/// Read bit: set for a read, clear for a write. Bits 6:0 are the register.
const READ: u8 = 0x80;

/// `PWR_MGMT_1`'s `DEVICE_RESET`: set by a write, cleared by the chip when the
/// reset is done.
const DEVICE_RESET: u8 = 0x80;
/// `PWR_MGMT_1` awake on the X-gyro PLL (`CLKSEL = 1`).
const CLOCK_PLL: u8 = 0x01;

/// `WHO_AM_I` on an MPU-9250. 0x70 is an MPU-6500 and 0x73 an MPU-9255.
pub const WHO_AM_I_VALUE: u8 = 0x71;

/// The power-on scales the driver decodes with.
const ACCEL_LSB_PER_G: f32 = 16384.0;
const GYRO_LSB_PER_DPS: f32 = 131.0;
const TEMP_LSB_PER_C: f32 = 333.87;
const TEMP_OFFSET_C: f32 = 21.0;

/// The fourteen bytes of one measurement: three acceleration pairs, the
/// temperature, then three rotation pairs.
const DATA_LEN: usize = (GYRO_ZOUT_L - ACCEL_XOUT_H + 1) as usize;

/// Reads of `PWR_MGMT_1` before a reset that never finishes is given up on.
/// The chip clears the bit within a few milliseconds; this is the bound that
/// keeps a dead part from hanging the firmware (a blocking delay is not part
/// of espforge's endpoint).
const POLL_ATTEMPTS: u32 = 200;

/// One measurement, in physical units.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Reading {
    /// Acceleration on X, in g.
    pub ax_g: f32,
    /// Acceleration on Y, in g.
    pub ay_g: f32,
    /// Acceleration on Z, in g.
    pub az_g: f32,
    /// Rotation about X, in degrees per second.
    pub gx_dps: f32,
    /// Rotation about Y, in degrees per second.
    pub gy_dps: f32,
    /// Rotation about Z, in degrees per second.
    pub gz_dps: f32,
    /// The die temperature, in degrees Celsius.
    pub temperature_c: f32,
}

/// Why a transfer, an identity check or a reset failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mpu9250Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The part answered, but not with an MPU-9250's identity.
    WrongDevice(u8),
    /// The answer was shorter than the data block it should carry.
    Short,
    /// `DEVICE_RESET` never cleared: the part is not finishing its reset.
    Stuck,
}

/// An MPU-9250 on an SPI device endpoint.
pub struct Mpu9250<SPI> {
    spi: SPI,
}

impl<SPI: SpiDevice> Mpu9250<SPI> {
    /// Reset the part, check its identity, apply the configuration and wake it
    /// on the X-gyro PLL.
    ///
    /// The reset is the chip's own (`PWR_MGMT_1 = 0x80`) and is polled until it
    /// clears, so no delay parameter is needed. A part that does not answer
    /// 0x71 to `WHO_AM_I` is `Mpu9250Error::WrongDevice`, so the boot log names
    /// the device instead of reporting an IMU that never moves.
    pub fn new(spi: SPI) -> Result<Self, Mpu9250Error<SPI::Error>> {
        let mut this = Self { spi };
        this.write_register(PWR_MGMT_1, DEVICE_RESET)?;

        let mut power = [0u8; 1];
        let mut cleared = false;
        for _ in 0..POLL_ATTEMPTS {
            this.read_register(PWR_MGMT_1, &mut power)?;
            if power[0] & DEVICE_RESET == 0 {
                cleared = true;
                break;
            }
        }
        if !cleared {
            return Err(Mpu9250Error::Stuck);
        }

        let mut who = [0u8; 1];
        this.read_register(WHO_AM_I, &mut who)?;
        if who[0] != WHO_AM_I_VALUE {
            return Err(Mpu9250Error::WrongDevice(who[0]));
        }

        // 1 kHz / (1 + 4) = 200 Hz, a 3-byte gyro and accelerometer digital
        // low-pass at 41 Hz, the power-on ±250 dps and ±2 g ranges.
        this.write_register(SMPLRT_DIV, 0x04)?;
        this.write_register(CONFIG, 0x03)?;
        this.write_register(ACCEL_CONFIG2, 0x03)?;
        this.write_register(GYRO_CONFIG, 0x00)?;
        this.write_register(ACCEL_CONFIG, 0x00)?;
        // Wake up and take the X-gyro PLL as the clock source.
        this.write_register(PWR_MGMT_1, CLOCK_PLL)?;
        Ok(this)
    }

    /// The part's identity: 0x71 on an MPU-9250.
    pub fn who_am_i(&mut self) -> Result<u8, Mpu9250Error<SPI::Error>> {
        let mut who = [0u8; 1];
        self.read_register(WHO_AM_I, &mut who)?;
        Ok(who[0])
    }

    /// One measurement, from all fourteen bytes in a single transaction.
    pub fn read(&mut self) -> Result<Reading, Mpu9250Error<SPI::Error>> {
        let mut raw = [0u8; DATA_LEN];
        self.read_registers(ACCEL_XOUT_H, &mut raw)?;
        Ok(Reading {
            ax_g: accel(raw[0], raw[1]),
            ay_g: accel(raw[2], raw[3]),
            az_g: accel(raw[4], raw[5]),
            temperature_c: temperature(raw[6], raw[7]),
            gx_dps: gyro(raw[8], raw[9]),
            gy_dps: gyro(raw[10], raw[11]),
            gz_dps: gyro(raw[12], raw[13]),
        })
    }

    /// A register read: the address byte carries the register with the read
    /// bit, and there is no dummy byte. The address increments across the
    /// answer.
    fn read_register(
        &mut self,
        register: u8,
        buf: &mut [u8],
    ) -> Result<(), Mpu9250Error<SPI::Error>> {
        self.read_registers(register, buf)
    }

    /// `len` registers from `register` in one transaction.
    fn read_registers(
        &mut self,
        register: u8,
        buf: &mut [u8],
    ) -> Result<(), Mpu9250Error<SPI::Error>> {
        let address = [(register & 0x7F) | READ];
        self.spi
            .transaction(&mut [Operation::Write(&address), Operation::Read(buf)])
            .map_err(Mpu9250Error::Bus)
    }

    /// A register write: the address byte, then the value.
    fn write_register(
        &mut self,
        register: u8,
        value: u8,
    ) -> Result<(), Mpu9250Error<SPI::Error>> {
        let frame = [register & 0x7F, value];
        self.spi
            .transaction(&mut [Operation::Write(&frame)])
            .map_err(Mpu9250Error::Bus)
    }
}

/// One acceleration pair at 16384 LSB/g.
fn accel(high: u8, low: u8) -> f32 {
    i16::from_be_bytes([high, low]) as f32 / ACCEL_LSB_PER_G
}

/// One rotation pair at 131 LSB/dps.
fn gyro(high: u8, low: u8) -> f32 {
    i16::from_be_bytes([high, low]) as f32 / GYRO_LSB_PER_DPS
}

/// The temperature pair: 333.87 LSB/°C, zero at 21 °C.
fn temperature(high: u8, low: u8) -> f32 {
    i16::from_be_bytes([high, low]) as f32 / TEMP_LSB_PER_C + TEMP_OFFSET_C
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use std::vec::Vec;

    /// A register-file MPU-9250 written against the datasheet rather than by
    /// reusing the driver's own code: the reset clears itself, `WHO_AM_I` is
    /// configurable and the data block is a fixed byte pattern.
    struct Fake {
        regs: [u8; 128],
        data: [u8; DATA_LEN],
        who_am_i: u8,
        /// An `END-DEVICE-RESET` that never clears, as a held part would.
        reset_stuck: bool,
        /// Every register write, by datasheet address.
        writes: Vec<(u8, u8)>,
        /// Every read: the wire address byte and how many bytes it asked for.
        reads: Vec<(u8, usize)>,
    }

    impl Fake {
        fn new() -> Self {
            let mut regs = [0u8; 128];
            // PWR_MGMT_1 powers up 0x40: asleep on the internal clock.
            regs[PWR_MGMT_1 as usize] = 0x40;
            Self {
                regs,
                data: [0u8; DATA_LEN],
                who_am_i: WHO_AM_I_VALUE,
                reset_stuck: false,
                writes: Vec::new(),
                reads: Vec::new(),
            }
        }

        fn write_register(&mut self, register: u8, value: u8) {
            self.writes.push((register, value));
            let register = (register & 0x7F) as usize;
            if register == PWR_MGMT_1 as usize && value & DEVICE_RESET != 0 {
                // A healthy part clears the bit to `SLEEP`; a stuck one leaves
                // it set.
                self.regs[register] = if self.reset_stuck { DEVICE_RESET } else { 0x40 };
                return;
            }
            self.regs[register] = value;
        }

        fn read_register(&mut self, register: u8) -> u8 {
            match register & 0x7F {
                WHO_AM_I => self.who_am_i,
                register @ ACCEL_XOUT_H..=GYRO_ZOUT_L => {
                    self.data[(register - ACCEL_XOUT_H) as usize]
                }
                register => self.regs[register as usize],
            }
        }
    }

    impl embedded_hal::spi::ErrorType for Fake {
        type Error = Infallible;
    }

    impl SpiDevice for Fake {
        fn transaction(
            &mut self,
            operations: &mut [Operation<'_, u8>],
        ) -> Result<(), Self::Error> {
            let mut command = None;
            for operation in operations {
                match operation {
                    Operation::Write(bytes) => {
                        let Some(&first) = bytes.first() else { continue };
                        if first & READ != 0 {
                            command = Some(first);
                        } else if let Some(&value) = bytes.get(1) {
                            self.write_register(first, value);
                        }
                    }
                    Operation::Read(buf) => {
                        let address = command.take().unwrap_or(0);
                        self.reads.push((address, buf.len()));
                        let mut register = address & 0x7F;
                        for byte in buf.iter_mut() {
                            *byte = self.read_register(register);
                            register = register.wrapping_add(1);
                        }
                    }
                    _ => {}
                }
            }
            Ok(())
        }
    }

    fn sensor() -> Mpu9250<Fake> {
        Mpu9250::new(Fake::new()).expect("an MPU-9250")
    }

    #[test]
    fn it_resets_then_checks_the_identity() {
        let sensor = sensor();
        assert_eq!(
            sensor.spi.writes[0],
            (PWR_MGMT_1, DEVICE_RESET),
            "the device reset comes first"
        );
        assert_eq!(
            sensor.spi.reads[0],
            (PWR_MGMT_1 | READ, 1),
            "then the reset status, at 0xEB"
        );
        assert_eq!(
            sensor.spi.reads[1],
            (WHO_AM_I | READ, 1),
            "then the identity, at 0xF5"
        );
    }

    #[test]
    fn it_accepts_an_mpu9250_and_refuses_the_look_alikes() {
        assert!(Mpu9250::new(Fake::new()).is_ok());
        for other in [0x70u8, 0x73, 0x00, 0xFF] {
            let mut fake = Fake::new();
            fake.who_am_i = other;
            assert_eq!(
                Mpu9250::new(fake).err(),
                Some(Mpu9250Error::WrongDevice(other)),
                "0x{other:02x} is not an MPU-9250"
            );
        }
    }

    #[test]
    fn it_gives_up_on_a_reset_that_never_clears() {
        let mut fake = Fake::new();
        fake.reset_stuck = true;
        assert_eq!(Mpu9250::new(fake).err(), Some(Mpu9250Error::Stuck));
    }

    #[test]
    fn it_applies_the_configuration_at_the_datasheet_addresses() {
        let sensor = sensor();
        assert_eq!(
            &sensor.spi.writes[1..],
            &[
                (SMPLRT_DIV, 0x04),
                (CONFIG, 0x03),
                (ACCEL_CONFIG2, 0x03),
                (GYRO_CONFIG, 0x00),
                (ACCEL_CONFIG, 0x00),
                (PWR_MGMT_1, CLOCK_PLL),
            ],
            "six writes, then awake on the X-gyro PLL"
        );
        let regs = &sensor.spi.regs;
        assert_eq!(regs[SMPLRT_DIV as usize], 0x04);
        assert_eq!(regs[CONFIG as usize], 0x03);
        assert_eq!(regs[ACCEL_CONFIG2 as usize], 0x03);
        assert_eq!(regs[PWR_MGMT_1 as usize], 0x01);
    }

    #[test]
    fn it_reads_the_whole_data_block_in_one_transaction() {
        let mut sensor = sensor();
        sensor.read().unwrap();
        assert_eq!(
            sensor.spi.reads.last(),
            Some(&(0xBB, DATA_LEN)),
            "0x3B with the read bit, fourteen bytes"
        );
        let reads_before = sensor.spi.reads.len();
        sensor.read().unwrap();
        assert_eq!(
            sensor.spi.reads.len(),
            reads_before + 1,
            "one transaction per reading, not one per byte"
        );
    }

    #[test]
    fn it_decodes_the_pairs_into_g_dps_and_celsius() {
        let mut fake = Fake::new();
        fake.data = [
            0x20, 0x00, // ax = 8192 = 0.5 g
            0xC0, 0x00, // ay = -16384 = -1 g
            0x40, 0x00, // az = 16384 = 1 g
            0x00, 0x00, // 21 °C
            0x2E, 0x0E, // gx = 11790 = 90 dps
            0xFA, 0xE2, // gy = -1310 = -10 dps
            0x00, 0x00, // gz = 0 dps
        ];
        let mut sensor = Mpu9250::new(fake).unwrap();
        assert_eq!(
            sensor.read().unwrap(),
            Reading {
                ax_g: 0.5,
                ay_g: -1.0,
                az_g: 1.0,
                gx_dps: 90.0,
                gy_dps: -10.0,
                gz_dps: 0.0,
                temperature_c: 21.0,
            }
        );
    }

    #[test]
    fn the_decoders_are_the_inverse_of_the_datasheet_scales() {
        assert_eq!(accel(0x40, 0x00), 1.0);
        assert_eq!(accel(0xC0, 0x00), -1.0);
        assert_eq!(gyro(0x2E, 0x0E), 90.0);
        assert_eq!(temperature(0x00, 0x00), 21.0);
        // 0x0537 = 1335 counts. The encode rounds, so 25 °C comes back within
        // a hundredth of a degree, not bit for bit: 1335 / 333.87 + 21.
        assert!((temperature(0x05, 0x37) - 25.0).abs() < 0.01);
        assert_eq!(WHO_AM_I_VALUE, 0x71);
    }

    #[test]
    fn the_identity_read_masks_the_address() {
        // 0x75 already has bit 6 set; the wire byte is the register with the
        // read bit, not a shifted address.
        let mut sensor = sensor();
        assert_eq!(sensor.who_am_i().unwrap(), 0x71);
        assert_eq!(sensor.spi.reads.last(), Some(&(0xF5, 1)));
    }
}
