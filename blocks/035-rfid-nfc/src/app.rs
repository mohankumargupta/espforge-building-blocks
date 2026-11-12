use crate::Context;

const PN532_ADDRESS: u8 = 0x24;
const IN_LIST_PASSIVE_TARGET: u8 = 0x4A;
/// `uid: "4B-12-8A-3F"`
const ALLOWED_UID: [u8; 4] = [0x4B, 0x12, 0x8A, 0x3F];
const POLL_MS: u32 = 500;
const STRIKE_MS: u32 = 3_000;

/// Build a PN532 command frame: 00 00 FF LEN LCS D4 cmd params DCS 00.
/// Returns the frame length written into `frame`.
fn build_command(command: u8, params: &[u8], frame: &mut [u8]) -> usize {
    let len = (params.len() + 2) as u8; // TFI + command + params
    frame[0] = 0x00;
    frame[1] = 0x00;
    frame[2] = 0xFF;
    frame[3] = len;
    frame[4] = (!len).wrapping_add(1); // LCS
    frame[5] = 0xD4; // host to PN532
    frame[6] = command;
    frame[7..7 + params.len()].copy_from_slice(params);

    let mut sum = 0xD4u8.wrapping_add(command);
    for param in params {
        sum = sum.wrapping_add(*param);
    }
    let dcs = 7 + params.len();
    frame[dcs] = (!sum).wrapping_add(1); // DCS
    frame[dcs + 1] = 0x00; // postamble
    dcs + 2
}

/// Find the response frame in an I2C read that starts with a status byte, and
/// return its payload (response command byte plus parameters).
fn response_payload(buffer: &[u8]) -> Option<&[u8]> {
    let mut i = 0;
    while i + 6 < buffer.len() {
        if buffer[i] == 0x00 && buffer[i + 1] == 0x00 && buffer[i + 2] == 0xFF {
            let len = buffer[i + 3] as usize;
            if len >= 2 && i + 6 + len <= buffer.len() && buffer[i + 5] == 0xD5 {
                // LEN covers TFI + command + params; drop the TFI.
                return Some(&buffer[i + 6..i + 5 + len]);
            }
        }
        i += 1;
    }
    None
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("rfid-nfc: PN532 on I2C0 at 0x24, allow-list armed");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let bus = device!(ctx, i2c0);
    let strike = device!(ctx, door_strike);

    // One target, 106 kbps type A.
    let mut frame = [0u8; 16];
    let length = build_command(IN_LIST_PASSIVE_TARGET, &[0x01, 0x00], &mut frame);
    if bus.write(PN532_ADDRESS, &frame[..length]).is_err() {
        ctx.logger.warn("rfid-nfc: command write failed");
        delay.delay_ms(POLL_MS);
        return;
    }

    // The reader needs a moment to run anticollision and answer.
    delay.delay_ms(30);
    let mut buffer = [0u8; 32];
    let mut received = false;
    for _ in 0..3 {
        if bus.read(PN532_ADDRESS, &mut buffer).is_ok() {
            received = true;
            break;
        }
        delay.delay_ms(10);
    }
    if !received {
        ctx.logger.warn("rfid-nfc: no I2C response");
        delay.delay_ms(POLL_MS);
        return;
    }

    let Some(payload) = response_payload(&buffer) else {
        ctx.logger.warn("rfid-nfc: malformed frame");
        delay.delay_ms(POLL_MS);
        return;
    };

    // Payload: 4B, NbTg, Tg, Len, SENS_RES(2), SEL_RES, UID length, UID...
    if payload.len() < 4 || payload[0] != 0x4B || payload[1] == 0 {
        delay.delay_ms(POLL_MS);
        return;
    }
    if payload.len() < 9 {
        ctx.logger.warn("rfid-nfc: short target data");
        delay.delay_ms(POLL_MS);
        return;
    }

    let uid_length = payload[7] as usize;
    if uid_length != ALLOWED_UID.len() || payload.len() < 8 + uid_length {
        ctx.logger
            .info(format_args!("Access Card: unexpected UID length {uid_length}"));
    } else if payload[8..8 + uid_length] == ALLOWED_UID {
        ctx.logger.info("Access Card Granted");
        strike.on();
        delay.delay_ms(STRIKE_MS);
        strike.off();
    } else {
        ctx.logger.info(format_args!(
            "Access Card Denied: {:02X}-{:02X}-{:02X}-{:02X}",
            payload[8], payload[9], payload[10], payload[11]
        ));
    }

    delay.delay_ms(POLL_MS);
}
