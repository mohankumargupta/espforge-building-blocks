//! An NXP 74HC595 8-bit serial-in, parallel-out shift register, written as a
//! virtual part.
//!
//! The firmware drives it the way it would the real chip: SER is sampled on
//! each rising SRCLK edge, the eight shifted bits move to the output latches on
//! a rising RCLK edge, SRCLR low clears the shift register, and OE low enables
//! the outputs. `drive_74hc595` 0.2.0's `load(byte)` is exactly that sequence
//! (`espforge` uses the crate through the fixture's manifest). README.md lists
//! what is left out.
//!
//! `Nxp74hc595` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_pins` included.

use esp_part::prelude::*;

/// The chip's own pin names. The board says which chip pins they are wired to,
/// as `{ pins: { OE: 4, SER: 3, SRCLR: 5, SRCLK: 6, RCLK: 7 } }`.
const OE: &str = "OE";
const SER: &str = "SER";
const SRCLR: &str = "SRCLR";
const SRCLK: &str = "SRCLK";
const RCLK: &str = "RCLK";

struct Nxp74hc595 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    oe: Option<u32>,
    ser: Option<u32>,
    srclr: Option<u32>,
    srclk: Option<u32>,
    rclk: Option<u32>,
    /// The levels of the previous `on_pins` call, so only transitions act.
    last_levels: Option<u64>,
    /// The eight bits shifted in, most recent last.
    shift: u8,
    /// The eight bits in the output latches: the parallel outputs.
    latch: u8,
}

impl Default for Nxp74hc595 {
    fn default() -> Self {
        Self {
            oe: None,
            ser: None,
            srclr: None,
            srclk: None,
            rclk: None,
            last_levels: None,
            // Power-on: the shift register and the latches are clear.
            shift: 0,
            latch: 0,
        }
    }
}

impl Device for Nxp74hc595 {
    const NAME: &'static str = "nxp74hc595";

    /// Where the board wired the part.
    fn configure(&mut self, config: &PartConfig) {
        self.oe = config.pins.get(OE).copied();
        self.ser = config.pins.get(SER).copied();
        self.srclr = config.pins.get(SRCLR).copied();
        self.srclk = config.pins.get(SRCLK).copied();
        self.rclk = config.pins.get(RCLK).copied();
    }

    /// The five lines the firmware drives. The part only watches them; it
    /// never drives a line back, so it has no exclusive claim.
    fn claims(&self) -> Vec<Claim> {
        let mut mask: u64 = 0;
        for pin in [self.oe, self.ser, self.srclr, self.srclk, self.rclk]
            .into_iter()
            .flatten()
        {
            mask |= 1 << pin;
        }
        if mask == 0 {
            Vec::new()
        } else {
            vec![watches(mask)]
        }
    }

    /// Decode the transfer from pin edges:
    ///
    /// - a falling SRCLR clears the shift register (the latches keep their
    ///   value until the next RCLK);
    /// - a rising SRCLK shifts SER into the register, MSB first;
    /// - a rising RCLK copies the register into the output latches.
    fn on_pins(&mut self, levels: u64, _now_ns: u64) {
        let (Some(ser), Some(srclr), Some(srclk), Some(rclk)) =
            (self.ser, self.srclr, self.srclk, self.rclk)
        else {
            self.last_levels = Some(levels);
            return;
        };
        let Some(previous) = self.last_levels.replace(levels) else {
            return;
        };

        // SRCLR is active low: the shift register is held clear while it is
        // low, and a falling edge clears what was shifted in.
        if bit(levels, srclr) == 0 && bit(previous, srclr) == 1 {
            self.shift = 0;
        }
        if bit(levels, srclk) == 1 && bit(previous, srclk) == 0 {
            self.shift = if bit(levels, srclr) == 0 {
                0
            } else {
                (self.shift << 1) | bit(levels, ser)
            };
        }
        if bit(levels, rclk) == 1 && bit(previous, rclk) == 0 {
            self.latch = self.shift;
        }
    }

    /// What the register holds, in the part's own words: the shift register
    /// and the output latches as hex, and whether OE lets the outputs drive.
    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("shift", format!("{:02x}", self.shift))
                .set("latch", format!("{:02x}", self.latch))
                .set(
                    "output",
                    if self.outputs_enabled() {
                        "enabled"
                    } else {
                        "disabled"
                    },
                ),
        )
    }
}

impl Nxp74hc595 {
    /// OE is active low: the outputs drive only when the board wires it and
    /// the firmware holds it low.
    fn outputs_enabled(&self) -> bool {
        match self.oe {
            Some(pin) => self.last_levels.is_some_and(|levels| bit(levels, pin) == 0),
            None => false,
        }
    }
}

/// One bit of `levels`, bit N for GPIO N.
fn bit(levels: u64, pin: u32) -> u8 {
    ((levels >> pin) & 1) as u8
}

// Big enough for the longest state line and for the labels a host sends.
esp_part::export_part!(Nxp74hc595, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as README.md's board wires it.
    const OE_PIN: u32 = 4;
    const SER_PIN: u32 = 3;
    const SRCLR_PIN: u32 = 5;
    const SRCLK_PIN: u32 = 6;
    const RCLK_PIN: u32 = 7;

    fn chip() -> Nxp74hc595 {
        let mut chip = Nxp74hc595::default();
        let pins = [
            (OE.to_string(), OE_PIN),
            (SER.to_string(), SER_PIN),
            (SRCLR.to_string(), SRCLR_PIN),
            (SRCLK.to_string(), SRCLK_PIN),
            (RCLK.to_string(), RCLK_PIN),
        ]
        .into();
        chip.configure(&PartConfig {
            pins,
            ..Default::default()
        });
        chip
    }

    /// The five watched lines at the wanted levels.
    fn levels(oe_high: bool, ser_high: bool, srclr_high: bool, srclk_high: bool, rclk_high: bool) -> u64 {
        let mut mask = 0;
        for (pin, high) in [
            (OE_PIN, oe_high),
            (SER_PIN, ser_high),
            (SRCLR_PIN, srclr_high),
            (SRCLK_PIN, srclk_high),
            (RCLK_PIN, rclk_high),
        ] {
            if high {
                mask |= 1 << pin;
            }
        }
        mask
    }

    /// The idle line, as the manifest's defaults drive it.
    fn idle() -> u64 {
        levels(true, false, true, false, false)
    }

    struct Harness {
        chip: Nxp74hc595,
        now: u64,
        line: u64,
    }

    impl Harness {
        fn new() -> Self {
            let mut harness = Self {
                chip: chip(),
                now: 0,
                line: idle(),
            };
            harness.chip.on_pins(harness.line, harness.now);
            harness
        }

        fn set(&mut self, line: u64) {
            self.now += 1;
            self.line = line;
            self.chip.on_pins(line, self.now);
        }

        /// `ShiftRegister::load`: eight bits MSB first, then latch.
        fn load(&mut self, data: u8) {
            for shift in (0..8).rev() {
                let ser_high = (data >> shift) & 1 == 1;
                self.set(levels(true, ser_high, true, false, false));
                self.set(levels(true, ser_high, true, true, false));
                self.set(levels(true, ser_high, true, false, false));
            }
            self.set(levels(true, false, true, false, true));
            self.set(levels(true, false, true, false, false));
        }

        fn state(&self) -> State {
            self.chip.state().unwrap()
        }
    }

    #[test]
    fn it_powers_up_clear_and_with_the_outputs_off() {
        let shown = Harness::new().state();
        assert_eq!(shown.get("shift"), Some("00"));
        assert_eq!(shown.get("latch"), Some("00"));
        assert_eq!(shown.get("output"), Some("disabled"));
    }

    #[test]
    fn begin_enables_the_outputs() {
        let mut harness = Harness::new();
        // `begin()`: OE low, SRCLR high.
        harness.set(levels(false, false, true, false, false));
        assert_eq!(harness.state().get("output"), Some("enabled"));
    }

    #[test]
    fn load_latches_the_byte_msb_first() {
        let mut harness = Harness::new();
        harness.set(levels(false, false, true, false, false));
        harness.load(0xAA);
        let shown = harness.state();
        assert_eq!(shown.get("latch"), Some("aa"));
        assert_eq!(shown.get("shift"), Some("aa"));
    }

    #[test]
    fn the_latest_load_wins() {
        let mut harness = Harness::new();
        harness.load(0x0F);
        harness.load(0x3C);
        assert_eq!(harness.state().get("latch"), Some("3c"));
    }

    #[test]
    fn srclr_clears_the_shift_register_but_not_the_latches() {
        let mut harness = Harness::new();
        harness.load(0xAA);
        // `output_clear()`: SRCLR low, pulse RCLK, SRCLR high again.
        harness.set(levels(false, false, false, false, false));
        assert_eq!(harness.state().get("shift"), Some("00"));
        assert_eq!(
            harness.state().get("latch"),
            Some("aa"),
            "the latches hold until RCLK"
        );
        harness.set(levels(false, false, false, false, true));
        harness.set(levels(false, false, true, false, false));
        assert_eq!(harness.state().get("latch"), Some("00"));
    }

    #[test]
    fn it_watches_the_five_lines_and_drives_none() {
        let mut mask: u64 = 0;
        for pin in [OE_PIN, SER_PIN, SRCLR_PIN, SRCLK_PIN, RCLK_PIN] {
            mask |= 1 << pin;
        }
        assert_eq!(chip().claims(), [watches(mask)]);
        assert!(
            Nxp74hc595::default().claims().is_empty(),
            "an unwired part claims nothing"
        );
    }
}
