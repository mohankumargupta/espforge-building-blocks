use crate::Context;

/// `address: 0x68`
const ADDRESS: u8 = 0x68;
/// Seconds, minutes, hours, weekday, day, month, year: seven BCD bytes.
const REGISTER_TIME: u8 = 0x00;
const REGISTER_TEMPERATURE: u8 = 0x11;
/// One report a minute is enough to prove the clock is running.
const REPORT_MS: u32 = 60_000;

fn from_bcd(value: u8) -> u8 {
    (value >> 4) * 10 + (value & 0x0F)
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("ds3231-rtc: reading 20xx date and die temperature at 0x68");
}

pub fn forever(ctx: &mut Context) {
    let bus = device!(ctx, i2c0);

    let mut registers = [0u8; 7];
    if bus
        .write_read(ADDRESS, &[REGISTER_TIME], &mut registers)
        .is_err()
    {
        ctx.logger.warn("ds3231-rtc: no ACK at 0x68 (coin cell missing?)");
        ctx.delay.delay_ms(REPORT_MS);
        return;
    }

    let second = from_bcd(registers[0] & 0x7F);
    let minute = from_bcd(registers[1] & 0x7F);
    // Bit 6 selects 12-hour mode; clear it and bits 4..5 to get 24-hour hours.
    let hour = if registers[2] & 0x40 != 0 {
        from_bcd(registers[2] & 0x1F)
    } else {
        from_bcd(registers[2] & 0x3F)
    };
    let day = from_bcd(registers[4] & 0x3F);
    let month = from_bcd(registers[5] & 0x1F);
    // The century bit lives in bit 7 of the month register.
    let year = 2_000 + u16::from(from_bcd(registers[6]));

    ctx.logger.info(format_args!(
        "RTC: {year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}"
    ));

    // Temperature is a signed byte with a quarter-degree resolution.
    let mut raw = [0u8; 1];
    if bus
        .write_read(ADDRESS, &[REGISTER_TEMPERATURE], &mut raw)
        .is_ok()
    {
        let celsius = (raw[0] as i8) as f32;
        ctx.logger
            .info(format_args!("RTC Crystal Temperature: {celsius:.2} C"));
    }

    ctx.delay.delay_ms(REPORT_MS);
}
