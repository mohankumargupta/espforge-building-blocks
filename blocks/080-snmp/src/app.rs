use crate::{service, Context};
use embassy_executor::Spawner;

/// `community: "public"` — SNMPv2c, which is plaintext by design.
const COMMUNITY: &str = "public";
/// Objects are polled by the collector; this only proves the agent answers.
const REPUBLISH_MS: u32 = 60_000;

/// The intended shape: `<snmp>` owns the UDP socket and the OID table, and
/// `<snmp_object>` binds a live sensor value into it. It is a proposal.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("snmp: connecting...");
    service!(ctx, home).wait_connected().await;

    match service!(ctx, agent).listen() {
        Ok(()) => ctx.logger.info("snmp: agent listening on UDP 161"),
        Err(_) => ctx.logger.warn("snmp: could not bind UDP 161"),
    }
}

pub async fn forever(ctx: &'static Context) {
    let agent = service!(ctx, agent);

    // A real agent answers on demand; publishing here is what the object
    // table would do when polled.
    let temperature = match device!(ctx, air).read() {
        Ok(reading) => f32::from(reading.temperature_centi_c) / 100.0,
        Err(_) => {
            ctx.logger.warn("snmp: sensor read failed");
            ctx.delay.delay_ms(REPUBLISH_MS).await;
            return;
        }
    };

    ctx.logger.info(format_args!(
        "snmp: 1.3.6.1.4.1.53864.1.1.0 = {temperature:.1}, community {COMMUNITY}, {} requests served",
        agent.requests_served()
    ));

    ctx.delay.delay_ms(REPUBLISH_MS).await;
}
