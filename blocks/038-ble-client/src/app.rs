use crate::Context;

/// `service_uuid: "181A"` (Environmental Sensing) and `characteristic_uuid`.
const SERVICE_UUID: u16 = 0x181A;
const CHARACTERISTIC_UUID: u16 = 0x2A6E;
/// `update_interval: 60s`
const UPDATE_INTERVAL_MS: u32 = 60_000;

/// The intended shape: `<ble_client>` connects, discovers the service and
/// returns the characteristic's little-endian value. All of it is proposed.
pub fn setup(ctx: &mut Context) {
    ctx.logger
        .info("ble-client: soil node DC:23:4F:91:0A:BC (proposed)");
}

pub fn forever(ctx: &mut Context) {
    let node = device!(ctx, soil_node);

    match node.read_characteristic(SERVICE_UUID, CHARACTERISTIC_UUID) {
        Ok(raw) => ctx.logger.info(format_args!(
            "Remote Soil Moisture: {:.1} %",
            f32::from(raw) / 100.0
        )),
        Err(_) => ctx.logger.warn("ble-client: read failed"),
    }

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
