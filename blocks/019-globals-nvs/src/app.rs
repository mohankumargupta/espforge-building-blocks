use crate::Context;
use core::sync::atomic::{AtomicU32, Ordering};

/// The NVS key the count is stored under. `<nvs>` is a proposal, so
/// `load_u32`/`store_u32` do not exist yet.
const NVS_KEY: &str = "total_cycles";

/// `globals: id: total_cycles, type: int, initial_value: '0'`
static TOTAL_CYCLES: AtomicU32 = AtomicU32::new(0);

pub fn setup(ctx: &mut Context) {
    match service!(ctx, flash).load_u32(NVS_KEY) {
        Ok(value) => {
            TOTAL_CYCLES.store(value, Ordering::Relaxed);
            ctx.logger
                .info(format_args!("globals-nvs: restored {}", value));
        }
        Err(_) => ctx.logger.info("globals-nvs: no stored value, starting at 0"),
    }
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let button = device!(ctx, cycle_button);

    if button.is_pressed() {
        delay.delay_ms(20);
        if button.is_pressed() {
            let cycles = TOTAL_CYCLES.fetch_add(1, Ordering::Relaxed) + 1;
            ctx.logger.info(format_args!("total_cycles = {}", cycles));

            // `restore_value: yes`: the new count must reach flash before the
            // next reset, or the increment is lost.
            if service!(ctx, flash).store_u32(NVS_KEY, cycles).is_err() {
                ctx.logger.warn("globals-nvs: persist failed");
            }

            // One event per press, like block 009.
            while button.is_pressed() {
                delay.delay_ms(10);
            }
        }
    }

    delay.delay_ms(10);
}
