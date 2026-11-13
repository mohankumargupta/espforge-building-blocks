use crate::Context;

/// `change_mode_every: 8` windows of 10 s: ~80 s per voltage/current pair.
const CHANGE_MODE_EVERY: u32 = 8;
/// `update_interval: 10s`
const WINDOW_MS: u32 = 10_000;
/// Typical HLW8012 calibration at 220 V mains with the stock shunt.
const POWER_PER_HZ: f32 = 0.6;
const VOLTAGE_PER_HZ: f32 = 0.14;
const CURRENT_PER_HZ: f32 = 0.003;

/// The intended shape: `<pcnt>` counts edges in hardware and hands a window's
/// total over, so the app only converts frequencies. It is a proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "hlw8012-energy: CF GPIO3, CF1 GPIO4, SEL GPIO2, 10 s windows (proposed)",
    );
}

pub fn forever(ctx: &mut Context) {
    static mut WINDOWS: u32 = 0;

    // SAFETY: only `forever` touches this, and it is not re-entered.
    let windows = unsafe { &mut *core::ptr::addr_of_mut!(WINDOWS) };
    *windows += 1;

    // SEL high selects current; low selects voltage.
    let measuring_current = (*windows / CHANGE_MODE_EVERY) % 2 == 1;
    let sel = device!(ctx, sel);
    if measuring_current {
        sel.on();
    } else {
        sel.off();
    }

    let power_hz = device!(ctx, cf_counter).frequency_hz(WINDOW_MS);
    let secondary_hz = device!(ctx, cf1_counter).frequency_hz(WINDOW_MS);

    let watts = power_hz * POWER_PER_HZ;
    let amps = secondary_hz * CURRENT_PER_HZ;
    let volts = secondary_hz * VOLTAGE_PER_HZ;

    ctx.logger.info(format_args!("Active Power: {watts:.1} W"));
    if measuring_current {
        ctx.logger.info(format_args!("Mains Current: {amps:.2} A"));
    } else {
        ctx.logger.info(format_args!("Mains Voltage: {volts:.0} V"));
    }

    ctx.delay.delay_ms(WINDOW_MS);
}
