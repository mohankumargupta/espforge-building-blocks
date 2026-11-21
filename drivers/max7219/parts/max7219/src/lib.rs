//! A Maxim MAX7219 LED display driver, written as a virtual part.
//!
//! The chip is write-only: CS is asserted, sixteen bits are clocked in — an
//! 8-bit register address then an 8-bit value — and CS is released, latching
//! the word. Nothing comes back on MISO. `Max7219` keeps that register file
//! and shows it through `part_state`, so a test can read what the firmware
//! wrote; README.md lists what is left out.
//!
//! `Max7219` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names. The board says which chip pins they are wired
/// to, as `{ pins: { CS: 7, SCK: 6, MOSI: 3 } }`. DIN is the crate's `MOSI`,
/// and there is no MISO because the chip never drives one.
const CS: &str = "CS";
const SCK: &str = "SCK";
const MOSI: &str = "MOSI";

/// The eight digit registers: one 8x8 row, or one 7-segment digit, each.
const DIGITS: usize = 8;
const DIGIT_0: usize = 1;
/// Decode mode (0x09), intensity (0x0A), scan limit (0x0B), shutdown (0x0C)
/// and display test (0x0F). 0x0D and 0x0E are reserved no-ops.
const DECODE_MODE: usize = 0x09;
const INTENSITY: usize = 0x0A;
const SCAN_LIMIT: usize = 0x0B;
const SHUTDOWN: usize = 0x0C;
const DISPLAY_TEST: usize = 0x0F;
/// Registers 0x00..=0x0F; the chip ignores a higher address.
const REGISTERS: usize = 16;

struct Max7219 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    cs: Option<u32>,
    sck: Option<u32>,
    mosi: Option<u32>,
    /// The register file, at the power-on defaults: every digit clear, decode
    /// and intensity off, scan limit 0, shutdown mode on, test mode off.
    regs: [u8; REGISTERS],
}

impl Default for Max7219 {
    fn default() -> Self {
        Self {
            cs: None,
            sck: None,
            mosi: None,
            regs: [0u8; REGISTERS],
        }
    }
}

impl Device for Max7219 {
    const NAME: &'static str = "max7219";
    const ON_BUS: bool = true;

    /// Where the board wired the part.
    fn configure(&mut self, config: &PartConfig) {
        self.cs = config.pins.get(CS).copied();
        self.sck = config.pins.get(SCK).copied();
        self.mosi = config.pins.get(MOSI).copied();
    }

    /// The chip select this part is selected by, plus the clock and data pads.
    /// espforge drives CS as a plain GPIO, so the pad carries no controller
    /// chip select; the emulator takes the bus from whichever of `SCK` and
    /// `MOSI` the firmware put a bus line on. There is no MISO pin.
    fn claims(&self) -> Vec<Claim> {
        let spi = SpiClaim {
            cs_pin: self.cs,
            clk_pin: self.sck,
            mosi_pin: self.mosi,
            ..Default::default()
        };
        vec![Claim {
            what: Some(claim::What::Spi(spi)),
        }]
    }

    /// The guest's SPI transfer. One transfer is one CS assertion, and the
    /// chip latches the last sixteen bits clocked before CS rises, whatever
    /// the byte alignment; earlier words are shifted through.
    fn on_bus(&mut self, request: &BusRequest) -> Option<BusReply> {
        let Some(bus_request::Op::Spi(spi)) = &request.op else {
            return None;
        };
        if spi.tx.len() >= 2 {
            let end = spi.tx.len();
            self.write_word(spi.tx[end - 2], spi.tx[end - 1]);
        }
        // The chip never drives MISO, but the controller can still clock a
        // transfer with bytes read back (a write on esp-hal's path does).
        // Answer it as an undriven line: `ack = false` means the master reads
        // 0xFF, as it would from a chip with no MISO pin.
        if spi.rx_len > 0 {
            return Some(BusReply {
                correlation_id: request.correlation_id,
                ack: false,
                data: Vec::new(),
                op: BusOp::Spi as i32,
            });
        }
        None
    }

    /// What the register file holds, in the part's own words: the eight digit
    /// bytes as hex, the control registers, and whether the chip is out of
    /// shutdown mode.
    fn state(&self) -> Option<State> {
        let digits: String = self.regs[DIGIT_0..DIGIT_0 + DIGITS]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Some(
            State::new()
                .set("decode", format!("{:02x}", self.regs[DECODE_MODE]))
                .set("digits", digits)
                .set("intensity", format!("{:02x}", self.regs[INTENSITY]))
                .set("power", if self.power_on() { "on" } else { "off" })
                .set("scan", self.regs[SCAN_LIMIT].min(7))
                .set(
                    "test",
                    if self.regs[DISPLAY_TEST] != 0 {
                        "on"
                    } else {
                        "off"
                    },
                ),
        )
    }
}

impl Max7219 {
    /// Latch one word. The address is a full byte; only 0x00..=0x0F exists,
    /// and 0x0D/0x0E are the chip's no-op registers.
    fn write_word(&mut self, header: u8, data: u8) {
        if (header as usize) < REGISTERS {
            self.regs[header as usize] = data;
        }
    }

    /// The chip is on when its shutdown register is non-zero.
    fn power_on(&self) -> bool {
        self.regs[SHUTDOWN] != 0
    }
}

// Big enough for the driver's longest write — two bytes per word — and for
// the write a host hands over with it.
esp_part::export_part!(Max7219, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as README.md's board wires it: CS on GPIO 7, SCK on 6, MOSI on
    /// 3.
    fn chip() -> Max7219 {
        let mut chip = Max7219::default();
        let pins = [
            (CS.to_string(), 7),
            (SCK.to_string(), 6),
            (MOSI.to_string(), 3),
        ]
        .into();
        chip.configure(&PartConfig {
            pins,
            ..Default::default()
        });
        chip
    }

    /// One CS assertion carrying `tx`; a write reads nothing.
    fn write(chip: &mut Max7219, tx: &[u8]) {
        let spi = SpiTransfer {
            bus: 2,
            tx: tx.to_vec(),
            rx_len: 0,
            ..Default::default()
        };
        let request = BusRequest {
            correlation_id: 7,
            op: Some(bus_request::Op::Spi(spi)),
        };
        assert_eq!(chip.on_bus(&request), None, "a write is never answered");
    }

    #[test]
    fn it_powers_up_with_every_digit_clear_and_the_chip_off() {
        let shown = chip().state().unwrap();
        assert_eq!(
            (
                shown.get("decode"),
                shown.get("digits"),
                shown.get("intensity"),
                shown.get("power"),
                shown.get("scan"),
                shown.get("test"),
            ),
            (
                Some("00"),
                Some("0000000000000000"),
                Some("00"),
                Some("off"),
                Some("0"),
                Some("off"),
            )
        );
    }

    #[test]
    fn a_word_latches_the_register_it_names() {
        let mut chip = chip();
        write(&mut chip, &[0x0A, 0x0F]);
        write(&mut chip, &[0x0C, 0x01]);
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("intensity"), Some("0f"));
        assert_eq!(shown.get("power"), Some("on"));
    }

    #[test]
    fn the_eight_digit_registers_are_the_frame() {
        let mut chip = chip();
        for (digit, value) in (1u8..=8).zip([0x7E, 0x30, 0x6D, 0x79, 0x33, 0x5B, 0x5F, 0x7F]) {
            write(&mut chip, &[digit, value]);
        }
        assert_eq!(
            chip.state().unwrap().get("digits"),
            Some("7e306d79335b5f7f")
        );
    }

    #[test]
    fn only_the_last_word_of_a_transfer_latches() {
        // A chain shifts several words before CS rises; one display keeps the
        // last one.
        let mut chip = chip();
        write(&mut chip, &[0x0A, 0x01, 0x0B, 0x07]);
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("scan"), Some("7"));
        assert_eq!(
            shown.get("intensity"),
            Some("00"),
            "the first word is shifted through"
        );
    }

    #[test]
    fn the_last_sixteen_bits_latch_whatever_the_alignment() {
        // Three bytes clocked before CS rises: the chip keeps `05 0B`, so
        // digit 5 gets 0x0B and the intensity word was shifted through.
        let mut chip = chip();
        write(&mut chip, &[0x0A, 0x05, 0x0B]);
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("intensity"), Some("00"));
        assert_eq!(shown.get("digits"), Some("000000000b000000"));
    }

    #[test]
    fn an_address_above_0f_is_ignored() {
        let mut chip = chip();
        write(&mut chip, &[0x1A, 0x0F]);
        assert_eq!(chip.state().unwrap().get("intensity"), Some("00"));
    }

    #[test]
    fn a_transfer_that_reads_is_answered_as_an_undriven_line() {
        // One MAX7219 has no MISO, but the controller can still clock a
        // transfer with bytes read back; the part must answer it.
        let mut chip = chip();
        let spi = SpiTransfer {
            bus: 2,
            tx: vec![0x0A, 0x0F],
            rx_len: 2,
            ..Default::default()
        };
        let request = BusRequest {
            correlation_id: 7,
            op: Some(bus_request::Op::Spi(spi)),
        };
        let reply = chip.on_bus(&request).expect("a read is answered");
        assert_eq!(
            (reply.correlation_id, reply.ack, reply.data, reply.op),
            (7, false, Vec::new(), BusOp::Spi as i32)
        );
        assert_eq!(chip.state().unwrap().get("intensity"), Some("0f"));
    }

    #[test]
    fn it_claims_the_chip_select_and_both_bus_pads_but_no_miso() {
        let spi = SpiClaim {
            cs_pin: Some(7),
            clk_pin: Some(6),
            mosi_pin: Some(3),
            ..Default::default()
        };
        assert_eq!(
            chip().claims(),
            [Claim {
                what: Some(claim::What::Spi(spi))
            }]
        );
        // Unwired: claimed without a chip select, which the handshake refuses.
        assert_eq!(
            Max7219::default().claims(),
            [Claim {
                what: Some(claim::What::Spi(SpiClaim::default()))
            }]
        );
    }
}
