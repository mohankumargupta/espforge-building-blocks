use crate::Context;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use embassy_executor::Spawner;

/// One blink tick. The cadence is built from these.
const TICK_MS: u32 = 100;
/// Connecting: on/off every tick (5 Hz).
const CONNECTING_HALF_TICKS: u8 = 1;
/// Associated but no address yet: two quick blinks, then a pause.
const PAUSED_HALF_TICKS: u8 = 3;
/// Offline after having been up: a slow heartbeat, 1 Hz.
const OFFLINE_HALF_TICKS: u8 = 5;

const ONLINE: u8 = 0;
const CONNECTING: u8 = 1;
const PAUSED: u8 = 2;
const OFFLINE: u8 = 3;

/// The current life-cycle state, updated by `setup` and `forever`.
static STATE: AtomicU8 = AtomicU8::new(CONNECTING);
static LED_ON: AtomicBool = AtomicBool::new(false);
static HALF_TICKS: AtomicU8 = AtomicU8::new(0);

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger
        .info("status-led: fast blink while connecting, steady when online");
    STATE.store(CONNECTING, Ordering::Relaxed);
}

pub async fn forever(ctx: &'static Context) {
    // `config_v4()` is Some only once DHCP has answered: that is "online".
    let state = if service!(ctx, home).stack().config_v4().is_some() {
        ONLINE
    } else {
        OFFLINE
    };
    // The station has no "associated but no address" signal yet; keep the
    // cadence for it anyway so a future state can use it.
    let _ = PAUSED;
    STATE.store(state, Ordering::Relaxed);

    let half_ticks = match state {
        CONNECTING => CONNECTING_HALF_TICKS,
        PAUSED => PAUSED_HALF_TICKS,
        OFFLINE => OFFLINE_HALF_TICKS,
        _ => 0,
    };

    let led = device!(ctx, status_led);
    if half_ticks == 0 {
        // Online: solid.
        led.on();
        LED_ON.store(true, Ordering::Relaxed);
    } else {
        let ticks = HALF_TICKS.load(Ordering::Relaxed);
        if ticks == 0 {
            let on = !LED_ON.load(Ordering::Relaxed);
            LED_ON.store(on, Ordering::Relaxed);
            if on {
                led.on();
            } else {
                led.off();
            }
        }
        HALF_TICKS.store((ticks + 1) % half_ticks, Ordering::Relaxed);
    }

    ctx.delay.delay_ms(TICK_MS).await;
}
