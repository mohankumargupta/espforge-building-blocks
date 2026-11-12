use crate::Context;

/// `update_interval: 10s`: the counting window.
const WINDOW_MS: u32 = 10_000;
/// The polling period bounds the shortest pulse the counter can see.
const SAMPLE_MS: u32 = 1;
/// Edges closer together than this are contact bounce, not flow.
const GLITCH_MS: u32 = 2;
/// YF-S201: 450 pulses per litre.
const PULSES_PER_LITRE: f32 = 450.0;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("pulse-counter: water flow on GPIO4");
}

pub fn forever(ctx: &mut Context) {
    let delay = ctx.delay;
    let sensor = device!(ctx, flow_sensor);

    let mut pulses = 0u32;
    let mut elapsed_ms = 0;
    let mut since_edge_ms = GLITCH_MS;
    let mut was_pressed = sensor.is_pressed();

    while elapsed_ms < WINDOW_MS {
        let pressed = sensor.is_pressed();
        since_edge_ms = since_edge_ms.saturating_add(SAMPLE_MS);

        if pressed && !was_pressed && since_edge_ms >= GLITCH_MS {
            pulses += 1;
            since_edge_ms = 0;
        }

        was_pressed = pressed;
        delay.delay_ms(SAMPLE_MS);
        elapsed_ms += SAMPLE_MS;
    }

    let hz = pulses as f32 / (WINDOW_MS as f32 / 1000.0);
    let litres_per_minute = hz * 60.0 / PULSES_PER_LITRE;
    ctx.logger.info(format_args!(
        "Water Flow Rate: {:.2} L/min ({} pulses/s)",
        litres_per_minute, hz
    ));
}
