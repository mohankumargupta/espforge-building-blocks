use crate::Context;

/// `update_interval: 30s`
const UPDATE_INTERVAL_MS: u32 = 30_000;
/// How long to wait for the 9-byte response before giving up.
const READ_TIMEOUT_MS: u32 = 500;
const POLL_MS: u32 = 2;

/// MHZ19 "read CO2" command, checksum included.
const READ_CO2: [u8; 9] = [0xFF, 0x01, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x79];

/// The sensor's checksum: 0xFF minus the sum of bytes 1..=7, plus one.
fn checksum(frame: &[u8; 9]) -> u8 {
    let sum = frame[1..8]
        .iter()
        .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
    0xFFu8.wrapping_sub(sum).wrapping_add(1)
}

/// `co2:` and `temperature:` from one response frame.
fn parse(frame: &[u8; 9]) -> Option<(u16, i16)> {
    if frame[0] != 0xFF || frame[1] != 0x86 || frame[8] != checksum(frame) {
        return None;
    }
    let co2 = u16::from_be_bytes([frame[2], frame[3]]);
    // The temperature byte is offset by 40 degrees Celsius.
    let temperature = frame[4] as i16 - 40;
    Some((co2, temperature))
}

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("uart-sensor: MHZ19 on UART1 at 9600 baud");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let uart = device!(ctx, uart1);

    if uart.write_all(&READ_CO2).is_err() {
        ctx.logger.warn("uart-sensor: command write failed");
        delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    let mut frame = [0u8; 9];
    let mut filled = 0;
    let mut waited_ms = 0;
    while filled < frame.len() && waited_ms < READ_TIMEOUT_MS {
        match uart.read(&mut frame[filled..]) {
            Ok(0) => {
                delay.delay_ms(POLL_MS);
                waited_ms += POLL_MS;
            }
            Ok(n) => filled += n,
            Err(_) => break,
        }
    }

    if filled < frame.len() {
        ctx.logger.warn("uart-sensor: response timed out");
    } else if let Some((co2, temperature)) = parse(&frame) {
        ctx.logger
            .info(format_args!("CO2 Concentration: {} ppm", co2));
        ctx.logger
            .info(format_args!("Sensor Temperature: {} C", temperature));
    } else {
        ctx.logger.warn("uart-sensor: checksum or framing error");
    }

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
