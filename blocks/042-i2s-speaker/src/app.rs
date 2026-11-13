use crate::Context;
use embassy_executor::Spawner;

/// The alert chime is a 1.5 s clip at 16 kHz, 16-bit mono.
const CLIP_SAMPLES: usize = 24_000;

/// The intended shape: `<media_player>` accepts a whole clip, then keeps the
/// DMA FIFO fed from a timer task while `forever` carries on. Both elements
/// are proposals.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger
        .info("i2s-speaker: Alert Speaker on GPIO4/5/7 (proposed)");
}

pub async fn forever(ctx: &'static Context) {
    let speaker = device!(ctx, speaker);

    match speaker.play_blocking(CLIP_SAMPLES) {
        Ok(()) => ctx.logger.info("speaker: chime played"),
        Err(_) => ctx.logger.warn("i2s-speaker: under-run while playing"),
    }

    ctx.delay.delay_ms(60_000).await;
}
