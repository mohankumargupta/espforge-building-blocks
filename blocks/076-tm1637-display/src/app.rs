use crate::Context;

/// `intensity: 4` — brightness 0..7, bit 3 always set in the command.
const INTENSITY: u8 = 4;
/// Display-control command: on, with the brightness above.
const COMMAND_DISPLAY: u8 = 0x88 | INTENSITY;
/// Address auto-increment, then six data bytes from 0xC0.
const COMMAND_WRITE: u8 = 0x40;
const ADDRESS: u8 = 0xC0;
/// Each protocol edge is short; 5 us is far inside the part's 250 kHz limit.
const EDGE_NS: u32 = 5_000;
/// The colon blinks once a second.
const BLINK_MS: u32 = 500;

/// Seven-segment bit order: a b c d e f g, with the decimal point in bit 7.
const SEGMENTS: [u8; 10] = [
    0x3F, 0x06, 0x5B, 0x4F, 0x66, 0x6D, 0x7D, 0x07, 0x7F, 0x6F,
];

fn start(ctx: &mut Context) {
    let delay = ctx.delay;
    let clk = device!(ctx, tm_clk);
    let dio = device!(ctx, tm_dio);
    // Both high, then DIO falls while CLK is high.
    clk.on();
    dio.on();
    delay.delay_ns(EDGE_NS);
    dio.off();
    delay.delay_ns(EDGE_NS);
    clk.off();
}

fn stop(ctx: &mut Context) {
    let delay = ctx.delay;
    let clk = device!(ctx, tm_clk);
    let dio = device!(ctx, tm_dio);
    // CLK low, DIO low, then DIO rises while CLK is high.
    clk.off();
    dio.off();
    delay.delay_ns(EDGE_NS);
    clk.on();
    delay.delay_ns(EDGE_NS);
    dio.on();
    delay.delay_ns(EDGE_NS);
}

/// One byte, least significant bit first, clocked on the falling edge.
fn write_byte(ctx: &mut Context, byte: u8) {
    let delay = ctx.delay;
    let clk = device!(ctx, tm_clk);
    let dio = device!(ctx, tm_dio);

    for bit in 0..8 {
        clk.off();
        if byte & (1 << bit) != 0 {
            dio.on();
        } else {
            dio.off();
        }
        delay.delay_ns(EDGE_NS);
        clk.on();
        delay.delay_ns(EDGE_NS);
    }

    // Acknowledge: CLK low, DIO released, ninth clock, then CLK low again.
    clk.off();
    dio.on();
    delay.delay_ns(EDGE_NS);
    clk.on();
    delay.delay_ns(EDGE_NS);
    clk.off();
}

fn colour_segments(digits: &[u8; 4], colon: bool) -> [u8; 4] {
    let mut out = [0u8; 4];
    for (index, digit) in digits.iter().enumerate() {
        out[index] = SEGMENTS[(*digit as usize).min(9)];
    }
    // The colon is the second digit's decimal point.
    if colon {
        out[1] |= 0x80;
    }
    out
}

pub fn setup(ctx: &mut Context) {
    let digits = [1u8, 2, 3, 4];
    let segments = colour_segments(&digits, true);

    // Display control, then address write mode, then the four digits.
    start(ctx);
    write_byte(ctx, COMMAND_DISPLAY);
    stop(ctx);

    start(ctx);
    write_byte(ctx, COMMAND_WRITE);
    stop(ctx);

    start(ctx);
    write_byte(ctx, ADDRESS);
    for segment in segments {
        write_byte(ctx, segment);
    }
    stop(ctx);

    ctx.logger
        .info(format_args!("tm1637: {}{}:{}{} at intensity {INTENSITY}", digits[0], digits[1], digits[2], digits[3]));
}

pub fn forever(ctx: &mut Context) {
    static mut SECOND: u8 = 0;
    static mut COLON: bool = true;

    // SAFETY: only `forever` touches these, and it is not re-entered.
    let (second, colon) = unsafe { (&mut *core::ptr::addr_of_mut!(SECOND), &mut *core::ptr::addr_of_mut!(COLON)) };

    *colon = !*colon;
    // Count 00..59 and carry into the minutes, exactly like a clock.
    if *colon {
        *second = (*second + 1) % 60;
    }
    let minutes = u32::from(*second) / 60;
    let digits = [
        (minutes / 10) as u8,
        (minutes % 10) as u8,
        (*second / 10) as u8,
        (*second % 10) as u8,
    ];

    start(ctx);
    write_byte(ctx, ADDRESS);
    for segment in colour_segments(&digits, *colon) {
        write_byte(ctx, segment);
    }
    stop(ctx);

    ctx.delay.delay_ms(BLINK_MS);
}
