use crate::Context;

const CMD_PLAY_FOLDER: u8 = 0x0F;
const CMD_SET_VOLUME: u8 = 0x06;
/// `folder: 1, file: 1`
const DOORBELL_FOLDER: u8 = 1;
const DOORBELL_FILE: u8 = 1;
const VOLUME: u8 = 20;
/// BUSY is low while a track plays, so this waits for it to release.
const BUSY_POLL_MS: u32 = 100;
const BUSY_TIMEOUT_MS: u32 = 10_000;

/// Build a command packet: 7E FF 06 cmd 00 param_hi param_lo chk_hi chk_lo EF.
fn packet(command: u8, parameter: u16) -> [u8; 10] {
    let mut frame = [
        0x7E,
        0xFF,
        0x06,
        command,
        0x00,
        (parameter >> 8) as u8,
        parameter as u8,
        0x00,
        0x00,
        0xEF,
    ];

    // Two's complement of the version, length, command, ack and parameter bytes.
    let mut sum: u16 = 0;
    for byte in &frame[1..7] {
        sum = sum.wrapping_add(u16::from(*byte));
    }
    let checksum = 0u16.wrapping_sub(sum);
    frame[7] = (checksum >> 8) as u8;
    frame[8] = checksum as u8;
    frame
}

pub fn setup(ctx: &mut Context) {
    if device!(ctx, uart1)
        .write_all(&packet(CMD_SET_VOLUME, u16::from(VOLUME)))
        .is_ok()
    {
        ctx.logger
            .info(format_args!("dfplayer: volume set to {VOLUME}"));
    } else {
        ctx.logger.warn("dfplayer: no UART write");
    }
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let uart = device!(ctx, uart1);
    let busy = device!(ctx, player_busy);

    if device!(ctx, chime_button).is_pressed() {
        let folder_and_file = (u16::from(DOORBELL_FOLDER) << 8) | u16::from(DOORBELL_FILE);
        if uart
            .write_all(&packet(CMD_PLAY_FOLDER, folder_and_file))
            .is_err()
        {
            ctx.logger.warn("dfplayer: play command failed");
        } else {
            ctx.logger.info("Play Doorbell Chime: folder 1, track 1");
            // BUSY low means playing: report when the chime finishes.
            let mut waited_ms = 0;
            while !busy.is_pressed() && waited_ms < BUSY_TIMEOUT_MS {
                delay.delay_ms(BUSY_POLL_MS);
                waited_ms += BUSY_POLL_MS;
            }
            ctx.logger.info("dfplayer: chime finished");
        }
        // One chime per press: wait for the button to be released.
        while device!(ctx, chime_button).is_pressed() {
            delay.delay_ms(BUSY_POLL_MS);
        }
    }

    delay.delay_ms(50);
}
