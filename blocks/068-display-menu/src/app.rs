use crate::Context;
use embedded_graphics::{
    mono_font::{ascii::FONT_6X10, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    text::Text,
};

/// The `items:` list, in order: a command and a custom item.
const ITEMS: [&str; 2] = ["Reboot System", "Backlight On"];
/// One entry per line, plus a header line.
const LINE_HEIGHT: i32 = 12;
const HEADER_Y: i32 = 10;
/// Debounce for the two menu buttons.
const SAMPLE_MS: u32 = 20;
const DEBOUNCE_SAMPLES: u8 = 3;

/// Draw the whole menu: the header, then every item with a cursor.
async fn redraw(ctx: &'static Context, cursor: usize) {
    let mut slot = device!(ctx, oled);
    let oled = match slot.init().await {
        Ok(oled) => oled,
        Err(error) => {
            ctx.logger.warn(format_args!("display-menu: {error}"));
            return;
        }
    };

    let style = MonoTextStyle::new(&FONT_6X10, BinaryColor::On);
    oled.clear();

    Text::new("Menu", Point::new(0, HEADER_Y), style)
        .draw(&mut *oled)
        .ok();

    for (index, title) in ITEMS.iter().enumerate() {
        let y = HEADER_Y + LINE_HEIGHT * (index as i32 + 1);
        // The cursor is a glyph, not a colour: the panel is 1-bit.
        let marker = if index == cursor { "> " } else { "  " };
        let mut line = *b"-- -- -- -- -- -- -- --";
        let bytes = marker.as_bytes();
        line[0] = bytes[0];
        line[1] = bytes[1];
        for (offset, byte) in title.bytes().enumerate().take(14) {
            line[2 + offset] = byte;
        }
        let text = core::str::from_utf8(&line).unwrap_or("");
        Text::new(text, Point::new(0, y), style)
            .draw(&mut *oled)
            .ok();
    }

    oled.flush().await.ok();
}

pub async fn setup(ctx: &'static Context, _spawner: embassy_executor::Spawner) {
    ctx.logger
        .info("display-menu: two items, Next on GPIO4, Enter on GPIO5");
    redraw(ctx, 0).await;
}

pub async fn forever(ctx: &'static Context) {
    static mut CURSOR: usize = 0;
    static mut NEXT_STABLE: u8 = 0;
    static mut ENTER_STABLE: u8 = 0;

    // SAFETY: only `forever` touches these, and it is not re-entered.
    let (cursor, next_stable, enter_stable) =
        unsafe { (&mut CURSOR, &mut NEXT_STABLE, &mut ENTER_STABLE) };

    let next_pressed = device!(ctx, menu_next).is_pressed();
    let enter_pressed = device!(ctx, menu_enter).is_pressed();
    let mut changed = false;

    *next_stable = if next_pressed {
        next_stable.saturating_add(1)
    } else {
        0
    };
    if *next_stable == DEBOUNCE_SAMPLES {
        *cursor = (*cursor + 1) % ITEMS.len();
        changed = true;
    }

    *enter_stable = if enter_pressed {
        enter_stable.saturating_add(1)
    } else {
        0
    };
    if *enter_stable == DEBOUNCE_SAMPLES {
        match *cursor {
            // `type: command` — Reboot System.
            0 => {
                ctx.logger.warn("menu: reboot selected");
                if let Err(error) = ctx.ota.mark_app_valid() {
                    ctx.logger.warn(format_args!("menu: {error}"));
                }
                ctx.ota.reboot();
            }
            // `type: custom` — Backlight On.
            _ => {
                device!(ctx, screen_backlight).on();
                ctx.logger.info("menu: Backlight On selected");
            }
        }
        changed = true;
    }

    if changed {
        redraw(ctx, *cursor).await;
    }

    ctx.delay.delay_ms(SAMPLE_MS).await;
}
