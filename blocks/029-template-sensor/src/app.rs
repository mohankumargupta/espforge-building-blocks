use crate::Context;

/// `update_interval` for the derived value.
const UPDATE_INTERVAL_MS: u32 = 60_000;

/// The ESPHome lambda, as a function: `t - ((100.0 - rh) / 5.0)`.
fn dew_point(temperature_c: f32, humidity_percent: f32) -> f32 {
    temperature_c - ((100.0 - humidity_percent) / 5.0)
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("template-sensor: dew point from the SCD4x reading");

    if device!(ctx, air).start().is_err() {
        ctx.logger.warn("template-sensor: sensor did not start");
    }
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;

    let reading = match device!(ctx, air).read() {
        Ok(reading) => reading,
        Err(_) => {
            ctx.logger.warn("template-sensor: sensor read failed");
            delay.delay_ms(UPDATE_INTERVAL_MS);
            return;
        }
    };

    let temperature = reading.temperature_centi_c as f32 / 100.0;
    let humidity = reading.humidity_centi_percent as f32 / 100.0;

    ctx.logger.info(format_args!(
        "Calculated Dew Point: {:.1} C",
        dew_point(temperature, humidity)
    ));

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
