use crate::Context;

/// A PMS5003 frame is exactly 32 bytes.
const FRAME_BYTES: usize = 32;
/// The module streams once a second; two seconds of silence is a fault.
const READ_TIMEOUT_MS: u32 = 2_000;
const POLL_MS: u32 = 5;
/// The checksum covers everything before it.
const CHECKSUM_BYTES: usize = 30;

fn checksum_ok(frame: &[u8; FRAME_BYTES]) -> bool {
    let sum: u16 = frame[..CHECKSUM_BYTES]
        .iter()
        .fold(0u16, |acc, byte| acc.wrapping_add(u16::from(*byte)));
    let declared = u16::from_be_bytes([frame[30], frame[31]]);
    sum == declared
}

fn concentration(frame: &[u8], offset: usize) -> u16 {
    u16::from_be_bytes([frame[offset], frame[offset + 1]])
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("pms5003: particulate sensor on UART1 at 9600 baud");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let uart = device!(ctx, uart1);
    let mut frame = [0u8; FRAME_BYTES];
    let mut filled = 0;
    let mut waited_ms = 0;

    while filled < FRAME_BYTES && waited_ms < READ_TIMEOUT_MS {
        match uart.read(&mut frame[filled..]) {
            Ok(0) => {
                delay.delay_ms(POLL_MS);
                waited_ms += POLL_MS;
            }
            Ok(n) => filled += n,
            Err(_) => break,
        }
    }

    if filled < FRAME_BYTES || frame[0] != 0x42 || frame[1] != 0x4D {
        ctx.logger.warn("pms5003: no complete frame");
    } else if !checksum_ok(&frame) {
        ctx.logger.warn("pms5003: checksum mismatch");
    } else {
        ctx.logger.info(format_args!(
            "Particulate Matter PM1.0: {} ug/m3",
            concentration(&frame, 10)
        ));
        ctx.logger.info(format_args!(
            "Particulate Matter PM2.5: {} ug/m3",
            concentration(&frame, 12)
        ));
        ctx.logger.info(format_args!(
            "Particulate Matter PM10.0: {} ug/m3",
            concentration(&frame, 14)
        ));
    }

    // The module already produces one frame a second.
    delay.delay_ms(1_000);
}
