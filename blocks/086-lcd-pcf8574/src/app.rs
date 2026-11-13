use crate::Context;

/// `address: 0x27`
const ADDRESS: u8 = 0x27;
/// `dimensions: 20x4`
const COLUMNS: u8 = 20;
const ROWS: u8 = 4;
/// Backpack bit positions: D4..D7 in the low nibble, RS, EN, backlight.
const EN: u8 = 0x04;
const RS: u8 = 0x01;
const BACKLIGHT: u8 = 0x08;
/// The LCD needs ~40 us after a nibble and ~1.5 ms after a command.
const NIBBLE_US: u32 = 50;
const COMMAND_MS: u32 = 2;
/// `update_interval:` — the panel is slow, so the value is refreshed lazily.
const UPDATE_INTERVAL_MS: u32 = 5_000;
/// DDRAM addresses of the four rows on a 20x4 panel.
const ROW_ADDRESS: [u8; 4] = [0x00, 0x40, 0x14, 0x54];

/// Write one nibble: data on D4..D7, then an enable pulse.
fn write_nibble(ctx: &mut Context, nibble: u8, rs: bool) {
    let bus = device!(ctx, i2c0);
    let base = BACKLIGHT | if rs { RS } else { 0 } | (nibble << 4);

    bus.write(ADDRESS, &[base | EN]).ok();
    ctx.delay.delay_ns(NIBBLE_US * 1_000);
    bus.write(ADDRESS, &[base]).ok();
    ctx.delay.delay_ns(NIBBLE_US * 1_000);
}

/// One byte, high nibble first, then the low nibble.
fn write_byte(ctx: &mut Context, byte: u8, rs: bool) {
    write_nibble(ctx, byte >> 4, rs);
    write_nibble(ctx, byte & 0x0F, rs);
}

fn command(ctx: &mut Context, byte: u8) {
    write_byte(ctx, byte, false);
    ctx.delay.delay_ms(COMMAND_MS);
}

/// `it.print(row, text)`: set the DDRAM address, then write characters.
fn print_row(ctx: &mut Context, row: usize, text: &str) {
    command(ctx, 0x80 | ROW_ADDRESS[row % 4]);
    for byte in text.bytes().take(COLUMNS as usize) {
        write_byte(ctx, byte, true);
    }
    for _ in text.len()..COLUMNS as usize {
        write_byte(ctx, b' ', true);
    }
}

pub fn setup(ctx: &mut Context) {
    ctx.delay.delay_ms(50);

    // The 4-bit initialisation dance from the HD44780 datasheet.
    write_nibble(ctx, 0x03, false);
    ctx.delay.delay_ms(5);
    write_nibble(ctx, 0x03, false);
    ctx.delay.delay_ms(1);
    write_nibble(ctx, 0x03, false);
    write_nibble(ctx, 0x02, false);
    command(ctx, 0x28); // 4-bit, 2 lines, 5x8 font
    command(ctx, 0x0C); // display on, cursor off
    command(ctx, 0x01); // clear
    command(ctx, 0x06); // increment on write

    print_row(ctx, 0, "SYSTEM ONLINE");
    ctx.logger.info("lcd-pcf8574: 20x4 panel initialised at 0x27");

    if device!(ctx, air).start().is_err() {
        ctx.logger.warn("lcd-pcf8574: sensor did not start");
    }
}

pub fn forever(ctx: &mut Context) {
    static mut BUFFER: [u8; 20] = [b' '; 20];

    let reading = match device!(ctx, air).read() {
        Ok(reading) => f32::from(reading.temperature_centi_c) / 100.0,
        Err(_) => {
            ctx.logger.warn("lcd-pcf8574: sensor read failed");
            ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
            return;
        }
    };

    // SAFETY: only `forever` touches the buffer, and it is not re-entered.
    let buffer = unsafe { &mut *core::ptr::addr_of_mut!(BUFFER) };
    let mut length = 0;
    for byte in b"Temp: " {
        buffer[length] = *byte;
        length += 1;
    }
    // One decimal place, without core::fmt in the hot path.
    let scaled = (reading * 10.0).round() as i32;
    let whole = scaled / 10;
    if whole >= 10 {
        buffer[length] = b'0' + (whole / 10) as u8;
        length += 1;
    }
    buffer[length] = b'0' + (whole % 10) as u8;
    length += 1;
    buffer[length] = b'.';
    length += 1;
    buffer[length] = b'0' + (scaled % 10).unsigned_abs() as u8;
    length += 1;
    buffer[length] = b' ';
    length += 1;
    buffer[length] = b'C';
    length += 1;

    let text = core::str::from_utf8(&buffer[..length]).unwrap_or("Temp: ?");
    print_row(ctx, 2, text);
    ctx.logger.info(format_args!("lcd: {text}"));

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
