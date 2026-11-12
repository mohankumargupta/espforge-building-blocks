use crate::Context;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// The SCD4x produces one measurement every five seconds.
const UPDATE_MS: u32 = 5_000;
/// `default_target_temperature_low: 20.0`
const TARGET_LOW_CENTI: i32 = 2_000;
/// `default_target_temperature_high: 22.0`
const TARGET_HIGH_CENTI: i32 = 2_200;
/// `min_heating_off_time: 300s`
const MIN_OFF_SECS: u32 = 300;

static HEATER_ON: AtomicBool = AtomicBool::new(false);
static SECONDS_SINCE_OFF: AtomicU32 = AtomicU32::new(MIN_OFF_SECS);

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("thermostat: 20.0-22.0 C, 300 s minimum off time");

    // The part needs periodic measurement enabled before its first reading.
    if device!(ctx, air).start().is_err() {
        ctx.logger.warn("thermostat: sensor did not start");
    }
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;

    let reading = match device!(ctx, air).read() {
        Ok(reading) => reading,
        Err(_) => {
            ctx.logger.warn("thermostat: sensor read failed");
            delay.delay_ms(UPDATE_MS);
            return;
        }
    };

    let temperature = i32::from(reading.temperature_centi_c);
    let mut on = HEATER_ON.load(Ordering::Relaxed);

    if on && temperature >= TARGET_HIGH_CENTI {
        // Above the high setpoint: idle, and start the off-time clock.
        on = false;
        HEATER_ON.store(false, Ordering::Relaxed);
        SECONDS_SINCE_OFF.store(0, Ordering::Relaxed);
        device!(ctx, heater).off();
        ctx.logger.info(format_args!(
            "thermostat: idle at {:.1} C",
            temperature as f32 / 100.0
        ));
    } else if !on {
        let since_off = SECONDS_SINCE_OFF.load(Ordering::Relaxed);
        if temperature <= TARGET_LOW_CENTI && since_off >= MIN_OFF_SECS {
            on = true;
            HEATER_ON.store(true, Ordering::Relaxed);
            device!(ctx, heater).on();
            ctx.logger.info(format_args!(
                "thermostat: heating at {:.1} C",
                temperature as f32 / 100.0
            ));
        } else {
            SECONDS_SINCE_OFF.store(
                since_off.saturating_add(UPDATE_MS / 1_000),
                Ordering::Relaxed,
            );
        }
    }

    ctx.logger.info(format_args!(
        "thermostat: {:.1} C, heater {}",
        temperature as f32 / 100.0,
        if on { "on" } else { "off" }
    ));

    delay.delay_ms(UPDATE_MS);
}
