use crate::Context;

/// NMEA sentences end with `*` and two hex digits; a module can emit ~1 kB/s.
const LINE_BYTES: usize = 96;
/// The longest sentence this parser cares about has 20 comma-separated fields.
const FIELDS: usize = 20;
/// How many sentence reads make up one fix (the module streams at 1 Hz).
const SENTENCES_PER_FIX: u32 = 20;

/// XOR checksum of the bytes between `$` and `*`.
fn checksum_ok(sentence: &[u8]) -> bool {
    let body = match sentence.iter().position(|&b| b == b'$') {
        Some(start) => &sentence[start + 1..],
        None => return false,
    };
    let star = match body.iter().position(|&b| b == b'*') {
        Some(star) => star,
        None => return false,
    };
    if body.len() < star + 3 {
        return false;
    }
    let expected = core::str::from_utf8(&body[star + 1..star + 3])
        .ok()
        .and_then(|hex| u8::from_str_radix(hex, 16).ok());

    let sum = body[..star].iter().fold(0u8, |acc, byte| acc ^ byte);
    expected == Some(sum)
}

/// Split a sentence on `,` into a fixed array, without allocating.
fn split_fields<'a>(sentence: &'a str, out: &mut [&'a str; FIELDS]) -> usize {
    let mut count = 0;
    let mut start = 0;
    for (index, byte) in sentence.bytes().enumerate() {
        if byte == b',' && count < FIELDS {
            out[count] = &sentence[start..index];
            count += 1;
            start = index + 1;
        }
    }
    if count < FIELDS {
        // The last field still carries its `*` checksum suffix; callers that
        // need it as a number strip it below.
        out[count] = &sentence[start..];
        count += 1;
    }
    count
}

/// Drop the `*hh` suffix from the final field.
fn number(field: &str) -> Option<f32> {
    let value = field.split('*').next().unwrap_or("");
    if value.is_empty() {
        None
    } else {
        value.parse().ok()
    }
}

/// `ddmm.mmmm` / `dddmm.mmmm` plus a hemisphere letter -> signed degrees.
fn degrees(value: &str, hemisphere: &str, degree_digits: usize) -> Option<f64> {
    if value.len() <= degree_digits {
        return None;
    }
    let whole: f64 = value[..degree_digits].parse().ok()?;
    let minutes: f64 = value[degree_digits..].parse().ok()?;
    let signed = whole + minutes / 60.0;
    Some(if hemisphere == "S" || hemisphere == "W" {
        -signed
    } else {
        signed
    })
}

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("gps-nmea: GNSS on UART1 at 9600 baud");
}

pub fn forever(ctx: &mut Context) {
    let uart = device!(ctx, uart1);
    let mut line = [0u8; LINE_BYTES];

    let mut position: Option<(f64, f64)> = None;
    let mut altitude: Option<f32> = None;
    let mut satellites: Option<f32> = None;

    // One second of sentences is one full update.
    for _ in 0..SENTENCES_PER_FIX {
        let length = match uart.read_line(&mut line) {
            Ok(0) => continue,
            Ok(length) => length,
            Err(_) => continue,
        };
        let sentence = &line[..length];
        if !checksum_ok(sentence) {
            ctx.logger.warn("gps-nmea: checksum mismatch");
            continue;
        }

        let Ok(text) = core::str::from_utf8(sentence) else {
            continue;
        };
        let text = text.trim_end();
        let mut fields = [""; FIELDS];
        let count = split_fields(text, &mut fields);
        if count < 7 {
            continue;
        }

        if fields[0].ends_with("RMC") && fields[2] == "A" {
            // Fields 3/4 are latitude and its hemisphere, 5/6 longitude.
            match (
                degrees(fields[3], fields[4], 2),
                degrees(fields[5], fields[6], 3),
            ) {
                (Some(lat), Some(lon)) => position = Some((lat, lon)),
                _ => {}
            }
        } else if fields[0].ends_with("GGA") && count >= 9 {
            // Field 7 is the satellite count, field 9 the altitude in metres.
            satellites = number(fields[7]);
            altitude = number(fields[9]);
        }
    }

    match position {
        Some((lat, lon)) => {
            ctx.logger
                .info(format_args!("Node Latitude: {lat:.6}"));
            ctx.logger
                .info(format_args!("Node Longitude: {lon:.6}"));
        }
        None => ctx.logger.warn("gps-nmea: no fix in the last second"),
    }
    if let Some(altitude) = altitude {
        ctx.logger
            .info(format_args!("Node Altitude: {altitude:.1} m"));
    }
    if let Some(satellites) = satellites {
        ctx.logger
            .info(format_args!("Visible Satellites: {satellites:.0}"));
    }
}
