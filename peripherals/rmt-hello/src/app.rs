//! The esp-hal `rmt-hello` example, read back through a UART.
//!
//! `uart_tx` is the raw `<rmt_tx>` channel from `rmt-hello.espf.txt`: GPIO 4,
//! clock divider 80, idle high. At 80 MHz / 80 one tick is 1 µs, so a 115200
//! baud bit (8.68 µs) is 9 ticks and each byte is start bit + 8 data bits +
//! stop bit. The app builds the pulses; the channel does not know UART.
//!
//! `uart1` is the chip's own second UART, its RX on GPIO 5. The virtual wire
//! in `parts/uartwire` watches GPIO 4 and puts the bytes it decodes on that
//! line, so the port receives what the RMT channel transmitted. Printing what
//! the UART read puts the text on the console, where the waveform started.

use esp_hal::gpio::Level;
use espforge_runtime::PulseCode;

use crate::Context;

/// 80 MHz / 80 = 1 MHz (1 tick = 1 µs), 115200 baud -> 9 ticks a bit.
const BIT_TICKS: u16 = 9;

/// 6 bytes ("hello\n") * 5 PulseCodes + 1 end marker = 31 symbols.
const FRAME_LEN: usize = 31;

/// A line from the UART; "hello\n" is six bytes.
const LINE_LEN: usize = 32;

fn byte_to_pulses(byte: u8) -> [PulseCode; 5] {
    let bit = |i: u8| {
        if (byte >> i) & 1 == 1 {
            Level::High
        } else {
            Level::Low
        }
    };

    [
        PulseCode::new(Level::Low, BIT_TICKS, bit(0), BIT_TICKS), // Start + D0
        PulseCode::new(bit(1), BIT_TICKS, bit(2), BIT_TICKS),     // D1 + D2
        PulseCode::new(bit(3), BIT_TICKS, bit(4), BIT_TICKS),     // D3 + D4
        PulseCode::new(bit(5), BIT_TICKS, bit(6), BIT_TICKS),     // D5 + D6
        PulseCode::new(bit(7), BIT_TICKS, Level::High, BIT_TICKS), // D7 + Stop
    ]
}

/// The whole message as pulse codes, ending on an end marker.
fn hello_frame() -> [PulseCode; FRAME_LEN] {
    let mut data = [PulseCode::end_marker(); FRAME_LEN];

    for (i, &byte) in b"hello\n".iter().enumerate() {
        data[i * 5..(i + 1) * 5].copy_from_slice(&byte_to_pulses(byte));
    }

    data
}

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: rmt hello boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, uart_tx).transmit(&hello_frame()) {
        Ok(()) => ctx.logger.info("TEST: rmt hello sent"),
        Err(_) => ctx.logger.error("TEST: rmt hello failed"),
    }

    let mut line = [0u8; LINE_LEN];
    match device!(ctx, uart1).read_line(&mut line) {
        Ok(n) => {
            let text = core::str::from_utf8(&line[..n]).unwrap_or("<not utf-8>");
            ctx.logger.info(format_args!(
                "TEST: uart {:?}",
                text.trim_end_matches(['\r', '\n'])
            ));
        }
        Err(_) => ctx.logger.error("TEST: uart read failed"),
    }

    ctx.delay.delay_ms(1_000);
}
