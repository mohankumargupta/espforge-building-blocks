#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::Level,
    main,
    rmt::{PulseCode, Rmt, TxChannel, TxChannelConfig, TxChannelCreator},
    time::Rate,
};

// 80 MHz / 80 = 1 MHz (1 tick = 1 µs)
// 115200 baud ≈ 8.68 µs per bit -> 9 ticks
const BIT_TICKS: u16 = 9;

fn byte_to_pulses(byte: u8) -> [PulseCode; 5] {
    let bit = |i: u8| {
        if (byte >> i) & 1 == 1 {
            Level::High
        } else {
            Level::Low
        }
    };

    [
        PulseCode::new(Level::Low, BIT_TICKS, bit(0), BIT_TICKS), // Start + D0
        PulseCode::new(bit(1), BIT_TICKS, bit(2), BIT_TICKS),     // D1 + D2
        PulseCode::new(bit(3), BIT_TICKS, bit(4), BIT_TICKS),     // D3 + D4
        PulseCode::new(bit(5), BIT_TICKS, bit(6), BIT_TICKS),     // D5 + D6
        PulseCode::new(bit(7), BIT_TICKS, Level::High, BIT_TICKS), // D7 + Stop
    ]
}

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let delay = Delay::new();

    // Initialize RMT at 80 MHz (use 40 MHz on ESP32-H2)
    let rmt = Rmt::new(peripherals.RMT, Rate::from_mhz(80)).unwrap();

    // UART idles HIGH
    let tx_config = TxChannelConfig::default()
        .with_clk_divider(80)
        .with_idle_output_level(Level::High)
        .with_idle_output(true);

    // Output on GPIO4 (change to your target TX pin)
    let mut channel = rmt.channel0.configure_tx(peripherals.GPIO4, tx_config).unwrap();

    // 6 bytes ("hello\n") * 5 PulseCodes + 1 end marker = 31 symbols
    let msg = b"hello\n";
    let mut data = [PulseCode::end_marker(); 31];

    for (i, &b) in msg.iter().enumerate() {
        let codes = byte_to_pulses(b);
        data[i * 5..(i + 1) * 5].copy_from_slice(&codes);
    }
    // Final element remains PulseCode::end_marker() (length1 = 0, length2 = 0)

    loop {
        let transaction = channel.transmit(&data).unwrap();
        channel = transaction.wait().unwrap();
        delay.delay_millis(1000);
    }
}
