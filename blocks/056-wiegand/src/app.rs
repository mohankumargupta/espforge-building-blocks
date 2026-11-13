use crate::Context;

/// `key: 0x123456` from a 26-bit frame: 8 facility bits, 16 card bits.
const ALLOWED_CODE: u32 = 0x123456;
/// 26 bits: 1 leading parity, 24 payload, 1 trailing parity.
const FRAME_BITS: u32 = 26;
/// How long the latch stays released for a granted card.
const LATCH_MS: u32 = 3_000;
/// Wiegand ends a frame after 25 ms of silence; a slow poll can miss bits.
const POLL_MS: u32 = 1;

/// The intended shape: `<wiegand>` captures both lines as edges, rebuilds the
/// frame, checks parity, and matches the key. It is a proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("wiegand: D0 GPIO2, D1 GPIO3, expecting 26-bit frames (proposed)");
}

pub fn forever(ctx: &mut Context) {
    let reader = device!(ctx, reader);
    let latch = device!(ctx, door_latch);

    match reader.frame() {
        Some(bits) if bits.len == FRAME_BITS => {
            // Bit 0 is the leading even-parity bit, bits 1..25 the payload.
            let code = (bits.value >> 1) & 0x00FF_FFFF;
            let parity_ok = bits.even_parity() && bits.odd_parity();

            if !parity_ok {
                ctx.logger.warn("wiegand: parity error, frame dropped");
            } else if code == ALLOWED_CODE {
                ctx.logger.info("Master Keycard Detected");
                latch.on();
                ctx.delay.delay_ms(LATCH_MS);
                latch.off();
            } else {
                ctx.logger
                    .info(format_args!("wiegand: card 0x{code:06X} denied"));
            }
        }
        _ => ctx.delay.delay_ms(POLL_MS),
    }
}
