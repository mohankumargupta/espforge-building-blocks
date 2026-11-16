//! An Analog Devices ADXL345 three-axis accelerometer, written as a virtual
//! part.
//!
//! The firmware reads it over SPI the way it would the real chip: a command
//! byte with bit 7 set for a read and bit 6 for auto-increment, then the
//! answer. `DEVID` (0x00) is 0xE5 and the three axes are six little-endian
//! bytes from `DATAX0` (0x32), which is all espforge's driver uses
//! (`espforge-drivers/src/adxl345.rs`). README.md lists what is left out.
//!
//! `Adxl345` is the device and knows nothing about WebAssembly. The
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

/// `DEVID`: the fixed identity register, 0xE5 on every ADXL345.
const DEVID: u8 = 0x00;
const DEVID_VALUE: u8 = 0xE5;
/// `DATAX0`: X low byte, then Y and Z, two bytes each little-endian.
const DATAX0: u8 = 0x32;

/// Read bit: set for a read, clear for a write.
const READ: u8 = 0x80;
/// Multi-byte bit: auto-increment the register address between bytes.
const MULTI: u8 = 0x40;

/// Counts per g: the datasheet's 3.9 mg/LSB at the default ±2 g range.
const COUNTS_PER_G: f32 = 256.0;
/// ±2 g, the range `DATA_FORMAT` (0x31) powers up in.
const MAX_G: f32 = 2.0;

struct Adxl345 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    cs: Option<u32>,
    sck: Option<u32>,
    mosi: Option<u32>,
    miso: Option<u32>,
    /// Gravity along each axis, in g.
    g: [f32; 3],
    /// Registers 0x00..=0x3F. `DEVID` and the axes are answered from the
    /// fields above; every other register is plain storage.
    regs: [u8; 64],
    /// The last command byte, for a read the emulator delivers as its own
    /// transfer with an empty `tx`.
    command: u8,
}

impl Default for Adxl345 {
    fn default() -> Self {
        let mut regs = [0u8; 64];
        // Bandwidth rate: the power-on 0x0A. Everything else powers up 0x00,
        // which is ±2 g, right-justified, 4-wire SPI in DATA_FORMAT (0x31).
        regs[0x2C] = 0x0A;
        Self {
            cs: None,
            sck: None,
            mosi: None,
            miso: None,
            // Flat on the table, gravity on +Z, so a fresh part reads 1 g.
            g: [0.0, 0.0, 1.0],
            regs,
            command: 0,
        }
    }
}

impl Device for Adxl345 {
    const NAME: &'static str = "adxl345";
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

    /// Move gravity. `set:x=<g>`, `set:y=<g>`, `set:z=<g>` set one axis, and
    /// `rest` puts the chip flat on the table again.
    fn on_input(&mut self, name: &str) {
        if let Some((axis, value)) = set_value(name) {
            let value = (value as f32).clamp(-MAX_G, MAX_G);
            match axis {
                "x" => self.g[0] = value,
                "y" => self.g[1] = value,
                "z" => self.g[2] = value,
                _ => {} // Not an axis this part has.
            }
        } else if name == "rest" {
            self.g = [0.0, 0.0, 1.0];
        }
    }

    /// The guest's SPI transfer. A write stores a register; a read answers
    /// from the command byte, or from the last command if the transfer
    /// carries no new one.
    fn on_bus(&mut self, request: &BusRequest) -> Option<BusReply> {
        let Some(bus_request::Op::Spi(spi)) = &request.op else { return None };
        // A pure write: the first byte is the command, the rest the value.
        if spi.rx_len == 0 {
            if let Some(&command) = spi.tx.first() {
                self.command = command;
                if command & READ == 0 {
                    let reg = (command & 0x3F) as usize;
                    if let Some(&value) = spi.tx.get(1) {
                        self.regs[reg] = value;
                    }
                }
            }
            return None;
        }
        // A read with the command byte in the same transfer is the common
        // shape: tx is exactly one byte and it has the read bit set. Anything
        // longer is the controller's buffer, not a command.
        if spi.tx.len() == 1 && spi.tx[0] & READ != 0 {
            self.command = spi.tx[0];
        }
        let data = (0..spi.rx_len as usize)
            .map(|i| {
                let step = if self.command & MULTI != 0 { i } else { 0 };
                self.register((self.command & 0x3F) as usize + step)
            })
            .collect();
        Some(BusReply {
            correlation_id: request.correlation_id,
            ack: true,
            data,
            op: BusOp::Spi as i32,
        })
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("x", self.g[0])
                .set("y", self.g[1])
                .set("z", self.g[2]),
        )
    }
}

impl Adxl345 {
    /// Register `reg`, wrapping inside the file. `DEVID` is read-only and the
    /// axes are the live values.
    fn register(&self, reg: usize) -> u8 {
        let reg = reg % self.regs.len();
        if reg == DEVID as usize {
            DEVID_VALUE
        } else if (DATAX0 as usize..=0x37).contains(&reg) {
            self.axis_bytes()[reg - DATAX0 as usize]
        } else {
            self.regs[reg]
        }
    }

    /// The six axis bytes, little-endian pairs X, Y, Z, clamped to the part's
    /// ±2 g range.
    fn axis_bytes(&self) -> [u8; 6] {
        let mut bytes = [0u8; 6];
        for (axis, g) in self.g.iter().enumerate() {
            let counts = (g * COUNTS_PER_G).round().clamp(-512.0, 512.0) as i16;
            bytes[axis * 2..axis * 2 + 2].copy_from_slice(&counts.to_le_bytes());
        }
        bytes
    }
}

// Big enough for any read the driver makes, 6 bytes, and for the write a host
// hands over with it.
esp_part::export_part!(Adxl345, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as README.md's board wires it: CS on GPIO 7, SCK on 6, MOSI on
    /// 3, MISO on 2.
    fn chip() -> Adxl345 {
        let mut chip = Adxl345::default();
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

    /// What the driver's `write_read` gets.
    fn read(chip: &mut Adxl345, command: u8, rx_len: u32) -> Vec<u8> {
        chip.on_bus(&request(&[command], rx_len)).expect("a read is answered").data
    }

    #[test]
    fn devid_is_e5() {
        assert_eq!(read(&mut chip(), 0x80, 1), [0xE5]);
    }

    #[test]
    fn the_axes_are_little_endian_pairs_from_datax0() {
        // Their command: DATAX0 | READ | MULTI.
        let mut chip = chip();
        assert_eq!(read(&mut chip, 0x32 | 0x80 | 0x40, 6), [0x00, 0x00, 0x00, 0x00, 0x00, 0x01]);
        chip.on_input("set:x=0.5");
        chip.on_input("set:y=-1");
        chip.on_input("set:z=0");
        assert_eq!(read(&mut chip, 0xF2, 6), vec![0x80, 0x00, 0x00, 0xFF, 0x00, 0x00]);
    }

    #[test]
    fn a_read_without_the_multi_bit_answers_one_register() {
        let mut chip = chip();
        assert_eq!(read(&mut chip, 0x32 | 0x80, 2), [0x00, 0x00], "the same register twice");
    }

    #[test]
    fn a_write_then_a_read_is_answered_from_the_last_command() {
        // A host may deliver the write and the read as two transfers.
        let mut chip = chip();
        assert_eq!(chip.on_bus(&request(&[0x80], 0)), None, "a write needs no answer");
        let reply = chip.on_bus(&request(&[], 1)).unwrap();
        assert_eq!((reply.ack, reply.data), (true, vec![0xE5]));
    }

    #[test]
    fn the_inputs_move_gravity_and_stop_at_two_g() {
        let mut chip = chip();
        chip.on_input("set:x=1");
        chip.on_input("set:y=9");
        chip.on_input("wiggle");
        let reading = read(&mut chip, 0xF2, 6);
        assert_eq!(reading[..4].to_vec(), [0x00, 0x01, 0x00, 0x02], "1 g, clamped to 2 g");
        chip.on_input("rest");
        assert_eq!(
            read(&mut chip, 0xF2, 6)[4..].to_vec(),
            [0x00, 0x01],
            "back to 1 g on Z"
        );
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
            Adxl345::default().claims(),
            [Claim { what: Some(claim::What::Spi(SpiClaim::default())) }]
        );
    }

    #[test]
    fn it_shows_the_three_axes() {
        let mut chip = chip();
        chip.on_input("set:x=-0.5");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("x"), shown.get("y"), shown.get("z")),
            (Some("-0.5"), Some("0"), Some("1"))
        );
    }
}
