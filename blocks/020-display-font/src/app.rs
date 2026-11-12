use crate::{service, Context};
use embassy_executor::Spawner;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

/// `it.strftime("%H:%M:%S")`: espforge has no strftime, so the digits are
/// formatted by hand into a stack buffer.
fn hhmmss(seconds_in_day: u64, out: &mut [u8; 8]) -> &str {
    let hour = (seconds_in_day / 3_600) % 24;
    let minute = (seconds_in_day % 3_600) / 60;
    let second = seconds_in_day % 60;

    out[0] = b'0' + (hour / 10) as u8;
    out[1] = b'0' + (hour % 10) as u8;
    out[2] = b':';
    out[3] = b'0' + (minute / 10) as u8;
    out[4] = b'0' + (minute % 10) as u8;
    out[5] = b':';
    out[6] = b'0' + (second / 10) as u8;
    out[7] = b'0' + (second % 10) as u8;

    core::str::from_utf8(&out[..]).unwrap_or("--:--:--")
}

/// One page redraw: clear the framebuffer, draw both lines, flush it.
async fn draw(ctx: &'static Context) {
    let mut slot = device!(ctx, oled);
    let oled = match slot.init().await {
        Ok(oled) => oled,
        Err(error) => {
            ctx.logger.warn(format_args!("display-font: {error}"));
            return;
        }
    };

    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    oled.clear();

    Text::new("Node Online", Point::new(0, 10), style)
        .draw(&mut *oled)
        .ok();

    let mut buffer = [b'-'; 8];
    let time = match service!(ctx, clock).now() {
        Ok(now) => hhmmss(now.seconds % 86_400, &mut buffer),
        Err(_) => "--:--:--",
    };
    Text::new(time, Point::new(0, 30), style)
        .draw(&mut *oled)
        .ok();

    oled.flush().await.ok();
}

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    logger.info("display-font: connecting...");
    service!(ctx, home).wait_connected().await;

    if service!(ctx, clock).sync().await.is_err() {
        logger.warn("display-font: NTP sync failed; the clock line stays blank");
    }

    draw(ctx).await;
}

pub async fn forever(ctx: &'static Context) {
    draw(ctx).await;
    ctx.delay.delay_ms(1000).await;
}
