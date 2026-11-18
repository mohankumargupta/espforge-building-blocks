//! An AMS CCS811 eCO2 and TVOC air quality sensor, over any `embedded-hal`
//! I2C bus.
//!
//! [`Ccs811::new`] reads the hardware id (`0x81` on every CCS811), checks that
//! the part reports `APP_VALID` and no error, starts the application firmware
//! with `APP_START` if the part is still in boot mode, and then asks for a
//! measurement a second through `MEAS_MODE`.
//!
//! [`Ccs811::read`] checks the status byte and reads the four result bytes:
//! equivalent CO2 in ppm and total volatile organic compounds in ppb, both
//! big-endian 16-bit values.
//!
//! The endpoint is generic, so espforge's `ConfiguredI2cDevice`, the
//! emulator's virtual part and a host test double all take the same code.
//!
//! Only the happy path is here: no raw data, no environmental compensation, no
//! baseline save or restore, no threshold interrupts, no sleep/wake. See the
//! fixture's `README.md` for what is left out.

#![no_std]

// The crate is `no_std`, but its tests run on the host and keep a register
// file in an array.
#[cfg(test)]
extern crate std;

use embedded_hal::i2c::I2c;

/// The CCS811's address with ADDR low; ADDR high is 0x5B.
pub const ADDRESS: u8 = 0x5A;

/// The value every CCS811 answers at the hardware id register.
pub const HW_ID: u8 = 0x81;

const STATUS: u8 = 0x00;
const MEAS_MODE: u8 = 0x01;
const ALG_RESULT_DATA: u8 = 0x02;
const HW_ID_REGISTER: u8 = 0x20;
const APP_START: u8 = 0xF4;

/// Status bit 0: the part reported an error; `ERROR_ID` says which.
const STATUS_ERROR: u8 = 0b0000_0001;
/// Status bit 3: a new measurement is waiting in `ALG_RESULT_DATA`.
const STATUS_DATA_READY: u8 = 0b0000_1000;
/// Status bit 4: the application firmware is present.
const STATUS_APP_VALID: u8 = 0b0001_0000;
/// Status bit 7: the application firmware is running, not the bootloader.
const STATUS_FW_MODE: u8 = 0b1000_0000;

/// Drive mode 1 in bits 6:4: one measurement a second.
const DRIVE_MODE_1S: u8 = 0b001 << 4;

/// One result from the part's on-chip algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Reading {
    /// Equivalent CO2, in parts per million.
    pub eco2_ppm: u16,
    /// Total volatile organic compounds, in parts per billion.
    pub tvoc_ppb: u16,
}

/// Why the part could not be used, or a transfer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ccs811Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The hardware id was not a CCS811's.
    WrongChipId(u8),
    /// The part has no valid application firmware, or would not leave boot
    /// mode after `APP_START`.
    NotApplicationMode,
    /// The part's error bit is set.
    DeviceError,
    /// No measurement is waiting yet.
    NotReady,
}

/// A CCS811 on an I2C bus.
pub struct Ccs811<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C: I2c> Ccs811<I2C> {
    /// Identify the part, leave boot mode if needed, and start measuring.
    ///
    /// The sensor's first measurement takes a second, which the caller waits
    /// for; [`Ccs811::read`] reports `NotReady` until it has arrived.
    pub fn new(i2c: I2C, address: u8) -> Result<Self, Ccs811Error<I2C::Error>> {
        let mut this = Self { i2c, address };

        let mut id = [0u8; 1];
        this.i2c
            .write_read(address, &[HW_ID_REGISTER], &mut id)
            .map_err(Ccs811Error::Bus)?;
        if id[0] != HW_ID {
            return Err(Ccs811Error::WrongChipId(id[0]));
        }

        let status = this.status()?;
        if status & STATUS_ERROR != 0 {
            return Err(Ccs811Error::DeviceError);
        }
        if status & STATUS_APP_VALID == 0 {
            return Err(Ccs811Error::NotApplicationMode);
        }
        if status & STATUS_FW_MODE == 0 {
            // APP_START is the register address on its own: it takes no data.
            this.i2c
                .write(address, &[APP_START])
                .map_err(Ccs811Error::Bus)?;
            if this.status()? & STATUS_FW_MODE == 0 {
                return Err(Ccs811Error::NotApplicationMode);
            }
        }

        this.i2c
            .write(address, &[MEAS_MODE, DRIVE_MODE_1S])
            .map_err(Ccs811Error::Bus)?;
        Ok(this)
    }

    /// The address the driver talks to.
    pub fn address(&self) -> u8 {
        self.address
    }

    /// The status byte: error, data ready, application valid, firmware mode.
    pub fn status(&mut self) -> Result<u8, Ccs811Error<I2C::Error>> {
        let mut status = [0u8; 1];
        self.i2c
            .write_read(self.address, &[STATUS], &mut status)
            .map_err(Ccs811Error::Bus)?;
        Ok(status[0])
    }

    /// Read the latest eCO2 and TVOC result.
    ///
    /// The status is checked first: an error is reported as `DeviceError` and
    /// a measurement that has not arrived yet as `NotReady`.
    pub fn read(&mut self) -> Result<Reading, Ccs811Error<I2C::Error>> {
        let status = self.status()?;
        if status & STATUS_ERROR != 0 {
            return Err(Ccs811Error::DeviceError);
        }
        if status & STATUS_DATA_READY == 0 {
            return Err(Ccs811Error::NotReady);
        }

        let mut raw = [0u8; 4];
        self.i2c
            .write_read(self.address, &[ALG_RESULT_DATA], &mut raw)
            .map_err(Ccs811Error::Bus)?;
        Ok(Reading {
            eco2_ppm: u16::from_be_bytes([raw[0], raw[1]]),
            tvoc_ppb: u16::from_be_bytes([raw[2], raw[3]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::i2c::{ErrorType, Operation};
    use std::vec::Vec;

    /// The part reduced to the registers the driver touches.
    struct Fake {
        registers: [u8; 256],
        pointer: u8,
        /// The data writes only, so a read's register pointer is not one.
        writes: Vec<Vec<u8>>,
        /// How many times `APP_START` was sent.
        app_starts: u32,
        /// Whether `APP_START` actually runs the application.
        app_start_works: bool,
    }

    impl Fake {
        /// The power-on part: hardware id present, application valid but not
        /// running (boot mode), and 400 ppm with 8 ppb in the air.
        fn new() -> Self {
            let mut fake = Self {
                registers: [0u8; 256],
                pointer: 0,
                writes: Vec::new(),
                app_starts: 0,
                app_start_works: true,
            };
            fake.registers[usize::from(HW_ID_REGISTER)] = HW_ID;
            fake.registers[usize::from(STATUS)] = STATUS_APP_VALID;
            fake.set_reading(400, 8);
            fake
        }

        /// Store one result, in the register layout the part uses.
        fn set_reading(&mut self, eco2: u16, tvoc: u16) {
            self.registers[usize::from(ALG_RESULT_DATA)] = (eco2 >> 8) as u8;
            self.registers[usize::from(ALG_RESULT_DATA) + 1] = eco2 as u8;
            self.registers[usize::from(ALG_RESULT_DATA) + 2] = (tvoc >> 8) as u8;
            self.registers[usize::from(ALG_RESULT_DATA) + 3] = tvoc as u8;
        }
    }

    impl ErrorType for Fake {
        type Error = Infallible;
    }

    impl I2c for Fake {
        /// A one-byte write is the register pointer for the read that may
        /// follow, or the `APP_START` command. A longer write stores its data
        /// byte; a non-idle drive mode starts measuring, which is what makes
        /// `DATA_READY` set.
        fn transaction(
            &mut self,
            _address: u8,
            operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            for operation in operations {
                match operation {
                    Operation::Write(bytes) => {
                        let Some(&first) = bytes.first() else { continue };
                        self.pointer = first;
                        if bytes.len() > 1 {
                            self.writes.push(bytes.to_vec());
                            self.registers[usize::from(first)] = bytes[1];
                            if first == MEAS_MODE && bytes[1] & 0b0111_0000 != 0 {
                                self.registers[usize::from(STATUS)] |= STATUS_DATA_READY;
                            }
                        } else if first == APP_START {
                            self.app_starts += 1;
                            if self.app_start_works {
                                self.registers[usize::from(STATUS)] |= STATUS_FW_MODE;
                            }
                        }
                    }
                    Operation::Read(read) => {
                        for (offset, byte) in read.iter_mut().enumerate() {
                            let register = usize::from(self.pointer) + offset;
                            *byte = self.registers[register];
                        }
                    }
                }
            }
            Ok(())
        }
    }

    fn sensor() -> Ccs811<Fake> {
        Ccs811::new(Fake::new(), ADDRESS).expect("a CCS811")
    }

    #[test]
    fn it_selects_the_part_and_starts_measuring() {
        let sensor = sensor();
        assert_eq!(sensor.address(), 0x5A);
        assert_eq!(sensor.i2c.app_starts, 1, "APP_START leaves boot mode");
        assert_eq!(
            sensor.i2c.writes,
            [Vec::from([MEAS_MODE, DRIVE_MODE_1S])],
            "one measurement a second"
        );
        assert_eq!(
            sensor.i2c.registers[usize::from(STATUS)] & STATUS_FW_MODE,
            STATUS_FW_MODE,
            "the application firmware runs"
        );
    }

    #[test]
    fn a_running_part_is_not_restarted() {
        let mut fake = Fake::new();
        fake.registers[usize::from(STATUS)] = STATUS_APP_VALID | STATUS_FW_MODE;
        let sensor = Ccs811::new(fake, ADDRESS).unwrap();
        assert_eq!(sensor.i2c.app_starts, 0, "no APP_START was needed");
        assert_eq!(sensor.i2c.writes, [Vec::from([MEAS_MODE, DRIVE_MODE_1S])]);
    }

    #[test]
    fn a_measurement_decodes_to_ppm_and_ppb() {
        let mut sensor = sensor();
        assert_eq!(
            sensor.read().unwrap(),
            Reading {
                eco2_ppm: 400,
                tvoc_ppb: 8,
            }
        );
        sensor.i2c.set_reading(800, 50);
        assert_eq!(
            sensor.read().unwrap(),
            Reading {
                eco2_ppm: 800,
                tvoc_ppb: 50,
            }
        );
    }

    #[test]
    fn the_result_bytes_are_big_endian() {
        let mut sensor = sensor();
        sensor.i2c.set_reading(0x1234, 0x0008);
        assert_eq!(sensor.read().unwrap().eco2_ppm, 0x1234);
    }

    #[test]
    fn a_wrong_hardware_id_is_reported() {
        let mut fake = Fake::new();
        fake.registers[usize::from(HW_ID_REGISTER)] = 0x80; // a CCS801
        assert_eq!(
            Ccs811::new(fake, ADDRESS).err(),
            Some(Ccs811Error::WrongChipId(0x80))
        );
    }

    #[test]
    fn a_part_without_an_application_is_refused() {
        let mut fake = Fake::new();
        fake.registers[usize::from(STATUS)] = 0x00;
        assert_eq!(
            Ccs811::new(fake, ADDRESS).err(),
            Some(Ccs811Error::NotApplicationMode)
        );
    }

    #[test]
    fn a_part_that_stays_in_boot_mode_is_refused() {
        let mut fake = Fake::new();
        fake.app_start_works = false;
        assert_eq!(
            Ccs811::new(fake, ADDRESS).err(),
            Some(Ccs811Error::NotApplicationMode),
            "APP_START was sent but FW_MODE stayed clear"
        );
    }

    #[test]
    fn the_error_bit_is_reported() {
        let mut fake = Fake::new();
        fake.registers[usize::from(STATUS)] = STATUS_APP_VALID | STATUS_FW_MODE | STATUS_ERROR;
        assert_eq!(
            Ccs811::new(fake, ADDRESS).err(),
            Some(Ccs811Error::DeviceError)
        );

        let mut sensor = sensor();
        sensor.i2c.registers[usize::from(STATUS)] |= STATUS_ERROR;
        assert_eq!(sensor.read().err(), Some(Ccs811Error::DeviceError));
    }

    #[test]
    fn a_measurement_that_has_not_arrived_is_not_ready() {
        let mut sensor = sensor();
        sensor.i2c.registers[usize::from(STATUS)] &= !STATUS_DATA_READY;
        assert_eq!(sensor.read().err(), Some(Ccs811Error::NotReady));
    }
}
