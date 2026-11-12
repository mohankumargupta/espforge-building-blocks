use crate::Context;

/// `update_interval: 5s`
const UPDATE_INTERVAL_MS: u32 = 5_000;
/// MAX6675 bit 2 of the second byte: the thermocouple input is open.
const OPEN_THERMOCOUPLE: u8 = 0x04;

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("spi-sensor: MAX6675 on SPI2, CS GPIO10, 4000 kHz");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let spi = device!(ctx, spi2);

    // The part is read-only: two dummy bytes clock out the 16-bit result.
    let mut raw = [0u8; 2];
    match spi.write_read(&[0x00, 0x00], &mut raw) {
        Ok(()) if raw[1] & OPEN_THERMOCOUPLE != 0 => {
            ctx.logger
                .warn("Furnace Thermocouple: open circuit");
        }
        Ok(()) => {
            let counts = (u16::from(raw[0]) << 8 | u16::from(raw[1])) >> 3;
            let celsius = f32::from(counts) * 0.25;
            ctx.logger
                .info(format_args!("Furnace Thermocouple: {celsius:.2} C"));
        }
        Err(_) => ctx.logger.warn("spi-sensor: transfer failed"),
    }

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
