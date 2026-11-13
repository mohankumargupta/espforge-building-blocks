use crate::Context;
use core::sync::atomic::{AtomicU8, Ordering};

/// `full_update_every: 30`: a full refresh clears ghosting every thirtieth page.
const FULL_UPDATE_EVERY: u8 = 30;
/// A full refresh takes ~2 s on a 2.9 inch panel; a partial one ~300 ms.
const PAGE_INTERVAL_MS: u32 = 10_000;

static PAGES: AtomicU8 = AtomicU8::new(0);

/// The intended shape: `<epaper>` owns the init sequence, the LUT choice and
/// the BUSY wait, and exposes a framebuffer like `<ssd1306>` does. It is a
/// proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "epaper-display: 2.9in panel on SPI2, BUSY on GPIO4 (proposed)",
    );
}

pub fn forever(ctx: &mut Context) {
    let pages = PAGES.fetch_add(1, Ordering::Relaxed);
    let full = pages % FULL_UPDATE_EVERY == 0;

    let panel = device!(ctx, panel);
    let result = if full {
        panel.refresh_full()
    } else {
        panel.refresh_partial(10, 10, 200, 24)
    };

    match result {
        Ok(()) => ctx.logger.info(format_args!(
            "panel: {} refresh #{pages}",
            if full { "full" } else { "partial" }
        )),
        Err(_) => ctx.logger.warn("epaper-display: BUSY never released"),
    }

    ctx.delay.delay_ms(PAGE_INTERVAL_MS);
}
