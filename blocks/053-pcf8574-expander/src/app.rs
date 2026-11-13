use crate::Context;

/// `address: 0x20`
const ADDRESS: u8 = 0x20;
/// `number: 0` on the expander, `inverted: true`: the relay is active low.
const RELAY_BIT: u8 = 1 << 0;
/// Bits kept high for the pins used as inputs (quasi-bidirectional I/O).
const INPUT_MASK: u8 = 0xF0;
/// A quarter-second toggle is easy to see on a relay and on a meter.
const TOGGLE_MS: u32 = 250;

/// Write the whole port latch in one transaction.
fn write_port(ctx: &mut Context, latch: u8) -> bool {
    device!(ctx, i2c0).write(ADDRESS, &[latch]).is_ok()
}

/// Read the pin levels: inputs as driven, outputs as the latch.
fn read_port(ctx: &mut Context) -> Option<u8> {
    let mut levels = [0u8; 1];
    device!(ctx, i2c0)
        .read(ADDRESS, &mut levels)
        .ok()
        .map(|()| levels[0])
}

pub fn setup(ctx: &mut Context) {
    // All outputs high (relays released), inputs high so they can be pulled low.
    if write_port(ctx, INPUT_MASK) {
        ctx.logger
            .info("pcf8574-expander: 0x20 initialised, relay 1 released");
    } else {
        ctx.logger.warn("pcf8574-expander: no ACK from 0x20");
    }
}

pub fn forever(ctx: &mut Context) {
    static mut LATCH: u8 = INPUT_MASK;
    static mut ON: bool = false;

    // SAFETY: only `forever` touches these, and it is not re-entered.
    let (latch, on) = unsafe { (&mut LATCH, &mut ON) };

    *on = !*on;
    // `inverted: true` — the relay closes when the bit goes low.
    if *on {
        *latch &= !RELAY_BIT;
    } else {
        *latch |= RELAY_BIT;
    }

    if write_port(ctx, *latch) {
        ctx.logger.info(format_args!(
            "Expander Relay 1: {} ({:#010b})",
            if *on { "on" } else { "off" },
            *latch
        ));
    } else {
        ctx.logger.warn("pcf8574-expander: write failed");
    }

    if let Some(levels) = read_port(ctx) {
        ctx.logger
            .info(format_args!("expander inputs: {levels:#010b}"));
    }

    ctx.delay.delay_ms(TOGGLE_MS);
}
