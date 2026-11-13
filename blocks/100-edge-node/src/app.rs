use crate::{service, Context};
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::Spawner;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

/// The BME280's 10 s interval; the SCD4x measures every 5 s.
const SAMPLE_MS: u32 = 5_000;
/// `condition: sensor.in_range: ambient_temp above: 35.0`
const OVER_TEMP_CENTI: i32 = 3_500;
/// A vent that chatters on a reading near the threshold is worse than none.
const HYSTERESIS_CENTI: i32 = 100;
/// `logger: level: INFO` is the project-level default.

static VENT_ON: AtomicBool = AtomicBool::new(false);

/// `T: 21.4 C` without `core::fmt`: prefix, one-decimal value, suffix.
fn tenths_line<'a>(prefix: &[u8], centi: i32, suffix: &[u8], buffer: &'a mut [u8; 16]) -> &'a str {
    let mut length = 0;
    let mut push = |bytes: &[u8]| {
        for byte in bytes {
            if length < buffer.len() {
                buffer[length] = *byte;
                length += 1;
            }
        }
    };

    push(prefix);
    let scaled = centi.unsigned_abs();
    push(&[
        b'0' + (scaled / 1_000) as u8,
        b'0' + ((scaled / 100) % 10) as u8,
        b'.',
        b'0' + ((scaled / 10) % 10) as u8,
    ]);
    push(suffix);

    core::str::from_utf8(&buffer[..length]).unwrap_or("?")
}

/// One page: node state plus the two readings.
async fn draw(ctx: &'static Context, temperature: &str, humidity: &str, online: bool) {
    let mut slot = device!(ctx, oled);
    let oled = match slot.init().await {
        Ok(oled) => oled,
        Err(error) => {
            ctx.logger.warn(format_args!("edge-node: {error}"));
            return;
        }
    };

    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    oled.clear();

    let status = if online { "Node: Online" } else { "Node: Offline" };
    Text::new(status, Point::new(0, 10), style)
        .draw(&mut *oled)
        .ok();
    Text::new(temperature, Point::new(0, 30), style)
        .draw(&mut *oled)
        .ok();
    Text::new(humidity, Point::new(0, 50), style)
        .draw(&mut *oled)
        .ok();

    oled.flush().await.ok();
}

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("edge-node: booted, vent closed");

    if device!(ctx, air).start().is_err() {
        ctx.logger.warn("edge-node: sensor did not start");
    }

    // The link is optional for the local automation, so a failure is a warning.
    service!(ctx, home).wait_connected().await;
    if service!(ctx, clock).sync().await.is_err() {
        ctx.logger.warn("edge-node: no wall clock; timestamps will be uptime");
    }
}

pub async fn forever(ctx: &'static Context) {
    let reading = match device!(ctx, air).read() {
        Ok(reading) => reading,
        Err(_) => {
            ctx.logger.warn("edge-node: sensor read failed");
            ctx.delay.delay_ms(SAMPLE_MS).await;
            return;
        }
    };

    let temperature_centi = i32::from(reading.temperature_centi_c);
    let humidity_centi = i32::from(reading.humidity_centi_pct);
    let temperature = temperature_centi as f32 / 100.0;
    let humidity = humidity_centi as f32 / 100.0;
    let online = service!(ctx, home).stack().config_v4().is_some();

    // `interval: 5s` + `if: sensor.in_range:` — autonomous, offline-safe.
    let vent = device!(ctx, emergency_vent);
    let was_on = VENT_ON.load(Ordering::Relaxed);
    if temperature_centi >= OVER_TEMP_CENTI + HYSTERESIS_CENTI && !was_on {
        vent.on();
        VENT_ON.store(true, Ordering::Relaxed);
        ctx.logger
            .info(format_args!("edge-node: over-temp {temperature:.1} C, vent on"));
    } else if temperature_centi <= OVER_TEMP_CENTI - HYSTERESIS_CENTI && was_on {
        vent.off();
        VENT_ON.store(false, Ordering::Relaxed);
        ctx.logger.info(format_args!(
            "edge-node: cooled to {temperature:.1} C, vent off"
        ));
    }

    // Local display and log: both keep working with no network.
    let mut temperature_line = [b' '; 16];
    let mut humidity_line = [b' '; 16];
    let temperature_text = tenths_line(b"T: ", temperature_centi, b" C", &mut temperature_line);
    let humidity_text = tenths_line(b"H: ", humidity_centi, b" %", &mut humidity_line);
    draw(ctx, temperature_text, humidity_text, online).await;
    ctx.logger.info(format_args!(
        "edge-node: {temperature:.1} C, {humidity:.1} %, link {}, vent {}, clock {}",
        if online { "up" } else { "down" },
        if VENT_ON.load(Ordering::Relaxed) { "on" } else { "off" },
        match service!(ctx, clock).now() {
            Ok(now) => now.seconds,
            Err(_) => 0,
        }
    ));

    ctx.delay.delay_ms(SAMPLE_MS).await;
}
