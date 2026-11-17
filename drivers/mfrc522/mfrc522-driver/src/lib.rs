//! An NXP MFRC522 contactless reader over any `embedded-hal` SPI device.
//!
//! The reader's SPI protocol is one address byte per transfer:
//! `register << 1`, with bit 7 set for a read and bit 0 always 0, and no dummy
//! byte. A read auto-increments across the register file. `VersionReg` (0x37)
//! answers 0x92 on a v2.0 reader and 0x91 on some clones; [`Mfrc522::new`]
//! accepts both and refuses anything else with
//! [`Mfrc522Error::WrongVersion`].
//!
//! [`Mfrc522::read_tag`] runs the three frames ISO/IEC 14443-3 gives for a card
//! selection: `REQA` as a 7-bit frame, anticollision CL1 (`93 20`) and select
//! CL1 (`93 70` plus the UID, its BCC and CRC_A). The answer is the card's
//! ATQA, its four-byte UID and its SAK, or `None` when no card answers.
//!
//! The endpoint is generic, so espforge's `ConfiguredSpiDevice`, the emulator's
//! virtual part and a host test double all take the same code.

#![no_std]

use embedded_hal::spi::{Operation, SpiDevice};

// The crate is `no_std`, but its tests run on the host and keep the frames and
// the register file in vectors.
#[cfg(test)]
extern crate std;

/// The registers the driver touches, by datasheet address.
const COMMAND: u8 = 0x01;
const COM_IRQ: u8 = 0x04;
const ERROR: u8 = 0x06;
const FIFO_DATA: u8 = 0x09;
const FIFO_LEVEL: u8 = 0x0A;
const BIT_FRAMING: u8 = 0x0D;
const MODE: u8 = 0x11;
const TX_CONTROL: u8 = 0x14;
const TX_ASK: u8 = 0x15;
const T_MODE: u8 = 0x2A;
const T_PRESCALER: u8 = 0x2B;
const T_RELOAD_H: u8 = 0x2C;
const T_RELOAD_L: u8 = 0x2D;
const STATUS2: u8 = 0x08;
const VERSION: u8 = 0x37;

/// Read bit: set for a read, clear for a write.
const READ: u8 = 0x80;

/// `CommandReg`: the soft reset and the Transceive command.
const SOFT_RESET: u8 = 0x0F;
const TRANSCEIVE: u8 = 0x0C;

/// `FIFOLevelReg`'s flush bit: writing it empties the FIFO.
const FIFO_FLUSH: u8 = 0x80;

/// `ComIrqReg`: a card answered, or the timer ran out.
const IRQ_RX: u8 = 0x20;
const IRQ_TIMER: u8 = 0x01;

/// `VersionReg` on a v2.0 reader, and on the clones that answer 0x91.
pub const VERSION_2_0: u8 = 0x92;
pub const VERSION_CLONE: u8 = 0x91;

/// `TxControlReg`'s two antenna driver bits.
const ANTENNA_DRIVERS: u8 = 0x03;

/// Reads of `ComIrqReg` before a frame is given up on. The reader raises the
/// flag within a few tens of microseconds of the command; this is the bound
/// that keeps a dead part from hanging the firmware.
const POLL_ATTEMPTS: u32 = 200;

/// The frames of a MIFARE Classic selection.
const REQA: u8 = 0x26;
const CASCADE_LEVEL_1: u8 = 0x93;
const ANTICOLLISION_CL1: u8 = 0x20;
const SELECT_CL1: u8 = 0x70;
/// `BitFramingReg`'s TxLastBits for the 7-bit `REQA` frame.
const TX_LAST_BITS_REQA: u8 = 0x07;

/// Why a transfer, an identity check or a card answer failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mfrc522Error<E> {
    /// The bus rejected a transaction.
    Bus(E),
    /// The part answered, but not with the version an MFRC522 has.
    WrongVersion(u8),
    /// The answer was shorter than the frame it should carry.
    Short,
    /// The anticollision answer's BCC was not the XOR of the UID; the byte
    /// carried is the BCC the card sent.
    Bcc(u8),
    /// The select answer's CRC_A was not the checksum of its SAK.
    Crc,
}

/// A selected card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Tag {
    /// The card's answer to `REQA`, as a little-endian word.
    pub atqa: u16,
    /// The card's four-byte UID.
    pub uid: [u8; 4],
    /// The select acknowledge: 0x08 for a MIFARE Classic 1K.
    pub sak: u8,
}

/// An MFRC522 on an SPI device endpoint.
pub struct Mfrc522<SPI> {
    spi: SPI,
}

impl<SPI: SpiDevice> Mfrc522<SPI> {
    /// Soft-reset the reader, check its version, apply the standard init and
    /// switch the antenna on.
    ///
    /// A part that answers with another version is
    /// `Mfrc522Error::WrongVersion`, so the boot log names the device
    /// instead of reporting a reader that never sees a card.
    pub fn new(spi: SPI) -> Result<Self, Mfrc522Error<SPI::Error>> {
        let mut this = Self { spi };
        // The soft reset clears the register file, so the identity check runs
        // against the reader's own power-on state.
        this.write_register(COMMAND, SOFT_RESET)?;
        let version = this.version()?;
        if version != VERSION_2_0 && version != VERSION_CLONE {
            return Err(Mfrc522Error::WrongVersion(version));
        }
        // The datasheet's application-note init: a 25 ms timer, a 100 % ASK
        // modulation and the CRC preset of 0x6363 for CRC_A.
        this.write_register(T_MODE, 0x8D)?;
        this.write_register(T_PRESCALER, 0x3E)?;
        this.write_register(T_RELOAD_H, 0x00)?;
        this.write_register(T_RELOAD_L, 0x1E)?;
        this.write_register(TX_ASK, 0x40)?;
        this.write_register(MODE, 0x3D)?;
        // The antenna drivers power up off: read the register and switch on the
        // two bits without disturbing the rest.
        let mut tx_control = [0u8; 1];
        this.read_register(TX_CONTROL, &mut tx_control)?;
        this.write_register(TX_CONTROL, tx_control[0] | ANTENNA_DRIVERS)?;
        // Nothing is encrypted yet.
        this.write_register(STATUS2, 0x00)?;
        Ok(this)
    }

    /// `VersionReg`: 0x92 on a v2.0 reader, 0x91 on some clones.
    pub fn version(&mut self) -> Result<u8, Mfrc522Error<SPI::Error>> {
        let mut version = [0u8; 1];
        self.read_register(VERSION, &mut version)?;
        Ok(version[0])
    }

    /// Select the card in the field: `REQA`, anticollision CL1, select CL1.
    ///
    /// `Ok(Some(tag))` when a card answered, `Ok(None)` when the field is
    /// empty or the card stayed quiet.
    pub fn read_tag(&mut self) -> Result<Option<Tag>, Mfrc522Error<SPI::Error>> {
        let mut reply = [0u8; 8];

        let len = self.transceive(&[REQA], TX_LAST_BITS_REQA, &mut reply)?;
        if len == 0 {
            return Ok(None);
        }
        if len < 2 {
            return Err(Mfrc522Error::Short);
        }
        let atqa = u16::from_le_bytes([reply[0], reply[1]]);

        let len = self.transceive(&[CASCADE_LEVEL_1, ANTICOLLISION_CL1], 0, &mut reply)?;
        if len == 0 {
            return Ok(None);
        }
        if len < 5 {
            return Err(Mfrc522Error::Short);
        }
        let uid = [reply[0], reply[1], reply[2], reply[3]];
        let bcc = bcc(&uid);
        if reply[4] != bcc {
            return Err(Mfrc522Error::Bcc(reply[4]));
        }

        // Select CL1: the cascade level, the select byte, the UID, its BCC and
        // the CRC_A over those seven bytes.
        let mut select = [0u8; 9];
        select[0] = CASCADE_LEVEL_1;
        select[1] = SELECT_CL1;
        select[2..6].copy_from_slice(&uid);
        select[6] = bcc;
        let select_crc = crc_a(&select[..7]).to_le_bytes();
        select[7..9].copy_from_slice(&select_crc);
        let len = self.transceive(&select, 0, &mut reply)?;
        if len == 0 {
            return Ok(None);
        }
        if len < 3 {
            return Err(Mfrc522Error::Short);
        }
        let sak = reply[0];
        if reply[1..3] != crc_a(&[sak]).to_le_bytes() {
            return Err(Mfrc522Error::Crc);
        }
        Ok(Some(Tag { atqa, uid, sak }))
    }

    /// One frame: clear the FIFO, push `frame`, set the framing, Transceive,
    /// wait for the answer, then drain it into `reply`. Returns how many bytes
    /// the card answered, or 0 when it did not.
    fn transceive(
        &mut self,
        frame: &[u8],
        tx_last_bits: u8,
        reply: &mut [u8],
    ) -> Result<usize, Mfrc522Error<SPI::Error>> {
        self.write_register(FIFO_LEVEL, FIFO_FLUSH)?;
        for &byte in frame {
            self.write_register(FIFO_DATA, byte)?;
        }
        self.write_register(BIT_FRAMING, tx_last_bits)?;
        self.write_register(COMMAND, TRANSCEIVE)?;

        let mut irq = [0u8; 1];
        let mut answered = false;
        for _ in 0..POLL_ATTEMPTS {
            self.read_register(COM_IRQ, &mut irq)?;
            if irq[0] & (IRQ_RX | IRQ_TIMER) != 0 {
                answered = irq[0] & IRQ_RX != 0;
                break;
            }
        }
        if !answered {
            // The timer ran out, or nothing ever rose: no card in the field.
            return Ok(0);
        }
        let mut error = [0u8; 1];
        self.read_register(ERROR, &mut error)?;
        if error[0] != 0 {
            // The reader refused the frame: read it as no answer, as a card
            // that stayed quiet would be.
            return Ok(0);
        }
        let mut level = [0u8; 1];
        self.read_register(FIFO_LEVEL, &mut level)?;
        let len = level[0] as usize;
        if len > reply.len() {
            return Err(Mfrc522Error::Short);
        }
        for byte in reply.iter_mut().take(len) {
            let mut one = [0u8; 1];
            self.read_register(FIFO_DATA, &mut one)?;
            *byte = one[0];
        }
        Ok(len)
    }

    /// A register read: the address byte carries the register shifted left with
    /// the read bit, and there is no dummy byte. The address increments across
    /// the answer.
    fn read_register(
        &mut self,
        register: u8,
        buf: &mut [u8],
    ) -> Result<(), Mfrc522Error<SPI::Error>> {
        let command = [(register << 1) | READ];
        self.spi
            .transaction(&mut [Operation::Write(&command), Operation::Read(buf)])
            .map_err(Mfrc522Error::Bus)
    }

    /// A register write: the address byte, then the value.
    fn write_register(
        &mut self,
        register: u8,
        value: u8,
    ) -> Result<(), Mfrc522Error<SPI::Error>> {
        let frame = [register << 1, value];
        self.spi
            .transaction(&mut [Operation::Write(&frame)])
            .map_err(Mfrc522Error::Bus)
    }
}

/// The BCC of a four-byte UID: the XOR of its bytes.
fn bcc(uid: &[u8; 4]) -> u8 {
    uid[0] ^ uid[1] ^ uid[2] ^ uid[3]
}

/// CRC_A of ISO/IEC 14443-3: the preset 0x6363 and the reflected polynomial
/// 0x8408. The result goes on the wire low byte first, so a MIFARE Classic
/// HALT (`50 00`) carries `57 CD`.
pub fn crc_a(frame: &[u8]) -> u16 {
    let mut crc: u16 = 0x6363;
    for &byte in frame {
        crc ^= byte as u16;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0x8408
            } else {
                crc >> 1
            };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use std::vec;
    use std::vec::Vec;

    /// A register-file MFRC522 with the MIFARE Classic 1K card a real
    /// selection would find, written against the datasheet rather than by
    /// reusing the driver's own code.
    struct Fake {
        regs: [u8; 64],
        fifo: Vec<u8>,
        irq: u8,
        version: u8,
        present: bool,
        uid: [u8; 4],
        /// Corrupt the BCC, the SAK's CRC_A, as a bad card or a noisy field
        /// would.
        bad_bcc: bool,
        bad_crc: bool,
        /// Every register write, by datasheet address.
        writes: Vec<(u8, u8)>,
        /// Every frame the driver handed to a Transceive.
        frames: Vec<Vec<u8>>,
    }

    impl Fake {
        fn card(version: u8, present: bool) -> Self {
            let mut regs = [0u8; 64];
            // TxControlReg powers up 0x80: the antenna drivers are off.
            regs[TX_CONTROL as usize] = 0x80;
            Self {
                regs,
                fifo: Vec::new(),
                irq: 0,
                version,
                present,
                uid: [0xDE, 0xAD, 0xBE, 0xEF],
                bad_bcc: false,
                bad_crc: false,
                writes: Vec::new(),
                frames: Vec::new(),
            }
        }

        fn write_register(&mut self, register: u8, value: u8) {
            self.writes.push((register, value));
            match register {
                COMMAND => match value {
                    SOFT_RESET => {
                        self.regs = [0u8; 64];
                        self.regs[TX_CONTROL as usize] = 0x80;
                        self.fifo.clear();
                        self.irq = 0;
                    }
                    TRANSCEIVE => self.transceive(),
                    _ => {}
                },
                FIFO_DATA => self.fifo.push(value),
                FIFO_LEVEL => {
                    if value & FIFO_FLUSH != 0 {
                        self.fifo.clear();
                    }
                }
                COM_IRQ => self.irq &= !value,
                _ => self.regs[register as usize] = value,
            }
        }

        fn read_register(&mut self, register: u8) -> u8 {
            match register {
                COM_IRQ => {
                    let irq = self.irq;
                    self.irq = 0;
                    irq
                }
                ERROR => 0,
                FIFO_DATA => {
                    if self.fifo.is_empty() {
                        0
                    } else {
                        self.fifo.remove(0)
                    }
                }
                FIFO_LEVEL => self.fifo.len() as u8,
                VERSION => self.version,
                _ => self.regs[register as usize],
            }
        }

        fn transceive(&mut self) {
            let frame = core::mem::take(&mut self.fifo);
            self.frames.push(frame.clone());
            let tx_last_bits = self.regs[BIT_FRAMING as usize] & 0x07;
            let answer = if self.present {
                self.answer(&frame, tx_last_bits)
            } else {
                None
            };
            match answer {
                Some(reply) => {
                    self.fifo = reply;
                    self.irq = IRQ_RX | 0x10;
                }
                None => {
                    self.fifo.clear();
                    self.irq = IRQ_TIMER | 0x10;
                }
            }
        }

        fn answer(&self, frame: &[u8], tx_last_bits: u8) -> Option<Vec<u8>> {
            if frame.len() == 1 && frame[0] == REQA && tx_last_bits == TX_LAST_BITS_REQA {
                return Some(vec![0x04, 0x00]);
            }
            if frame.len() == 2 && frame[0] == CASCADE_LEVEL_1 && frame[1] == ANTICOLLISION_CL1 {
                let mut reply = self.uid.to_vec();
                let mut bcc = self.uid.iter().fold(0u8, |acc, byte| acc ^ byte);
                if self.bad_bcc {
                    bcc ^= 0xFF;
                }
                reply.push(bcc);
                return Some(reply);
            }
            if frame.len() == 9 && frame[0] == CASCADE_LEVEL_1 && frame[1] == SELECT_CL1 {
                let sak = 0x08u8;
                let mut crc = crc_a(&[sak]).to_le_bytes();
                if self.bad_crc {
                    crc[0] ^= 0xFF;
                }
                return Some(vec![sak, crc[0], crc[1]]);
            }
            None
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
                        // The wire address is the register shifted left, with
                        // the read bit on a read and bit 0 always 0.
                        if first & READ != 0 {
                            command = Some((first >> 1) & 0x3F);
                        } else if let Some(&value) = bytes.get(1) {
                            self.write_register((first >> 1) & 0x3F, value);
                        }
                    }
                    Operation::Read(buf) => {
                        let mut register = command.take().unwrap_or(0);
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

    fn sensor(version: u8) -> Result<Mfrc522<Fake>, Mfrc522Error<Infallible>> {
        Mfrc522::new(Fake::card(version, true))
    }

    #[test]
    fn crc_a_matches_the_published_frame_vectors() {
        // A MIFARE Classic HALT is `50 00 57 CD` on the wire.
        assert_eq!(crc_a(&[0x50, 0x00]), 0xCD57);
        // The select frame of the default UID, low byte first.
        assert_eq!(crc_a(&[0x93, 0x70, 0xDE, 0xAD, 0xBE, 0xEF, 0x22]), 0x9CB9);
        assert_eq!(bcc(&[0xDE, 0xAD, 0xBE, 0xEF]), 0x22);
    }

    #[test]
    fn it_soft_resets_before_it_checks_the_version() {
        let sensor = sensor(VERSION_2_0).expect("a v2.0 reader");
        assert_eq!(sensor.spi.writes[0], (COMMAND, SOFT_RESET), "the reset comes first");
        assert_eq!(
            sensor.spi.writes.iter().map(|(register, _)| *register).next(),
            Some(COMMAND)
        );
    }

    #[test]
    fn it_accepts_the_v2_reader_and_the_clones() {
        assert!(sensor(VERSION_2_0).is_ok());
        assert!(sensor(VERSION_CLONE).is_ok());
    }

    #[test]
    fn it_refuses_a_part_that_is_not_an_mfrc522() {
        assert_eq!(
            sensor(0x00).err(),
            Some(Mfrc522Error::WrongVersion(0x00)),
            "a part that answers nothing"
        );
        assert_eq!(sensor(0x12).err(), Some(Mfrc522Error::WrongVersion(0x12)));
    }

    #[test]
    fn it_applies_the_standard_init_and_switches_the_antenna_on() {
        let sensor = sensor(VERSION_2_0).unwrap();
        let regs = &sensor.spi.regs;
        assert_eq!(regs[T_MODE as usize], 0x8D);
        assert_eq!(regs[T_PRESCALER as usize], 0x3E);
        assert_eq!(regs[T_RELOAD_H as usize], 0x00);
        assert_eq!(regs[T_RELOAD_L as usize], 0x1E);
        assert_eq!(regs[TX_ASK as usize], 0x40);
        assert_eq!(regs[MODE as usize], 0x3D);
        assert_eq!(regs[TX_CONTROL as usize], 0x83, "0x80 powered up, then | 0x03");
        assert_eq!(regs[STATUS2 as usize], 0x00);
    }

    #[test]
    fn it_transceives_the_three_frames_of_a_selection() {
        let mut sensor = sensor(VERSION_2_0).unwrap();
        sensor.read_tag().unwrap();
        assert_eq!(sensor.spi.frames.len(), 3);
        assert_eq!(sensor.spi.frames[0], [REQA], "REQA, alone and 7 bits");
        assert_eq!(
            sensor.spi.frames[1],
            [CASCADE_LEVEL_1, ANTICOLLISION_CL1],
            "anticollision CL1"
        );
        assert_eq!(
            sensor.spi.frames[2],
            [0x93, 0x70, 0xDE, 0xAD, 0xBE, 0xEF, 0x22, 0xB9, 0x9C],
            "select CL1: UID, BCC and CRC_A"
        );
    }

    #[test]
    fn the_selection_decodes_to_the_cards_uid_sak_and_atqa() {
        let mut sensor = sensor(VERSION_2_0).unwrap();
        assert_eq!(
            sensor.read_tag().unwrap(),
            Some(Tag { atqa: 0x0004, uid: [0xDE, 0xAD, 0xBE, 0xEF], sak: 0x08 })
        );
    }

    #[test]
    fn no_card_reads_as_no_tag() {
        let mut sensor = Mfrc522::new(Fake::card(VERSION_2_0, false)).unwrap();
        assert_eq!(sensor.read_tag(), Ok(None));
    }

    #[test]
    fn it_refuses_an_answer_with_a_wrong_bcc() {
        let mut fake = Fake::card(VERSION_2_0, true);
        fake.bad_bcc = true;
        let mut sensor = Mfrc522::new(fake).unwrap();
        assert_eq!(sensor.read_tag().err(), Some(Mfrc522Error::Bcc(0xDD)));
    }

    #[test]
    fn it_refuses_an_answer_with_a_wrong_crc() {
        let mut fake = Fake::card(VERSION_2_0, true);
        fake.bad_crc = true;
        let mut sensor = Mfrc522::new(fake).unwrap();
        assert_eq!(sensor.read_tag().err(), Some(Mfrc522Error::Crc));
    }
}
