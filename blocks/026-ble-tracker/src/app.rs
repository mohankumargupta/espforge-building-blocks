use crate::Context;

/// How often the presence state is reported.
const REPORT_INTERVAL_MS: u32 = 1_000;

/// The intended shape: `<ble_tracker>` owns scanning and `<ble_presence>`
/// turns advertisements from one MAC into a boolean and an RSSI. Neither
/// element exists yet, so both methods are proposals.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("ble-tracker: scanning for AA:BB:CC:DD:EE:FF (proposed)");
}

pub fn forever(ctx: &mut Context) {
    let beacon = device!(ctx, key_beacon);

    if beacon.is_present() {
        ctx.logger.info(format_args!(
            "Key Beacon Presence: detected at {} dBm",
            beacon.rssi_dbm()
        ));
    } else {
        ctx.logger.info("Key Beacon Presence: not detected");
    }

    ctx.delay.delay_ms(REPORT_INTERVAL_MS);
}
