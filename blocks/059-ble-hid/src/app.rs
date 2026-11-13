use crate::Context;

/// Consumer-page usage for media mute, from the HID usage tables.
const MEDIA_MUTE: u16 = 0x00E2;
/// A HID report is the usage low byte after a zero modifier byte.
const REPORT_LEN: usize = 2;
const POLL_MS: u32 = 20;

/// The intended shape: `<ble_hid>` holds the GATT server and the report
/// descriptor, and `send` notifies the host. It is a proposal.
pub fn setup(ctx: &mut Context) {
    ctx.logger.info(
        "ble-hid: Living Room Remote, consumer report 0x00E2 for mute (proposed)",
    );
}

pub fn forever(ctx: &mut Context) {
    let remote = service!(ctx, remote);
    let button = device!(ctx, mute_button);

    if button.is_pressed() {
        // Press then release: a host that only sees the press latches mute.
        let report = [0u8, MEDIA_MUTE as u8];
        debug_assert_eq!(report.len(), REPORT_LEN);

        if remote.send(&report).is_ok() {
            ctx.logger.info("ble-hid: media mute sent");
            remote.send(&[0u8, 0]).ok();
        } else {
            ctx.logger.warn("ble-hid: not connected");
        }
    }

    ctx.delay.delay_ms(POLL_MS);
}
