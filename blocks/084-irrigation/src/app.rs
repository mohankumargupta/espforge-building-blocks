use crate::Context;

/// `valves: run_duration:` in minutes, one entry per zone.
const RUN_MINUTES: [u32; 3] = [15, 20, 10];
/// Tick period for the countdown; the valves themselves are slow.
const TICK_MS: u32 = 1_000;
/// `main_switch`: the master valve is held open only while a zone is open.
const ZONE_COUNT: usize = RUN_MINUTES.len();

fn drive_zone(ctx: &mut Context, index: usize) {
    match index {
        0 => device!(ctx, zone1_valve).on(),
        1 => device!(ctx, zone2_valve).on(),
        _ => device!(ctx, zone3_valve).on(),
    }
}

fn release_all(ctx: &mut Context) {
    device!(ctx, zone1_valve).off();
    device!(ctx, zone2_valve).off();
    device!(ctx, zone3_valve).off();
    device!(ctx, master_valve).off();
}

pub fn setup(ctx: &mut Context) {
    release_all(ctx);
    ctx.logger.info(format_args!(
        "irrigation: {} zones, {} min total, rain lockout on GPIO4",
        ZONE_COUNT,
        RUN_MINUTES.iter().sum::<u32>()
    ));
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;

    // A wet rain sensor shorts to ground: the whole cycle is skipped.
    if device!(ctx, rain_sensor).is_pressed() {
        ctx.logger.info("irrigation: rain sensor wet, cycle skipped");
        delay.delay_ms(60_000);
        return;
    }

    device!(ctx, master_valve).on();

    for (zone, minutes) in RUN_MINUTES.iter().enumerate() {
        // Interlock: close every other zone before opening this one.
        release_all(ctx);
        device!(ctx, master_valve).on();
        drive_zone(ctx, zone);

        let total_seconds = minutes * 60;
        let mut elapsed = 0;
        while elapsed < total_seconds {
            if device!(ctx, rain_sensor).is_pressed() {
                ctx.logger.info("irrigation: rain started, aborting cycle");
                release_all(ctx);
                delay.delay_ms(60_000);
                return;
            }
            delay.delay_ms(TICK_MS);
            elapsed += TICK_MS / 1_000;
        }

        ctx.logger.info(format_args!(
            "irrigation: zone {} finished after {minutes} min",
            zone + 1
        ));
    }

    release_all(ctx);
    ctx.logger.info("irrigation: cycle complete, valves closed");
    delay.delay_ms(60_000);
}
