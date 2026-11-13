use crate::{service, Context};
use embassy_executor::Spawner;

/// `interval: 10s`
const RECORD_MS: u32 = 10_000;
const LOG_PATH: &str = "/telemetry.csv";

/// The intended shape: `<sd_card>` mounts the card and exposes `open`/`write`/
/// `close`, with a flush that survives a power cut. Everything under it is a
/// proposal.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("sd-card-logging: mounting the card...");

    match device!(ctx, sd_logger).mount() {
        Ok(()) => ctx.logger.info("sd-card-logging: FAT filesystem mounted"),
        Err(_) => ctx.logger.warn("sd-card-logging: no card, logging disabled"),
    }

    if service!(ctx, clock).sync().await.is_err() {
        ctx.logger.warn("sd-card-logging: no wall clock; records use uptime");
    }
}

pub async fn forever(ctx: &'static Context) {
    let card = device!(ctx, sd_logger);

    let temperature = match device!(ctx, air).read() {
        Ok(reading) => f32::from(reading.temperature_centi_c) / 100.0,
        Err(_) => {
            ctx.logger.warn("sd-card-logging: sensor read failed");
            ctx.delay.delay_ms(RECORD_MS).await;
            return;
        }
    };

    let timestamp = match service!(ctx, clock).now() {
        Ok(now) => now.seconds as i64,
        Err(_) => 0,
    };

    let mut record = [0u8; 48];
    let line = format_record(timestamp, temperature, &mut record);

    match card.append(LOG_PATH, line.as_bytes()) {
        Ok(()) => ctx.logger.info(format_args!("sd: {line}")),
        Err(_) => ctx.logger.warn("sd-card-logging: append failed"),
    }

    ctx.delay.delay_ms(RECORD_MS).await;
}

/// `fprintf(f, "%d,%.2f\n", ...)` without a formatter: integer timestamp, then
/// the temperature with two decimals, then a newline.
fn format_record(timestamp: i64, temperature: f32, buffer: &mut [u8; 48]) -> &str {
    let mut length = 0;
    let mut write = |bytes: &[u8]| {
        let end = (length + bytes.len()).min(buffer.len());
        buffer[length..end].copy_from_slice(&bytes[..end - length]);
        length = end;
    };

    let mut digits = [0u8; 20];
    let mut count = 0;
    let mut value = timestamp.unsigned_abs();
    loop {
        digits[count] = b'0' + (value % 10) as u8;
        count += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    if timestamp < 0 {
        write(b"-");
    }
    for index in (0..count).rev() {
        write(&[digits[index]]);
    }

    write(b",");
    let scaled = (temperature * 100.0).round() as i32;
    let mut whole = [0u8; 10];
    let mut whole_len = 0;
    let mut rest = (scaled / 100).unsigned_abs();
    loop {
        whole[whole_len] = b'0' + (rest % 10) as u8;
        whole_len += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    if scaled < 0 {
        write(b"-");
    }
    for index in (0..whole_len).rev() {
        write(&[whole[index]]);
    }
    write(b".");
    let fraction = (scaled % 100).unsigned_abs();
    write(&[b'0' + (fraction / 10) as u8, b'0' + (fraction % 10) as u8]);
    write(b"\n");

    core::str::from_utf8(&buffer[..length]).unwrap_or("")
}
