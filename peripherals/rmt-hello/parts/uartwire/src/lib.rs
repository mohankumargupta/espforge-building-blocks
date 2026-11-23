//! A wire from the chip's RMT output to its own UART receiver.
//!
//! `rmt-hello` drives GPIO 4 with a 115200 8N1 pulse train: a low start bit,
//! eight data bits least significant first and a high stop bit, nine ticks of
//! 1 µs each. The emulator has no wire between an RMT channel and a UART, so
//! this part is that wire: it reads the pin as runs of edges
//! ([`reads_edges`]), decodes the bytes, and puts them on the UART1 line the
//! firmware's port is wired to ([`serial`]). The firmware then prints what its
//! own UART received, and the console shows the text the waveform carried.
//!
//! An RMT receiver records where the line changes, so a byte's ten bit times
//! arrive as runs: `h`'s first three low bits are one 27 µs low, not three
//! 9 µs ones. The byte is decoded by sampling the timeline at each bit's
//! centre, not by reading symbols.

use esp_part::prelude::*;

/// The chip pin the RMT channel drives. The board says which, as
/// `{ pins: { RMT: 4 } }`.
const RMT: &str = "RMT";
/// The part's own line pins, as a serial part labels them: its TX is wired to
/// the GPIO the firmware's UART receives on, its RX to the one it transmits on.
const TX: &str = "TX";
const RX: &str = "RX";

/// One 8N1 bit: nine ticks of 1 µs, the framing `rmt-hello` transmits.
const BIT_NS: u64 = 9_000;
/// Start bit, eight data bits, stop bit.
const BYTE_NS: u64 = 10 * BIT_NS;

/// The edges kept before the oldest are dropped. A frame is about sixty
/// transitions, so this holds many frames' worth even if nobody reads them.
const MAX_EDGES: usize = 512;

#[derive(Default)]
struct UartWire {
    /// The chip pin the RMT channel drives; `None` where the board left it out.
    rmt: Option<u32>,
    /// The chip pins the UART line is on, from the part's end.
    tx: Option<u32>,
    rx: Option<u32>,
    /// Where the line changed, and the level it took: `(time_ns, high)`.
    edges: Vec<(u64, bool)>,
    /// Bytes already handed to the UART, so a re-decode sends no duplicates.
    sent: usize,
}

impl UartWire {
    /// The level the line held at `at`, from the last edge at or before it.
    fn level_at(&self, at: u64) -> Option<bool> {
        self.edges
            .iter()
            .rev()
            .find(|(time, _)| *time <= at)
            .map(|&(_, level)| level)
    }

    /// One 8N1 byte starting at `start`, if `now` is far enough along for all
    /// ten bit times to be known.
    fn byte_at(&self, start: u64, now: u64) -> Option<u8> {
        if self.level_at(start)? {
            return None; // the start bit is low
        }

        let mut byte = 0u8;
        for bit in 0..8u64 {
            // The centre of data bit `bit`, which sits just after the start bit.
            let at = start + (bit + 1) * BIT_NS + BIT_NS / 2;
            if at > now {
                return None;
            }
            if self.level_at(at)? {
                byte |= 1 << bit;
            }
        }

        // Sampling the stop bit also proves the byte had its full ten bits.
        let stop = start + 9 * BIT_NS + BIT_NS / 2;
        if stop > now {
            return None;
        }
        self.level_at(stop)?.then_some(byte)
    }

    /// Every complete byte the edges hold, in order.
    fn decode(&self, now: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut next = 0;
        let mut previous: Option<bool> = None;

        for &(start, level) in &self.edges {
            // A byte starts where the line falls for its start bit.
            let falls = !level && previous != Some(false);
            if falls && start >= next {
                if let Some(byte) = self.byte_at(start, now) {
                    bytes.push(byte);
                    next = start + BYTE_NS;
                }
            }
            previous = Some(level);
        }

        bytes
    }
}

impl Device for UartWire {
    const NAME: &'static str = "uartwire";
    const ON_MESSAGE: bool = true;

    fn configure(&mut self, config: &PartConfig) {
        self.rmt = config.pins.get(RMT).copied();
        self.tx = config.pins.get(TX).copied();
        self.rx = config.pins.get(RX).copied();
    }

    /// The UART line, and the RMT pin read as runs of edges.
    fn claims(&self) -> Vec<Claim> {
        let mut claims = vec![serial(self.tx, self.rx)];
        if let Some(pin) = self.rmt {
            claims.push(reads_edges(1 << pin));
        }
        claims
    }

    fn on_edges(&mut self, pin: u32, first_level: bool, widths_ns: &[u32], at_ns: u64) {
        if Some(pin) != self.rmt {
            return;
        }
        if self.edges.len() + widths_ns.len() + 1 > MAX_EDGES {
            self.edges.clear();
            self.sent = 0;
        }

        let mut time = at_ns;
        let mut level = first_level;
        self.edges.push((time, level));
        for &width in widths_ns {
            time += u64::from(width);
            level = !level;
            self.edges.push((time, level));
        }
    }

    /// Hand over bytes decoded since the last call. The last stop bit's high
    /// run has no closing edge, so this is also where a frame's final byte is
    /// recognised: by then `now_ns` is past its centre.
    fn poll(&mut self, now_ns: u64) -> Option<EmulatorMessage> {
        let bytes = self.decode(now_ns);
        if bytes.len() <= self.sent {
            self.sent = bytes.len();
            return None;
        }
        let fresh = bytes[self.sent..].to_vec();
        self.sent = bytes.len();
        Some(transmit(&fresh, now_ns))
    }

    fn state(&self) -> Option<State> {
        Some(State::new().set("bytes", self.sent))
    }
}

// A run of edges is up to 2,581 bytes (`PART_ABI.md`); the answer is small.
esp_part::export_part!(UartWire, buffer = 4096);

#[cfg(test)]
mod tests {
    use super::*;

    const PIN: u32 = 4;

    fn wired() -> UartWire {
        let mut wire = UartWire::default();
        wire.configure(&PartConfig {
            pins: [(RMT.to_string(), PIN), (TX.to_string(), 5), (RX.to_string(), 6)].into(),
            ..Default::default()
        });
        wire
    }

    /// The levels of `message` on the wire, one a bit.
    fn levels(message: &[u8]) -> Vec<bool> {
        let mut levels = Vec::new();
        for &byte in message {
            levels.push(false); // start
            for bit in 0..8 {
                levels.push(byte >> bit & 1 == 1);
            }
            levels.push(true); // stop
        }
        levels
    }

    /// The wire as the emulator reports it: the first level, then a width for
    /// every level change. The final level stays open, as a stop bit does.
    fn runs(message: &[u8]) -> (bool, Vec<u32>) {
        let levels = levels(message);
        let mut widths = Vec::new();
        let mut width = BIT_NS;
        for pair in levels.windows(2) {
            if pair[0] == pair[1] {
                width += BIT_NS;
            } else {
                widths.push(width as u32);
                width = BIT_NS;
            }
        }
        (levels[0], widths)
    }

    fn received(wire: &mut UartWire, now: u64) -> Vec<u8> {
        match wire.poll(now).and_then(|message| message.payload) {
            Some(esp_part::Payload::UartRx(rx)) => rx.data,
            None => Vec::new(),
            Some(other) => panic!("not a UartRx: {other:?}"),
        }
    }

    const MESSAGE: &[u8] = b"hello\n";

    #[test]
    fn the_hello_frame_arrives_on_the_uart() {
        let mut wire = wired();
        let (first, widths) = runs(MESSAGE);
        let end = 1_000 + widths.iter().sum::<u32>() as u64;
        wire.on_edges(PIN, first, &widths, 1_000);
        assert_eq!(
            received(&mut wire, end),
            b"hello",
            "the last byte's stop bit is still open"
        );
        assert_eq!(received(&mut wire, end + BIT_NS), b"\n");
    }

    #[test]
    fn a_frame_in_pieces_decodes_as_one() {
        let mut wire = wired();
        let (first, widths) = runs(MESSAGE);
        let end = 1_000 + widths.iter().sum::<u32>() as u64;
        let cut = 20;
        wire.on_edges(PIN, first, &widths[..cut], 1_000);
        let at = 1_000 + widths[..cut].iter().sum::<u32>() as u64;
        // The next run starts with the level the edge at `at` left behind.
        wire.on_edges(PIN, first ^ (cut % 2 == 1), &widths[cut..], at);
        assert_eq!(received(&mut wire, end + BIT_NS), MESSAGE);
    }

    #[test]
    fn it_reads_its_pin_and_the_line_it_is_wired_to() {
        assert_eq!(
            wired().claims(),
            [serial(Some(5), Some(6)), reads_edges(1 << PIN)]
        );
    }

    #[test]
    fn another_pin_and_unwired_parts_do_nothing() {
        let mut wire = wired();
        let (first, widths) = runs(MESSAGE);
        wire.on_edges(PIN + 1, first, &widths, 0);
        assert!(wire.edges.is_empty());

        let mut unwired = UartWire::default();
        let (first, widths) = runs(MESSAGE);
        unwired.on_edges(PIN, first, &widths, 0);
        assert!(received(&mut unwired, 1_000_000).is_empty());
    }

    #[test]
    fn noise_is_not_a_byte() {
        let mut wire = wired();
        // A short low then a high: too short to be a start bit and a byte.
        wire.on_edges(PIN, false, &[3_000, 2_000], 0);
        assert_eq!(received(&mut wire, 100_000), []);
    }

    #[test]
    fn two_frames_send_twice() {
        let mut wire = wired();
        let (first, widths) = runs(MESSAGE);
        let end = 1_000 + widths.iter().sum::<u32>() as u64;
        wire.on_edges(PIN, first, &widths, 1_000);
        assert_eq!(received(&mut wire, end + BIT_NS), MESSAGE);
        assert_eq!(received(&mut wire, end + 2 * BIT_NS), [], "nothing new");
        wire.on_edges(PIN, first, &widths, end + 10 * BYTE_NS);
        let end = end + 10 * BYTE_NS + widths.iter().sum::<u32>() as u64;
        assert_eq!(received(&mut wire, end + BIT_NS), MESSAGE);
    }
}
