use crate::Context;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// `target_humidity: 65%`
const TARGET_CENTI: i32 = 6_500;
/// `target_humidity_range: 2.0%`
const RANGE_CENTI: i32 = 200;
/// A compressor needs time between starts; the humidifier's pump does too.
const MIN_OFF_SECS: u32 = 60;
/// The SCD4x measures every five seconds.
const UPDATE_MS: u32 = 5_000;

static MISTING: AtomicBool = AtomicBool::new(false);
static SECONDS_SINCE_OFF: AtomicU32 = AtomicU32::new(MIN_OFF_SECS);

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("humidifier: Cigar Humidor, 65% +/- 2%, 60 s minimum off");

    if device!(ctx, air).start().is_err() {
        ctx.logger.warn("humidifier: sensor did not start");
    }
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;

    let reading = match device!(ctx, air).read() {
        Ok(reading) => reading,
        Err(_) => {
            ctx.logger.warn("humidifier: sensor read failed");
            delay.delay_ms(UPDATE_MS);
            return;
        }
    };

    let humidity = i32::from(reading.humidity_centi_pct);
    let misting = MISTING.load(Ordering::Relaxed);
    let since_off = SECONDS_SINCE_OFF.load(Ordering::Relaxed);

    if misting && humidity >= TARGET_CENTI + RANGE_CENTI {
        MISTING.store(false, Ordering::Relaxed);
        SECONDS_SINCE_OFF.store(0, Ordering::Relaxed);
        device!(ctx, mist_generator).off();
        ctx.logger.info(format_args!(
            "humidifier: idle at {:.1} %",
            humidity as f32 / 100.0
        ));
    } else if !misting && humidity <= TARGET_CENTI - RANGE_CENTI && since_off >= MIN_OFF_SECS {
        MISTING.store(true, Ordering::Relaxed);
        device!(ctx, mist_generator).on();
        ctx.logger.info(format_args!(
            "humidifier: misting at {:.1} %",
            humidity as f32 / 100.0
        ));
    } else if !misting {
        SECONDS_SINCE_OFF.store(since_off.saturating_add(UPDATE_MS / 1_000), Ordering::Relaxed);
    }

    ctx.logger.info(format_args!(
        "humidor: {:.1} % (target {:.1} +/- {:.1}), mist {}",
        humidity as f32 / 100.0,
        TARGET_CENTI as f32 / 100.0,
        RANGE_CENTI as f32 / 100.0,
        if MISTING.load(Ordering::Relaxed) { "on" } else { "off" }
    ));

    delay.delay_ms(UPDATE_MS);
}
