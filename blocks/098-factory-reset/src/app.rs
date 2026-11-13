use crate::Context;

/// `min_length: 5s` — the soft restart band starts here.
const RESTART_MS: u32 = 5_000;
/// `min_length: 15s` — the factory-wipe band starts here.
const WIPE_MS: u32 = 15_000;
/// Sampling period for the hold timer.
const SAMPLE_MS: u32 = 20;
/// The LED changes cadence at each threshold, so the band is visible.
const FAST_BLINK_MS: u32 = 100;
const SLOW_BLINK_MS: u32 = 400;

pub fn setup(ctx: &mut Context) {
    device!(ctx, status_led).off();
    ctx.logger
        .info("factory-reset: 5 s restart, 15 s wipe and restart");
}

pub fn forever(ctx: &mut Context) {
    static mut HELD_MS: u32 = 0;
    static mut BLINK_MS: u32 = 0;
    static mut LED_ON: bool = false;

    // SAFETY: only `forever` touches these, and it is not re-entered.
    let (held, blink_left, led_on) = unsafe {
        (
            &mut *core::ptr::addr_of_mut!(HELD_MS),
            &mut *core::ptr::addr_of_mut!(BLINK_MS),
            &mut *core::ptr::addr_of_mut!(LED_ON),
        )
    };

    if !device!(ctx, recovery_button).is_pressed() {
        // Released before a threshold: nothing happens, LED off.
        *held = 0;
        device!(ctx, status_led).off();
        *led_on = false;
        ctx.delay.delay_ms(SAMPLE_MS);
        return;
    }

    *held += SAMPLE_MS;

    // The LED cadence is the only feedback while the button is held.
    *blink_left = blink_left.saturating_sub(SAMPLE_MS);
    if *blink_left == 0 {
        *blink_left = if *held >= WIPE_MS {
            SLOW_BLINK_MS
        } else if *held >= RESTART_MS {
            FAST_BLINK_MS
        } else {
            SAMPLE_MS
        };
        *led_on = !*led_on;
        let led = device!(ctx, status_led);
        if *led_on {
            led.on();
        } else {
            led.off();
        }
    }

    if *held >= WIPE_MS {
        // The intended action. There is no NVS namespace to erase yet: block
        // 019 owns credential persistence, and this block owns the wipe policy.
        ctx.logger.warn(
            "Factory reset triggered: wiping NVS flash (no NVS API: nothing to erase)",
        );
        device!(ctx, status_led).on();
        ctx.ota.reboot();
    } else if *held >= RESTART_MS && *held - SAMPLE_MS < RESTART_MS {
        ctx.logger.info("Soft reboot initiated");
        ctx.ota.reboot();
    }

    ctx.delay.delay_ms(SAMPLE_MS);
}
