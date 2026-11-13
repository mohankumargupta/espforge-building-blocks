use crate::Context;

/// `reference_resistor: 430.0 ohm`
const REFERENCE_OHMS: f32 = 430.0;
/// `rtd_nominal_resistance: 100.0 ohm` (PT100)
const NOMINAL_OHMS: f32 = 100.0;
/// Callendar-Van Dusen coefficients for a PT100.
const A: f32 = 3.9083e-3;
const B: f32 = -5.775e-7;
/// Configuration: 3-wire, 60 Hz filter, VBIAS on, auto-convert, 1-shot off.
const CONFIG: u8 = 0b1100_0011;
const REGISTER_CONFIG: u8 = 0x00;
const REGISTER_RTD: u8 = 0x01;
/// `update_interval: 2s`
const UPDATE_INTERVAL_MS: u32 = 2_000;
/// The conversion needs ~65 ms per sample.
const CONVERSION_MS: u32 = 70;

/// Callendar-Van Dusen, for the positive range: R = R0 (1 + A t + B t^2).
/// Solved with the quadratic formula for t.
fn rtd_to_celsius(ohms: f32) -> f32 {
    let ratio = ohms / NOMINAL_OHMS - 1.0;
    (-A + (A * A - 4.0 * B * ratio).sqrt()) / (2.0 * B)
}

pub fn setup(ctx: &mut Context) {
    let spi = device!(ctx, spi2);

    // Address byte with the write bit clear, then the configuration.
    if spi.write(&[REGISTER_CONFIG, CONFIG]).is_err() {
        ctx.logger.warn("max31865-rtd: configuration write failed");
    } else {
        ctx.logger.info(
            "max31865-rtd: High-Temp Flue Probe, PT100, 3-wire, 430 ohm reference",
        );
    }
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let spi = device!(ctx, spi2);

    delay.delay_ms(CONVERSION_MS);

    // Read the two RTD registers: 15 bits plus the fault bit.
    let mut raw = [0u8; 2];
    if spi.write_read(&[REGISTER_RTD, 0x00], &mut raw).is_err() {
        ctx.logger.warn("max31865-rtd: read failed");
        delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    if raw[0] & 0x01 != 0 {
        // Faults are detailed in the fault-status register; open circuit is
        // the common one for a disconnected probe.
        ctx.logger
            .warn("High-Temp Flue Probe: RTD fault (open or short)");
        delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    let code = (u16::from(raw[0]) << 8 | u16::from(raw[1])) >> 1;
    let ohms = f32::from(code) * REFERENCE_OHMS / 32_768.0;
    let celsius = rtd_to_celsius(ohms);

    ctx.logger.info(format_args!(
        "High-Temp Flue Probe: {celsius:.2} C ({ohms:.2} ohm)"
    ));

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
