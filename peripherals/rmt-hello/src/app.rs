//! The esp-hal `rmt-hello` example as an espforge app.
//!
//! `uart_tx` is the raw `<rmt_tx>` channel from `rmt-hello.espf.txt`: GPIO 4,
//! clock divider 80, idle high. At 80 MHz / 80 one tick is 1 µs, so a 115200
//! baud bit (8.68 µs) is 9 ticks and each byte is start bit + 8 data bits +
//! stop bit. The app builds the pulses; the channel does not know UART.

use esp_hal::gpio::Level;
use espforge_runtime::PulseCode;

use crate::Context;

/// 80 MHz / 80 = 1 MHz (1 tick = 1 µs), 115200 baud -> 9 ticks a bit.
const BIT_TICKS: u16 = 9;

/// 6 bytes ("hello\n") * 5 PulseCodes + 1 end marker = 31 symbols.
const FRAME_LEN: usize = 31;

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
    let msg = b"hello\n";
    let mut data = [PulseCode::end_marker(); FRAME_LEN];

    for (i, &byte) in msg.iter().enumerate() {
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
    ctx.delay.delay_ms(1_000);
}
