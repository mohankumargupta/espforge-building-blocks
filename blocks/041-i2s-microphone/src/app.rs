use crate::Context;

/// 16 kHz, mono, 16-bit: one voice-assistant frame.
const SAMPLE_RATE_HZ: u32 = 16_000;
/// One DMA half-buffer worth of PCM.
const CHUNK_SAMPLES: usize = 512;
/// How long one chunk takes to fill at that rate.
const CHUNK_MS: u32 = (CHUNK_SAMPLES as u32 * 1_000) / SAMPLE_RATE_HZ;

/// The intended shape: `<microphone>` owns the I²S RX channel and its DMA
/// ring, fills a chunk, and hands it over. Neither element exists yet, so
/// `read_samples` is a proposal, not an API.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("i2s-microphone: 16 kHz mono capture on GPIO4/5/6 (proposed)");
}

pub fn forever(ctx: &mut Context) {
    let mic = device!(ctx, digital_mic);
    let mut pcm = [0i16; CHUNK_SAMPLES];

    match mic.read_samples(&mut pcm) {
        Ok(samples) => ctx.logger.info(format_args!(
            "mic: {samples} samples, {} PCM bytes",
            samples * 2
        )),
        Err(_) => ctx.logger.warn("i2s-microphone: DMA overrun"),
    }

    ctx.delay.delay_ms(CHUNK_MS);
}
