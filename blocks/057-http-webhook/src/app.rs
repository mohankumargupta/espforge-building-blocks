use crate::{service, Context};
use core::sync::atomic::{AtomicBool, Ordering};
use embassy_executor::Spawner;

/// `url:`
const ENDPOINT: &str = "http://192.168.1.50:8000/alert";
/// `json:` — written by hand instead of pulling in a serialiser.
const BODY: &[u8] = br#"{"event":"button_pressed","node":"office_node"}"#;
const CONTENT_TYPE: &str = "application/json";
/// How often the button is sampled.
const SAMPLE_MS: u32 = 20;
/// The reply is ignored, but the connection needs somewhere to put it.
const RESPONSE_BYTES: usize = 256;

/// Edge detection: one POST per press, not one per sample.
static ARMED: AtomicBool = AtomicBool::new(true);

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let logger = ctx.logger;
    logger.info("http-webhook: connecting...");
    service!(ctx, home).wait_connected().await;
    logger.info("http-webhook: panic button armed");
}

pub async fn forever(ctx: &'static Context) {
    let pressed = device!(ctx, panic_button).is_pressed();
    let armed = ARMED.load(Ordering::Relaxed);

    if pressed && armed {
        ARMED.store(false, Ordering::Relaxed);

        let mut response = [0u8; RESPONSE_BYTES];
        let client = service!(ctx, webhook);
        let result = client
            .post(ENDPOINT, CONTENT_TYPE, BODY, &mut response)
            .await;

        match result {
            Ok(body) => ctx.logger.info(format_args!(
                "webhook: delivered, {} bytes of reply",
                body.len()
            )),
            Err(error) => ctx.logger.warn(format_args!("webhook: {error}")),
        }
    } else if !pressed {
        ARMED.store(true, Ordering::Relaxed);
    }

    ctx.delay.delay_ms(SAMPLE_MS).await;
}
