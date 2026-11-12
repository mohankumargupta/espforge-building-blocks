use crate::Context;

/// `update_interval: 60s`
const UPDATE_INTERVAL_MS: u32 = 60_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("i2c-bme280: room climate");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let climate = device!(ctx, climate);

    match climate.read() {
        Ok(reading) => {
            ctx.logger.info(format_args!(
                "Room Temperature: {:.2} C",
                reading.temperature_c
            ));
            ctx.logger
                .info(format_args!("Room Humidity: {:.2} %", reading.humidity_percent));
        }
        Err(_) => ctx.logger.warn("bme280: read failed"),
    }

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
