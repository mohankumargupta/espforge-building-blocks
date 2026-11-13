use crate::Context;

/// `update_interval: 500ms`
const UPDATE_INTERVAL_MS: u32 = 500;
/// XSHUT must be low while the bus settles, then released to boot the part.
const SHUTDOWN_MS: u32 = 10;
/// A long-range profile reaches ~2 m; the tank is 1.2 m deep.
const TANK_DEPTH_MM: u16 = 1_200;

/// The intended shape: `<vl53l0x>` holds the tuning table, the calibration and
/// the ranging state machine, and returns millimetres. It is a proposal.
pub fn setup(ctx: &mut Context) {
    let xshut = device!(ctx, tof_xshut);

    // The driver would release reset and then run its tuning sequence.
    xshut.off();
    ctx.delay.delay_ms(SHUTDOWN_MS);
    xshut.on();

    match device!(ctx, tank_level).init() {
        Ok(()) => ctx.logger.info(
            "vl53l0x: Water Tank Headspace Distance, long-range profile (proposed)",
        ),
        Err(_) => ctx.logger.warn("vl53l0x: init sequence failed"),
    }
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, tank_level).range_mm() {
        Ok(distance) => {
            let headspace = distance.min(TANK_DEPTH_MM);
            let filled_percent =
                100 - (u32::from(headspace) * 100 / u32::from(TANK_DEPTH_MM)) as u8;
            ctx.logger.info(format_args!(
                "Water Tank: {distance} mm to water, ~{filled_percent}% full"
            ));
        }
        Err(_) => ctx.logger.warn("vl53l0x: ranging timed out"),
    }

    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
