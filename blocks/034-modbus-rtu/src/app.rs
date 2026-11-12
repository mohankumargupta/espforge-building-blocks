use crate::Context;

/// `address: 0x01` — the power meter on the bus.
const SLAVE: u8 = 0x01;
/// `register_type: holding`, `register_address: 0x0012`, two registers (FP32).
const REGISTER: u16 = 0x0012;
const REGISTER_COUNT: u16 = 2;
/// slave, function, byte count, 4 data bytes, CRC16.
const RESPONSE_BYTES: usize = 9;
const READ_TIMEOUT_MS: u32 = 100;
const POLL_MS: u32 = 1;
/// `update_interval`
const UPDATE_INTERVAL_MS: u32 = 10_000;

/// CRC16-MODBUS: reflected polynomial 0xA001, initial value 0xFFFF.
fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xFFFFu16;
    for byte in data {
        crc ^= u16::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xA001
            } else {
                crc >> 1
            };
        }
    }
    crc
}

pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("modbus-rtu: meter 0x01 on UART1, 9600 8N1, DE GPIO10");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let uart = device!(ctx, uart1);
    let driver_enable = device!(ctx, rs485_de);

    // Read-holding-registers request: 01 03 00 12 00 02 CRC.
    let mut request = [0u8; 8];
    request[0] = SLAVE;
    request[1] = 0x03;
    request[2..4].copy_from_slice(&REGISTER.to_be_bytes());
    request[4..6].copy_from_slice(&REGISTER_COUNT.to_be_bytes());
    let crc = crc16(&request[..6]);
    request[6] = crc as u8;
    request[7] = (crc >> 8) as u8;

    // Assert DE for the transmit, release it before listening.
    driver_enable.on();
    let sent = uart.write_all(&request);
    driver_enable.off();

    if sent.is_err() {
        ctx.logger.warn("modbus-rtu: request write failed");
        delay.delay_ms(UPDATE_INTERVAL_MS);
        return;
    }

    let mut response = [0u8; RESPONSE_BYTES];
    let mut filled = 0;
    let mut waited_ms = 0;
    while filled < response.len() && waited_ms < READ_TIMEOUT_MS {
        match uart.read(&mut response[filled..]) {
            Ok(0) => {
                delay.delay_ms(POLL_MS);
                waited_ms += POLL_MS;
            }
            Ok(n) => filled += n,
            Err(_) => break,
        }
    }

    if filled < response.len() {
        ctx.logger.warn("modbus-rtu: response timed out");
    } else if response[0] != SLAVE || response[1] != 0x03 || response[2] != 4 {
        ctx.logger.warn("modbus-rtu: unexpected slave, function or length");
    } else {
        let expected = crc16(&response[..7]);
        if response[7] != expected as u8 || response[8] != (expected >> 8) as u8 {
            ctx.logger.warn("modbus-rtu: CRC16 mismatch");
        } else {
            let power = f32::from_be_bytes([
                response[3],
                response[4],
                response[5],
                response[6],
            ]);
            ctx.logger
                .info(format_args!("Active Power: {power:.1} W"));
        }
    }

    delay.delay_ms(UPDATE_INTERVAL_MS);
}
