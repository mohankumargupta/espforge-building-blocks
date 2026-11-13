use crate::Context;

/// `calibrate_linear`: 2.45 V in free air is 0 %, 1.10 V in water is 100 %.
const DRY_VOLTS: f32 = 2.45;
const WET_VOLTS: f32 = 1.10;
/// An 11 dB attenuated 12-bit ADC: full scale is ~2.45 V.
const ADC_FULL_SCALE_V: f32 = 2.45;
const ADC_MAX_COUNTS: f32 = 4_095.0;
/// `update_interval: 30s`
const UPDATE_INTERVAL_MS: u32 = 30_000;

/// The intended shape: `<analog_sensor>` returns millivolts and the app
/// calibrates. It is a proposal; there is no ADC in the vocabulary.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(format_args!(
        "soil-moisture: GPIO0 at 11 dB, {DRY_VOLTS} V dry / {WET_VOLTS} V wet (proposed)"
    ));
}

pub fn forever(ctx: &mut Context) {
    let millivolts = match device!(ctx, soil).read_millivolts() {
        Ok(millivolts) => millivolts,
        Err(_) => {
            ctx.logger.warn("soil-moisture: ADC read failed");
            ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
            return;
        }
    };

    let volts = f32::from(millivolts) / 1_000.0;
    // Sanity check against the configured full scale, as an ADC driver would.
    let counts = (volts / ADC_FULL_SCALE_V * ADC_MAX_COUNTS).round();

    let percent = (volts - DRY_VOLTS) / (WET_VOLTS - DRY_VOLTS) * 100.0;
    let percent = percent.clamp(0.0, 100.0);

    ctx.logger.info(format_args!(
        "Soil Moisture: {percent:.0} % ({volts:.2} V, {counts:.0} counts)"
    ));

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
