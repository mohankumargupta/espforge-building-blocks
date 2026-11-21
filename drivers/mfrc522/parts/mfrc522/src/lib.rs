//! An NXP MFRC522 contactless reader with a MIFARE Classic 1K card in its
//! field, written as a virtual part.
//!
//! The firmware talks to it over SPI the way it would the real chip: one
//! address byte per transfer, `register << 1`, with bit 7 set for a read and
//! bit 0 always 0, and no dummy byte. `VersionReg` (0x37) answers 0x92 and
//! the reads auto-increment across the register file. `CommandReg` (0x01)
//! is the exchange: writing `Transceive` (0x0C) turns the FIFO's contents
//! into a card transaction, and `ComIrqReg` (0x04) says whether a card
//! answered or the timer ran out.
//!
//! The card is the part's own state, not a register: `present`, a four-byte
//! `uid`, the ATQA `04 00` and the SAK `08`. The exchange is the three
//! frames of an ISO/IEC 14443-3 selection: `REQA` as a 7-bit frame, then
//! anticollision CL1 (`93 20`), then select CL1 (`93 70` plus the UID, its
//! BCC and CRC_A). README.md in the fixture folder lists what is left out.
//!
//! `Mfrc522` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as
/// `{ pins: { CS: 7, SCK: 6, MOSI: 3, MISO: 2 } }`.
const CS: &str = "CS";
const SCK: &str = "SCK";
const MOSI: &str = "MOSI";
const MISO: &str = "MISO";

/// Read bit: set for a read, clear for a write. Bit 0 of the wire address is
/// always 0, so the register number is the byte shifted right one.
const READ: u8 = 0x80;

/// The registers the exchange needs, by datasheet address.
const COMMAND: usize = 0x01;
const COM_IRQ: usize = 0x04;
const ERROR: usize = 0x06;
const FIFO_DATA: usize = 0x09;
const FIFO_LEVEL: usize = 0x0A;
const BIT_FRAMING: usize = 0x0D;
const TX_CONTROL: usize = 0x14;
const CRC_RESULT_H: usize = 0x21;
const CRC_RESULT_L: usize = 0x22;
const VERSION: usize = 0x37;

/// `VersionReg` on a v2.0 reader. Some clones answer 0x91; this part is a
/// v2.0 part.
const VERSION_VALUE: u8 = 0x92;

/// `CommandReg`: the soft reset and the Transceive command. Idle (0x00) and
/// anything else leave the register file alone.
const SOFT_RESET: u8 = 0x0F;
const TRANSCEIVE: u8 = 0x0C;
const CALC_CRC: u8 = 0x03;

/// `DivIrqReg`: the CRC coprocessor finished.
const DIV_IRQ: usize = 0x05;
const CRC_IRQ: u8 = 0x04;

/// `ComIrqReg`: a card answered, the reader is idle, the timer ran out.
const IRQ_RX: u8 = 0x20;
const IRQ_IDLE: u8 = 0x10;
const IRQ_TIMER: u8 = 0x01;

/// `FIFOLevelReg`'s flush bit: writing it empties the FIFO.
const FIFO_FLUSH: u8 = 0x80;
/// The FIFO holds a frame of up to 64 bytes.
const FIFO_CAPACITY: usize = 64;

/// `TxControlReg` powers up with the antenna drivers off (0x80) and the
/// driver switches them on.
const TX_CONTROL_POWER_ON: u8 = 0x80;

/// The card's frame bytes.
const REQA: u8 = 0x26;
const CASCADE_LEVEL_1: u8 = 0x93;
const ANTICOLLISION_CL1: u8 = 0x20;
const SELECT_CL1: u8 = 0x70;
/// `BitFramingReg`'s TxLastBits for the 7-bit `REQA` frame.
const REQA_LAST_BITS: u8 = 0x07;

/// What the card answers with: a MIFARE Classic 1K.
const ATQA: [u8; 2] = [0x04, 0x00];
const SAK: u8 = 0x08;
const DEFAULT_UID: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];

struct Mfrc522 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    cs: Option<u32>,
    sck: Option<u32>,
    mosi: Option<u32>,
    miso: Option<u32>,
    /// Registers 0x00..=0x3F. `command`, `com_irq`, `error`, the FIFO, the
    /// CRC result and `version` are answered from the fields below; every
    /// other register is plain storage.
    regs: [u8; 64],
    /// The FIFO: writes push, reads pop.
    fifo: Vec<u8>,
    /// `ComIrqReg`, set by the last exchange and cleared when it is read.
    com_irq: u8,
    /// `DivIrqReg`: `CRCIRq` once a `CalcCRC` finished, cleared on read.
    div_irq: u8,
    /// `CRCResultRegH`/`L`: the CRC_A of the last answer, high byte first.
    crc: u16,
    /// The last wire address byte, for a read the emulator delivers as its
    /// own transfer with the address already gone by.
    command: u8,
    /// An address phase arrived and its answer phase is still to come.
    pending: bool,
    /// An address-only write arrived; the following transfer carries its data.
    burst: Option<usize>,
    /// `Transceive` was commanded and is waiting for `BitFraming`'s StartSend
    /// (or, for a driver that never writes StartSend, for the next read).
    pending_transceive: bool,

    /// Whether a card is in the field.
    present: bool,
    /// The card's four-byte UID.
    uid: [u8; 4],
}

impl Default for Mfrc522 {
    fn default() -> Self {
        Self {
            cs: None,
            sck: None,
            mosi: None,
            miso: None,
            regs: power_on_registers(),
            fifo: Vec::new(),
            com_irq: 0,
            div_irq: 0,
            crc: 0,
            command: 0,
            pending: false,
            burst: None,
            pending_transceive: false,
            present: true,
            uid: DEFAULT_UID,
        }
    }
}

impl Device for Mfrc522 {
    const NAME: &'static str = "mfrc522";
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

    /// Change the field. `present` puts a card in it, `absent` takes the card
    /// away, `set:uid=<hex>` gives it another four-byte UID (e.g.
    /// `set:uid=01:02:03:04`) and `rest` puts back the default card.
    fn on_input(&mut self, name: &str) {
        if let Some(hex) = name.strip_prefix("set:uid=") {
            if let Some(uid) = parse_uid(hex) {
                self.uid = uid;
            }
            return;
        }
        match name {
            "present" => self.present = true,
            "absent" => self.present = false,
            "rest" => {
                self.present = true;
                self.uid = DEFAULT_UID;
            }
            _ => {} // Not an interaction this part has.
        }
    }

    /// The guest's SPI transfer. A write stores a register, except on
    /// `CommandReg`, where it drives the exchange. A read answers from the
    /// address byte, auto-incrementing across the register file.
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
            if let Some(register) = self.burst.take() {
                // The data half of a split transaction: every byte goes to the
                // register its own address transfer named.
                for &byte in &spi.tx {
                    self.write(register, byte);
                }
                return None;
            }
            if let Some(&first) = spi.tx.first() {
                if first & READ != 0 {
                    // A command-only read: its answer phase is still to come.
                    self.command = first;
                    self.pending = true;
                } else if spi.tx.len() == 1 {
                    // An address-only write; its data follows in its own
                    // transfer.
                    self.pending = false;
                    self.burst = Some((first >> 1) as usize);
                } else if let Some(&value) = spi.tx.get(1) {
                    self.pending = false;
                    let register = (first >> 1) as usize;
                    if register == FIFO_DATA {
                        // A multi-byte FIFO write: every byte after the address.
                        for &byte in &spi.tx[1..] {
                            self.write(FIFO_DATA, byte);
                        }
                    } else {
                        self.write(register, value);
                    }
                }
            }
            return None;
        }
        if let Some(&first) = spi.tx.first() {
            if let Some(register) = self.burst.take() {
                // The data half of a split write: every byte goes to the
                // register its own address transfer named, whatever its bit 7
                // says (0x93 is both a select byte and a read address).
                for &byte in &spi.tx {
                    self.write(register, byte);
                }
                return Some(reply(vec![0u8; spi.rx_len as usize]));
            }
            if first & READ != 0 {
                // The address. A transfer longer than its own write carries
                // the answer with it; a full-duplex address phase is answered
                // with bytes the master clocks in and ignores, and the answer
                // phase follows.
                self.burst = None;
                self.command = first;
                if self.pending {
                    // The answer phase of a split read: all its bytes are the
                    // registers that start at the address transfer.
                    self.pending = false;
                    let data = self.serve((first >> 1) as usize, spi.rx_len);
                    return Some(reply(data));
                }
                if spi.rx_len as usize > spi.tx.len() {
                    self.pending = false;
                    let data = self.serve((first >> 1) as usize, spi.rx_len);
                    return Some(reply(data));
                }
                if spi.tx.len() == 1 {
                    // An address-only read; its answer follows in its own
                    // transfer, filling every byte with data.
                    self.pending = true;
                    return Some(reply(vec![0u8; spi.rx_len as usize]));
                }
                if spi.rx_len as usize == spi.tx.len() {
                    // The address and the answer in one full-duplex frame: the
                    // bytes after the address are the register's value.
                    let mut data = vec![0u8; spi.rx_len as usize];
                    if spi.rx_len > 1 {
                        let answer = self.serve((first >> 1) as usize, spi.rx_len - 1);
                        data[1..].copy_from_slice(&answer);
                    }
                    return Some(reply(data));
                }
                self.pending = true;
                return Some(reply(vec![0u8; spi.rx_len as usize]));
            }
            // The answer phase after an address that came in its own transfer
            // carries the controller's buffer, which is no command here.
            if self.pending {
                self.pending = false;
                let data = self.serve((self.command >> 1) as usize, spi.rx_len);
                return Some(reply(data));
            }
            if spi.tx.len() == 1 {
                // An address-only write; its data follows in its own transfer.
                self.pending = false;
                self.burst = Some((first >> 1) as usize);
                return Some(reply(vec![0u8; spi.rx_len as usize]));
            }
            if let Some(&value) = spi.tx.get(1) {
                // A full-duplex write: the frame is the address and the value,
                // and the read half is what the master ignores.
                self.pending = false;
                let register = (first >> 1) as usize;
                if register == FIFO_DATA {
                    for &byte in &spi.tx[1..] {
                        self.write(FIFO_DATA, byte);
                    }
                } else {
                    self.write(register, value);
                }
                return Some(reply(vec![0u8; spi.rx_len as usize]));
            }
        }
        // A read with no address of its own, answered from the last one.
        self.pending = false;
        let data = self.serve((self.command >> 1) as usize, spi.rx_len);
        Some(reply(data))
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("present", self.present)
                .set(
                    "uid",
                    format!(
                        "{:02x}{:02x}{:02x}{:02x}",
                        self.uid[0], self.uid[1], self.uid[2], self.uid[3]
                    ),
                )
                .set("sak", format!("0x{:02x}", SAK))
                .set("atqa", format!("0x{:04x}", u16::from_le_bytes(ATQA))),
        )
    }
}

impl Mfrc522 {
    /// Register `register`, and the registers after it, as a read answers them.
    /// `ComIrqReg` is cleared by the read and `FIFODataReg` pops, so this takes
    /// `&mut self`.
    fn read(&mut self, register: usize) -> u8 {
        match register & 0x3F {
            COM_IRQ => {
                let irq = self.com_irq;
                self.com_irq = 0;
                irq
            }
            DIV_IRQ => {
                let irq = self.div_irq;
                self.div_irq = 0;
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
            CRC_RESULT_H => (self.crc >> 8) as u8,
            CRC_RESULT_L => self.crc as u8,
            VERSION => VERSION_VALUE,
            register => self.regs[register],
        }
    }

    /// A register write. `CommandReg` drives the exchange; `FIFODataReg`
    /// pushes and `FIFOLevelReg`'s flush bit empties; the CRC result and the
    /// version are read-only.
    fn write(&mut self, register: usize, value: u8) {
        match register & 0x3F {
            COMMAND => {
                self.regs[COMMAND] = value;
                match value {
                    SOFT_RESET => self.soft_reset(),
                    TRANSCEIVE => self.pending_transceive = true,
                    CALC_CRC => {
                        if !self.fifo.is_empty() {
                            self.crc = crc_a(&self.fifo);
                            self.div_irq = CRC_IRQ;
                        }
                    }
                    _ => self.pending_transceive = false, // Idle, and unmodelled commands.
                }
            }
            DIV_IRQ => self.div_irq &= !value,
            BIT_FRAMING if value & 0x80 != 0 => {
                // StartSend: the command starts now, with these last bits.
                self.regs[BIT_FRAMING] = value & 0x07;
                self.pending_transceive = false;
                self.transceive();
            }
            COM_IRQ => self.com_irq &= !value,
            FIFO_DATA => {
                if self.fifo.len() < FIFO_CAPACITY {
                    self.fifo.push(value);
                }
            }
            FIFO_LEVEL => {
                if value & FIFO_FLUSH != 0 {
                    self.fifo.clear();
                }
            }
            ERROR | CRC_RESULT_H | CRC_RESULT_L | VERSION => {}
            register => self.regs[register] = value,
        }
    }

    /// `len` registers from `start`, which is one read: the address increments
    /// across the answer.
    fn serve(&mut self, start: usize, len: u32) -> Vec<u8> {
        // A driver that commands `Transceive` without StartSend has its
        // exchange run before the first answer is read.
        if self.pending_transceive {
            self.pending_transceive = false;
            self.transceive();
        }
        (0..len as usize)
            .map(|i| {
                if start & 0x3F == FIFO_DATA {
                    // The FIFO is one port, not a register file: every byte
                    // pops the next one.
                    self.read(FIFO_DATA)
                } else {
                    self.read(start + i)
                }
            })
            .collect()
    }

    /// The soft reset: back to the power-on register file, with an empty FIFO.
    /// The card is not the reader, so it stays in the field.
    fn soft_reset(&mut self) {                self.regs = power_on_registers();
        self.fifo.clear();
        self.com_irq = 0;
        self.div_irq = 0;
        self.crc = 0;
        self.pending = false;
        self.burst = None;
        self.pending_transceive = false;
    }

    /// `Transceive`: what is in the FIFO goes out, and what the card answers
    /// takes its place. A frame nobody answers leaves the FIFO empty and sets
    /// `TimerIRq`, which the driver reads as "no tag".
    fn transceive(&mut self) {
        let frame = core::mem::take(&mut self.fifo);
        let tx_last_bits = self.regs[BIT_FRAMING] & 0x07;
        let answer = if self.present {
            self.card_answer(&frame, tx_last_bits)
        } else {
            None
        };
        match answer {
            Some((crc, reply)) => {
                self.crc = crc;
                self.com_irq = IRQ_RX | IRQ_IDLE;
                self.fifo = reply;
            }
            None => {
                self.crc = 0;
                self.com_irq = IRQ_TIMER | IRQ_IDLE;
            }
        }
    }

    /// What the card answers to `frame`, as the CRC_A over the answer and the
    /// answer's bytes, or `None` when it does not answer. The three frames are
    /// an ISO/IEC 14443-3 selection: `REQA` (7 bits), anticollision CL1 and
    /// select CL1, whose CRC_A the card checks before it answers with its SAK.
    fn card_answer(&self, frame: &[u8], tx_last_bits: u8) -> Option<(u16, Vec<u8>)> {
        if frame.len() == 1 && frame[0] == REQA && tx_last_bits == REQA_LAST_BITS {
            return Some((crc_a(&ATQA), ATQA.to_vec()));
        }
        if frame.len() == 2 && frame[0] == CASCADE_LEVEL_1 && frame[1] == ANTICOLLISION_CL1 {
            let mut reply = self.uid.to_vec();
            reply.push(bcc(&self.uid));
            let crc = crc_a(&reply);
            return Some((crc, reply));
        }
        if frame.len() == 9 && frame[0] == CASCADE_LEVEL_1 && frame[1] == SELECT_CL1 {
            let select = &frame[..7];
            let expected = crc_a(select).to_le_bytes();
            // A card that is addressed with another UID, a wrong BCC or a
            // wrong CRC stays quiet.
            if frame[2..6] == self.uid
                && frame[6] == bcc(&self.uid)
                && frame[7] == expected[0]
                && frame[8] == expected[1]
            {
                let crc = crc_a(&[SAK]);
                let mut reply = vec![SAK];
                reply.extend_from_slice(&crc.to_le_bytes());
                return Some((crc, reply));
            }
        }
        None
    }
}

/// The power-on register file: the antenna drivers off, everything else 0.
fn power_on_registers() -> [u8; 64] {
    let mut regs = [0u8; 64];
    regs[TX_CONTROL] = TX_CONTROL_POWER_ON;
    regs
}

/// The BCC of a four-byte UID: the XOR of its bytes.
fn bcc(uid: &[u8; 4]) -> u8 {
    uid[0] ^ uid[1] ^ uid[2] ^ uid[3]
}

/// CRC_A of ISO/IEC 14443-3: the preset 0x6363, the reflected polynomial
/// 0x8408, and the result transmitted low byte first (a HALT `50 00` carries
/// `57 CD`).
fn crc_a(frame: &[u8]) -> u16 {
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

/// `set:uid=01:02:03:04`: exactly four two-digit hex bytes.
fn parse_uid(text: &str) -> Option<[u8; 4]> {
    let mut uid = [0u8; 4];
    let mut parts = text.split(':');
    for slot in uid.iter_mut() {
        let part = parts.next()?;
        if part.len() != 2 {
            return None;
        }
        *slot = u8::from_str_radix(part, 16).ok()?;
    }
    parts.next().is_none().then_some(uid)
}

// Big enough for a read of the register file and the frame a host hands over.
esp_part::export_part!(Mfrc522, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as the fixture's board wires it: CS on GPIO 7, SCK on 6, MOSI on
    /// 3, MISO on 2.
    fn chip() -> Mfrc522 {
        let mut chip = Mfrc522::default();
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
    fn write(chip: &mut Mfrc522, register: usize, value: u8) {
        let address = (register as u8) << 1;
        assert_eq!(
            chip.on_bus(&request(&[address, value], 0)),
            None,
            "a write needs no answer"
        );
    }

    /// A register read, in the two transfers the runtime makes of it: the
    /// address on its own, then an answer phase whose `tx` is the controller's
    /// buffer.
    fn read(chip: &mut Mfrc522, register: usize, rx_len: usize) -> Vec<u8> {
        let command = ((register as u8) << 1) | READ;
        let address = chip
            .on_bus(&request(&[command], 1))
            .expect("the address phase is answered");
        assert_eq!(address.data.len(), 1, "one byte the master ignores");
        let zeros = vec![0u8; rx_len];
        chip.on_bus(&request(&zeros, rx_len as u32))
            .expect("the answer phase is answered")
            .data
    }

    /// Push a frame into the FIFO, flushing what was there first.
    fn push(chip: &mut Mfrc522, frame: &[u8]) {
        write(chip, FIFO_LEVEL, FIFO_FLUSH);
        for &byte in frame {
            write(chip, FIFO_DATA, byte);
        }
    }

    /// One Transceive, and the answer drained from the FIFO.
    fn transceive(chip: &mut Mfrc522, frame: &[u8], tx_last_bits: u8) -> Vec<u8> {
        write(chip, BIT_FRAMING, tx_last_bits);
        push(chip, frame);
        write(chip, COMMAND, TRANSCEIVE);
        let level = read(chip, FIFO_LEVEL, 1)[0];
        (0..level).map(|_| read(chip, FIFO_DATA, 1)[0]).collect()
    }

    /// A select CL1 frame: the cascade level and select byte, the UID, its BCC
    /// and the CRC_A over those seven bytes.
    fn select_frame(uid: [u8; 4]) -> Vec<u8> {
        let mut frame = vec![CASCADE_LEVEL_1, SELECT_CL1];
        frame.extend_from_slice(&uid);
        frame.push(bcc(&uid));
        let crc = crc_a(&frame).to_le_bytes();
        frame.extend_from_slice(&crc);
        frame
    }

    #[test]
    fn the_version_is_answered_before_any_command() {
        // The wire address of 0x37 is 0xEE: the register shifted left, with
        // the read bit.
        let mut chip = chip();
        assert_eq!(read(&mut chip, VERSION, 1), [0x92]);
        assert_eq!(((VERSION as u8) << 1) | READ, 0xEE);
    }

    #[test]
    fn the_calc_crc_command_checksums_the_fifo() {
        let mut chip = chip();
        let frame = [0x93, 0x70, 0xDE, 0xAD, 0xBE, 0xEF, 0x22];
        push(&mut chip, &frame);
        write(&mut chip, COMMAND, CALC_CRC);
        assert_eq!(read(&mut chip, DIV_IRQ, 1)[0] & CRC_IRQ, CRC_IRQ);
        let low = read(&mut chip, CRC_RESULT_L, 1)[0];
        let high = read(&mut chip, CRC_RESULT_H, 1)[0];
        assert_eq!(u16::from_le_bytes([low, high]), crc_a(&frame));
    }

    #[test]
    fn a_full_duplex_split_fifo_write_pushes_every_byte() {
        let mut chip = chip();
        let address = [(FIFO_DATA as u8) << 1];
        assert!(chip.on_bus(&request(&address, 1)).is_some(), "the address phase");
        assert!(
            chip.on_bus(&request(&[0x93, 0x20], 2)).is_some(),
            "the data phase, clocked both ways"
        );
        assert_eq!(read(&mut chip, FIFO_LEVEL, 1), [2]);
        assert_eq!(read(&mut chip, FIFO_DATA, 1), [0x93]);
        assert_eq!(read(&mut chip, FIFO_DATA, 1), [0x20]);
    }

    #[test]
    fn a_split_fifo_write_pushes_every_byte() {
        let mut chip = chip();
        let address = [(FIFO_DATA as u8) << 1];
        assert_eq!(chip.on_bus(&request(&address, 0)), None, "the address phase");
        assert_eq!(chip.on_bus(&request(&[0x93, 0x20], 0)), None, "the data phase");
        assert_eq!(read(&mut chip, FIFO_LEVEL, 1), [2]);
        assert_eq!(read(&mut chip, FIFO_DATA, 1), [0x93]);
        assert_eq!(read(&mut chip, FIFO_DATA, 1), [0x20]);
    }

    #[test]
    fn a_full_duplex_read_answers_after_the_address() {
        // `[address | READ, 0]` in one transfer: the second byte is the value.
        let mut chip = chip();
        let answer = chip
            .on_bus(&request(&[((VERSION as u8) << 1) | READ, 0], 2))
            .expect("the read is answered in the same frame");
        assert_eq!(answer.data, [0x00, 0x92]);
    }

    #[test]
    fn crc_a_matches_the_published_frame_vectors() {
        // A MIFARE Classic HALT is `50 00 57 CD` on the wire, and the SAK
        // reply of a MIFARE Classic 1K is `08 B6 DD`.
        assert_eq!(crc_a(&[0x50, 0x00]), 0xCD57);
        assert_eq!(crc_a(&[0x08]), 0xDDB6);
        assert_eq!(bcc(&DEFAULT_UID), 0x22);
    }

    #[test]
    fn a_soft_reset_clears_the_register_file_but_not_the_card() {
        let mut chip = chip();
        write(&mut chip, 0x15, 0x40); // TxASKReg
        write(&mut chip, BIT_FRAMING, REQA_LAST_BITS);
        assert_eq!(read(&mut chip, 0x15, 1), [0x40]);
        write(&mut chip, COMMAND, SOFT_RESET);
        assert_eq!(read(&mut chip, 0x15, 1), [0x00], "cleared");
        assert_eq!(read(&mut chip, BIT_FRAMING, 1), [0x00], "cleared");
        assert_eq!(read(&mut chip, TX_CONTROL, 1), [0x80], "the power-on value");
        assert_eq!(read(&mut chip, VERSION, 1), [0x92], "the version survives");
        assert_eq!(transceive(&mut chip, &[REQA], REQA_LAST_BITS), [0x04, 0x00], "the card stays");
    }

    #[test]
    fn the_fifo_pushes_pops_and_reports_its_level() {
        let mut chip = chip();
        write(&mut chip, FIFO_LEVEL, FIFO_FLUSH);
        assert_eq!(read(&mut chip, FIFO_LEVEL, 1), [0]);
        for byte in [0xDEu8, 0xAD, 0xBE] {
            write(&mut chip, FIFO_DATA, byte);
        }
        assert_eq!(read(&mut chip, FIFO_LEVEL, 1), [3]);
        assert_eq!(read(&mut chip, FIFO_DATA, 1), [0xDE], "first in, first out");
        assert_eq!(read(&mut chip, FIFO_LEVEL, 1), [2], "the pop advanced the level");
        assert_eq!(read(&mut chip, FIFO_DATA, 1), [0xAD]);
        assert_eq!(read(&mut chip, FIFO_DATA, 1), [0xBE]);
        assert_eq!(read(&mut chip, FIFO_LEVEL, 1), [0]);
        write(&mut chip, FIFO_LEVEL, FIFO_FLUSH);
        assert_eq!(read(&mut chip, FIFO_LEVEL, 1), [0], "a flush of an empty FIFO");
    }

    #[test]
    fn the_three_frames_of_a_selection_answer_exactly() {
        let mut chip = chip();
        assert_eq!(
            transceive(&mut chip, &[REQA], REQA_LAST_BITS),
            [0x04, 0x00],
            "REQA -> ATQA"
        );
        assert_eq!(chip.com_irq, IRQ_RX | IRQ_IDLE);
        assert_eq!(
            transceive(&mut chip, &[CASCADE_LEVEL_1, ANTICOLLISION_CL1], 0),
            [0xDE, 0xAD, 0xBE, 0xEF, 0x22],
            "anticollision -> UID and BCC"
        );
        assert_eq!(chip.com_irq, IRQ_RX | IRQ_IDLE);
        assert_eq!(
            transceive(&mut chip, &select_frame(DEFAULT_UID), 0),
            [0x08, 0xB6, 0xDD],
            "select -> SAK and its CRC_A"
        );
        assert_eq!(chip.com_irq, IRQ_RX | IRQ_IDLE);
    }

    #[test]
    fn the_crc_result_registers_hold_the_answers_crc_high_byte_first() {
        let mut chip = chip();
        transceive(&mut chip, &[REQA], REQA_LAST_BITS);
        assert_eq!(read(&mut chip, CRC_RESULT_H, 2), [0x79, 0xC0], "CRC_A of the ATQA");
        transceive(&mut chip, &[CASCADE_LEVEL_1, ANTICOLLISION_CL1], 0);
        assert_eq!(read(&mut chip, CRC_RESULT_H, 2), [0x45, 0x73], "CRC_A of the UID and BCC");
        transceive(&mut chip, &select_frame(DEFAULT_UID), 0);
        assert_eq!(read(&mut chip, CRC_RESULT_H, 2), [0xDD, 0xB6], "CRC_A of the SAK");
    }

    #[test]
    fn a_requa_without_its_seven_bit_frame_is_not_a_requa() {
        let mut chip = chip();
        assert_eq!(
            transceive(&mut chip, &[REQA], 0),
            Vec::<u8>::new(),
            "the frame is 8 bits, not 7"
        );
        assert_eq!(chip.com_irq, IRQ_TIMER | IRQ_IDLE);
    }

    #[test]
    fn a_frame_the_card_does_not_take_leaves_the_fifo_empty() {
        let mut chip = chip();
        assert_eq!(transceive(&mut chip, &[0x50, 0x00], 0), Vec::<u8>::new(), "a HALT");
        // The select frame with another UID, a wrong BCC, then a wrong CRC.
        let mut other = select_frame(DEFAULT_UID);
        other[2] = 0x01;
        assert_eq!(transceive(&mut chip, &other, 0), Vec::<u8>::new(), "another UID");
        let mut wrong_bcc = select_frame(DEFAULT_UID);
        wrong_bcc[6] ^= 0xFF;
        assert_eq!(transceive(&mut chip, &wrong_bcc, 0), Vec::<u8>::new(), "a wrong BCC");
        let mut wrong_crc = select_frame(DEFAULT_UID);
        wrong_crc[7] ^= 0xFF;
        assert_eq!(transceive(&mut chip, &wrong_crc, 0), Vec::<u8>::new(), "a wrong CRC_A");
        assert_eq!(chip.com_irq, IRQ_TIMER | IRQ_IDLE);
    }

    #[test]
    fn no_card_leaves_the_fifo_empty_and_sets_the_timer_irq() {
        let mut chip = chip();
        chip.on_input("absent");
        assert_eq!(transceive(&mut chip, &[REQA], REQA_LAST_BITS), Vec::<u8>::new());
        assert_eq!(chip.com_irq, IRQ_TIMER | IRQ_IDLE);
        chip.on_input("present");
        assert_eq!(transceive(&mut chip, &[REQA], REQA_LAST_BITS), [0x04, 0x00]);
    }

    #[test]
    fn the_inputs_give_the_card_another_uid_and_take_it_away() {
        let mut chip = chip();
        chip.on_input("set:uid=01:02:03:04");
        assert_eq!(
            transceive(&mut chip, &[CASCADE_LEVEL_1, ANTICOLLISION_CL1], 0),
            [0x01, 0x02, 0x03, 0x04, 0x04],
            "the new UID and its BCC"
        );
        assert_eq!(
            transceive(&mut chip, &select_frame([0x01, 0x02, 0x03, 0x04]), 0),
            [0x08, 0xB6, 0xDD],
            "the new UID selects; the SAK's CRC_A does not depend on the UID"
        );
        chip.on_input("absent");
        assert_eq!(
            transceive(&mut chip, &[CASCADE_LEVEL_1, ANTICOLLISION_CL1], 0),
            Vec::<u8>::new(),
            "no card, no answer"
        );
        chip.on_input("rest");
        assert_eq!(
            transceive(&mut chip, &[CASCADE_LEVEL_1, ANTICOLLISION_CL1], 0),
            [0xDE, 0xAD, 0xBE, 0xEF, 0x22],
            "the default card is back"
        );
    }

    #[test]
    fn a_malformed_uid_is_ignored() {
        let mut chip = chip();
        for name in [
            "set:uid=01:02:03",
            "set:uid=01:02:03:04:05",
            "set:uid=zz:02:03:04",
            "set:uid=1:02:03:04",
            "set:uid=",
            "set:temperature=20",
        ] {
            chip.on_input(name);
        }
        assert_eq!(chip.uid, DEFAULT_UID);
    }

    #[test]
    fn a_combined_read_answers_in_one_transfer() {
        let mut chip = chip();
        let reply = chip.on_bus(&request(&[((VERSION as u8) << 1) | READ], 2)).unwrap();
        assert_eq!(reply.data, [0x92, 0x00], "0x37, then the register after it");
    }

    #[test]
    fn a_read_walks_the_register_file() {
        let mut chip = chip();
        write(&mut chip, 0x2A, 0x8D);
        write(&mut chip, 0x2B, 0x3E);
        assert_eq!(read(&mut chip, 0x2A, 2), [0x8D, 0x3E], "TModeReg then TPrescalerReg");
    }

    #[test]
    fn a_full_duplex_write_still_stores_the_register() {
        // The runtime may deliver a write with a read side the master ignores.
        let mut chip = chip();
        let reply = chip.on_bus(&request(&[0x2A << 1, 0x8D], 2)).unwrap();
        assert_eq!(reply.data, [0x00, 0x00]);
        assert_eq!(read(&mut chip, 0x2A, 1), [0x8D]);
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
            Mfrc522::default().claims(),
            [Claim { what: Some(claim::What::Spi(SpiClaim::default())) }]
        );
    }

    #[test]
    fn it_shows_the_card_in_the_field() {
        let mut chip = chip();
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("present"), shown.get("uid"), shown.get("sak"), shown.get("atqa")),
            (Some("true"), Some("deadbeef"), Some("0x08"), Some("0x0004"))
        );
        chip.on_input("absent");
        chip.on_input("set:uid=01:02:03:04");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("present"), shown.get("uid")),
            (Some("false"), Some("01020304"))
        );
    }
}
