use crate::Context;

/// `uuid:` with its dashes removed, as 16 bytes.
const UUID: [u8; 16] = [
    0xE2, 0xC5, 0x6D, 0xB5, 0xDF, 0xFB, 0x48, 0xD2, 0xB0, 0x60, 0xD0, 0xF5, 0xA7, 0x10, 0x96,
    0xE0,
];
/// `major: 100` and `minor: 1`, big-endian in the payload.
const MAJOR: u16 = 100;
const MINOR: u16 = 1;
/// `tx_power: -59` is the calibrated RSSI at one metre, as a signed byte.
const TX_POWER: i8 = -59;
/// iBeacons advertise at 100 ms; slower is harder to track at range.
const ADVERTISING_MS: u32 = 100;

/// The fixed iBeacon payload: Apple ID, type, length, UUID, major, minor, RSSI.
fn payload() -> [u8; 25] {
    let mut data = [0u8; 25];
    data[0] = 0x4C; // Apple, little-endian company ID
    data[1] = 0x00;
    data[2] = 0x02; // beacon type
    data[3] = 0x15; // remaining length: 21 bytes
    data[4..20].copy_from_slice(&UUID);
    data[20..22].copy_from_slice(&MAJOR.to_be_bytes());
    data[22..24].copy_from_slice(&MINOR.to_be_bytes());
    data[24] = TX_POWER as u8;
    data
}

/// The intended shape: `<ble_beacon>` re-advertises the payload on a timer
/// without asking the app. It is a proposal.
pub fn setup(ctx: &mut Context) {
    let data = payload();
    ctx.logger.info(format_args!(
        "ble-ibeacon: {} byte payload, major {MAJOR}, minor {MINOR} (proposed)",
        data.len()
    ));
}

pub fn forever(ctx: &mut Context) {
    let beacon = service!(ctx, beacon);

    // A broadcaster never connects: a non-connectable advertisement is enough.
    match beacon.advertise(&payload()) {
        Ok(()) => ctx.logger.info("beacon: advertising"),
        Err(_) => ctx.logger.warn("ble-ibeacon: radio refused the payload"),
    }

    ctx.delay.delay_ms(ADVERTISING_MS);
}
