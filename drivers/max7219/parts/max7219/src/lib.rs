//! A Maxim MAX7219 LED display driver, written as a virtual part.
//!
//! The chip is write-only and busless: the firmware drives CS low, shifts
//! sixteen bits (an 8-bit register address then an 8-bit value) out on DATA,
//! clocking each on a rising SCK edge, and raises CS to latch the word.
//! `Max7219` watches those three lines, keeps the register file and shows it
//! through `part_state`, so a test can read what the firmware wrote;
//! README.md lists what is left out.
//!
//! `Max7219` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_pins` included.

use esp_part::prelude::*;

/// The chip's own pin names. The board says which chip pins they are wired
/// to, as `{ pins: { DATA: 3, CS: 7, SCK: 6 } }`. There is no MISO because the
/// chip never drives one.
const DATA: &str = "DATA";
const CS: &str = "CS";
const SCK: &str = "SCK";

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
    data: Option<u32>,
    cs: Option<u32>,
    sck: Option<u32>,
    /// The levels of the previous `on_pins` call, so only transitions act.
    last_levels: Option<u64>,
    /// The bits clocked since CS fell, and how many. One display latches the
    /// last sixteen.
    shift: u16,
    bits: u8,
    /// The register file, at the power-on defaults: every digit clear, decode
    /// and intensity off, scan limit 0, shutdown mode on, test mode off.
    regs: [u8; REGISTERS],
}

impl Default for Max7219 {
    fn default() -> Self {
        Self {
            data: None,
            cs: None,
            sck: None,
            last_levels: None,
            shift: 0,
            bits: 0,
            regs: [0u8; REGISTERS],
        }
    }
}

impl Device for Max7219 {
    const NAME: &'static str = "max7219";

    /// Where the board wired the part.
    fn configure(&mut self, config: &PartConfig) {
        self.data = config.pins.get(DATA).copied();
        self.cs = config.pins.get(CS).copied();
        self.sck = config.pins.get(SCK).copied();
    }

    /// The three lines the firmware drives. The part only watches them; it
    /// never drives a line back, so it has no exclusive claim.
    fn claims(&self) -> Vec<Claim> {
        let mut mask: u64 = 0;
        for pin in [self.data, self.cs, self.sck].into_iter().flatten() {
            mask |= 1 << pin;
        }
        if mask == 0 {
            Vec::new()
        } else {
            vec![watches(mask)]
        }
    }

    /// Decode the bit-banged transfer from pin edges:
    ///
    /// - CS falling starts a word and clears the shift register;
    /// - a rising SCK edge while CS is low samples DATA, MSB first;
    /// - CS rising latches the last sixteen bits, if a whole word arrived.
    ///
    /// The chip has no MISO, so there is nothing to drive and no bus reply.
    fn on_pins(&mut self, levels: u64, _now_ns: u64) {
        let (Some(data), Some(cs), Some(sck)) = (self.data, self.cs, self.sck) else {
            self.last_levels = Some(levels);
            return;
        };
        let Some(previous) = self.last_levels.replace(levels) else {
            return;
        };

        let cs_low = bit(levels, cs) == 0;
        let was_low = bit(previous, cs) == 0;
        if cs_low && !was_low {
            self.shift = 0;
            self.bits = 0;
        }
        if cs_low && bit(levels, sck) == 1 && bit(previous, sck) == 0 {
            self.shift = (self.shift << 1) | bit(levels, data) as u16;
            self.bits = self.bits.saturating_add(1);
        }
        if !cs_low && was_low && self.bits >= 16 {
            let word = self.shift;
            self.write_word((word >> 8) as u8, word as u8);
        }
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

/// One bit of `levels`, bit N for GPIO N.
fn bit(levels: u64, pin: u32) -> u64 {
    (levels >> pin) & 1
}

// Big enough for the longest state line and for the labels a host sends.
esp_part::export_part!(Max7219, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as README.md's board wires it: DATA on GPIO 3, CS on 7, SCK on 6.
    const DATA_PIN: u32 = 3;
    const CS_PIN: u32 = 7;
    const SCK_PIN: u32 = 6;

    fn chip() -> Max7219 {
        let mut chip = Max7219::default();
        let pins = [
            (DATA.to_string(), DATA_PIN),
            (CS.to_string(), CS_PIN),
            (SCK.to_string(), SCK_PIN),
        ]
        .into();
        chip.configure(&PartConfig {
            pins,
            ..Default::default()
        });
        chip
    }

    /// The idle line: CS high, SCK and DATA low.
    fn idle() -> u64 {
        1 << CS_PIN
    }

    /// The three watched lines at the wanted levels.
    fn levels(cs_low: bool, sck_high: bool, data_high: bool) -> u64 {
        (if cs_low { 0 } else { 1 << CS_PIN })
            | (if sck_high { 1 << SCK_PIN } else { 0 })
            | (if data_high { 1 << DATA_PIN } else { 0 })
    }

    /// One CS assertion containing `word`, MSB first: set DATA, raise SCK to
    /// sample it, lower SCK again.
    fn clock_word(chip: &mut Max7219, now: &mut u64, word: u16) {
        for shift in (0..16).rev() {
            let high = (word >> shift) & 1 == 1;
            chip.on_pins(levels(true, false, high), *now);
            *now += 1;
            chip.on_pins(levels(true, true, high), *now);
            *now += 1;
            chip.on_pins(levels(true, false, high), *now);
            *now += 1;
        }
    }

    /// Start with an idle frame so the transitions below are edges.
    fn begin(chip: &mut Max7219, now: &mut u64) {
        chip.on_pins(idle(), *now);
        *now += 1;
        chip.on_pins(levels(true, false, false), *now);
        *now += 1;
    }

    /// Release CS: the chip latches what it shifted in.
    fn end(chip: &mut Max7219, now: &mut u64) {
        chip.on_pins(idle(), *now);
        *now += 1;
    }

    /// One complete word in its own CS assertion.
    fn write(chip: &mut Max7219, header: u8, data: u8) {
        let mut now = 0;
        begin(chip, &mut now);
        clock_word(chip, &mut now, (header as u16) << 8 | data as u16);
        end(chip, &mut now);
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
        write(&mut chip, 0x0A, 0x0F);
        write(&mut chip, 0x0C, 0x01);
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("intensity"), Some("0f"));
        assert_eq!(shown.get("power"), Some("on"));
    }

    #[test]
    fn the_eight_digit_registers_are_the_frame() {
        let mut chip = chip();
        for (digit, value) in (1u8..=8).zip([0x7E, 0x30, 0x6D, 0x79, 0x33, 0x5B, 0x5F, 0x7F]) {
            write(&mut chip, digit, value);
        }
        assert_eq!(
            chip.state().unwrap().get("digits"),
            Some("7e306d79335b5f7f")
        );
    }

    #[test]
    fn only_the_last_word_before_cs_rises_latches() {
        // A chain shifts several words before CS rises; one display keeps the
        // last one.
        let mut chip = chip();
        let mut now = 0;
        begin(&mut chip, &mut now);
        clock_word(&mut chip, &mut now, 0x0A01);
        clock_word(&mut chip, &mut now, 0x0B07);
        end(&mut chip, &mut now);
        let shown = chip.state().unwrap();
        assert_eq!(shown.get("scan"), Some("7"));
        assert_eq!(
            shown.get("intensity"),
            Some("00"),
            "the first word is shifted through"
        );
    }

    #[test]
    fn a_cs_pulse_with_no_full_word_latches_nothing() {
        let mut chip = chip();
        let mut now = 0;
        begin(&mut chip, &mut now);
        // Eight bits only: too short for an address and a value.
        for shift in (0..8).rev() {
            let high = (0x0Au16 >> shift) & 1 == 1;
            chip.on_pins(levels(true, false, high), now);
            now += 1;
            chip.on_pins(levels(true, true, high), now);
            now += 1;
            chip.on_pins(levels(true, false, high), now);
            now += 1;
        }
        end(&mut chip, &mut now);
        assert_eq!(chip.state().unwrap().get("intensity"), Some("00"));
    }

    #[test]
    fn an_address_above_0f_is_ignored() {
        let mut chip = chip();
        write(&mut chip, 0x1A, 0x0F);
        assert_eq!(chip.state().unwrap().get("intensity"), Some("00"));
    }

    #[test]
    fn it_watches_the_three_lines_and_drives_none() {
        let mut mask: u64 = 0;
        for pin in [DATA_PIN, CS_PIN, SCK_PIN] {
            mask |= 1 << pin;
        }
        assert_eq!(chip().claims(), [watches(mask)]);
        assert!(
            Max7219::default().claims().is_empty(),
            "an unwired part claims nothing"
        );
    }
}
