use crate::Context;

const HEADER: [u8; 4] = [0xF4, 0xF3, 0xF2, 0xF1];
const TAIL: [u8; 4] = [0xF8, 0xF7, 0xF6, 0xF5];
const FRAME_TYPE_TARGET: u8 = 0x02;
/// Header, 2 length bytes, type, head marker, tail.
const FRAME_OVERHEAD: usize = 4 + 2 + 2 + 4;
/// The reports arrive at 10 Hz; 100 ms of slack covers a slow UART read.
const READ_TIMEOUT_MS: u32 = 150;
const POLL_MS: u32 = 2;

/// One decoded target report.
struct Target {
    moving: bool,
    still: bool,
    moving_distance_cm: u16,
    still_distance_cm: u16,
}

fn decode(frame: &[u8]) -> Option<Target> {
    // frame starts at the head marker (type, 0xAA) and ends before the tail.
    if frame.len() < 11 || frame[0] != FRAME_TYPE_TARGET || frame[1] != 0xAA {
        return None;
    }
    Some(Target {
        moving: frame[2] & 0x01 != 0,
        still: frame[2] & 0x02 != 0,
        moving_distance_cm: u16::from_le_bytes([frame[3], frame[4]]),
        still_distance_cm: u16::from_le_bytes([frame[5], frame[6]]),
    })
}

/// Read one frame: header first, then the declared body, then the tail.
fn read_frame(ctx: &mut Context, buffer: &mut [u8]) -> Option<usize> {
    let delay = ctx.delay;
    let uart = device!(ctx, uart1);
    let mut filled = 0;
    let mut waited_ms = 0;

    while filled < buffer.len() && waited_ms < READ_TIMEOUT_MS {
        match uart.read(&mut buffer[filled..]) {
            Ok(0) => {
                delay.delay_ms(POLL_MS);
                waited_ms += POLL_MS;
            }
            Ok(n) => filled += n,
            Err(_) => return None,
        }
    }
    if filled < FRAME_OVERHEAD {
        return None;
    }
    if buffer[..4] != HEADER || buffer[filled - 4..filled] != TAIL {
        return None;
    }
    Some(filled - 8)
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("mmwave-radar: LD2410 on UART1 at 256000 baud");
}

pub fn forever(ctx: &mut Context) {
    let mut buffer = [0u8; 64];

    match read_frame(ctx, &mut buffer) {
        Some(body_length) if body_length >= 11 => {
            // Header and the two length bytes precede the body.
            if let Some(target) = decode(&buffer[6..6 + body_length]) {
                ctx.logger.info(format_args!(
                    "Target Detected: {}, moving {}, still {}",
                    target.moving || target.still,
                    target.moving_distance_cm,
                    target.still_distance_cm
                ));
                if !target.moving && !target.still {
                    ctx.logger.info("Target Detected: false");
                }
            }
        }
        Some(body_length) => ctx.logger.warn(format_args!(
            "mmwave-radar: short frame body {body_length}"
        )),
        None => ctx.logger.warn("mmwave-radar: no valid frame"),
    }

    ctx.delay.delay_ms(100);
}
