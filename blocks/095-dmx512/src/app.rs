use crate::Context;

/// A DMX universe: one start code plus 512 channel slots.
const UNIVERSE_BYTES: usize = 513;
/// A floating point fade from black to full over a few seconds.
const FADE_MS: u32 = 5_000;

/// The intended shape: `<dmx512>` owns the frame buffer and the BREAK timing,
/// and `<dmx_output>` writes one channel. It is a proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "dmx512: 250 kbaud 8N2 on GPIO0, channel 1 -> spotlight (proposed)",
    );
}

pub fn forever(ctx: &mut Context) {
    let mut frame = [0u8; UNIVERSE_BYTES];
    // Channel 1 is the first byte after the start code.
    const CHANNEL_1: usize = 1;
    frame[CHANNEL_1] = 0;

    // Fade the first channel up over the loop, then start again from black.
    for level in (0..=255u8).step_by(51) {
        frame[CHANNEL_1] = level;

        match device!(ctx, stage_dmx).send(&frame) {
            Ok(()) => ctx.logger.info(format_args!(
                "dmx: channel 1 = {level} (BREAK needed before this frame)"
            )),
            Err(_) => ctx.logger.warn("dmx512: UART write failed"),
        }

        ctx.delay.delay_ms(FADE_MS / 6);
    }
}
