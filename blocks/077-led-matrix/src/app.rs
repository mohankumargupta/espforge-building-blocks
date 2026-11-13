use crate::Context;

const WIDTH: usize = 16;
const HEIGHT: usize = 16;
/// One frame every 100 ms is 10 fps: enough for text scrolling.
const FRAME_MS: u32 = 100;
/// How many pixels the text has scrolled across.
const TEXT: &[u8; 2] = b"HI";

/// Serpentine wiring: row 0 runs left to right, row 1 right to left, and so on.
/// This is the `pixel_mapper` lambda as a plain function.
fn pixel_index(x: usize, y: usize) -> usize {
    if y % 2 == 0 {
        y * WIDTH + x
    } else {
        y * WIDTH + (WIDTH - 1 - x)
    }
}

/// The intended shape: `<matrix>` owns the framebuffer, the layout and the
/// flush. It is a proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(format_args!(
        "led-matrix: {WIDTH}x{HEIGHT} serpentine panel, {} pixels (proposed)",
        WIDTH * HEIGHT
    ));
}

pub fn forever(ctx: &mut Context) {
    static mut OFFSET: usize = 0;

    // SAFETY: only `forever` touches this, and it is not re-entered.
    let offset = unsafe { &mut *core::ptr::addr_of_mut!(OFFSET) };

    // Draw one column of text at the current scroll offset.
    let glyph = TEXT[(*offset / WIDTH) % TEXT.len()];
    let column = *offset % WIDTH;
    // A 5x7 glyph is reduced to a lightning-rod column here: the font engine
    // is the missing piece, so this only proves the mapping.
    let ink = glyph & 0x0F != 0;

    if ink {
        for y in 0..HEIGHT {
            device!(ctx, panel).set_pixel(column, y, (0, 0, 255), pixel_index(column, y));
        }
    } else {
        for y in 0..HEIGHT {
            device!(ctx, panel).set_pixel(column, y, (0, 0, 0), pixel_index(column, y));
        }
    }
    device!(ctx, panel_strip).flush();

    *offset = (*offset + 1) % (WIDTH * TEXT.len());
    ctx.delay.delay_ms(FRAME_MS);
}
