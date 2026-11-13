use crate::{service, Context};
use embassy_executor::Spawner;

/// `facility: LOCAL0` is facility 16 in the syslog numbering.
const FACILITY_LOCAL0: u8 = 16;
/// `log_level: DEBUG` is severity 7.
const SEVERITY_DEBUG: u8 = 7;
/// A debug record every few seconds proves the link without flooding it.
const REPORT_MS: u32 = 5_000;

/// The syslog priority byte: facility in the high bits, severity in the low.
fn priority() -> u8 {
    FACILITY_LOCAL0 * 8 + SEVERITY_DEBUG
}

/// The intended shape: `<syslog>` installs a second sink in the logger, so
/// every record the console sees also leaves as a datagram. It is a proposal.
pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("syslog: connecting...");
    service!(ctx, home).wait_connected().await;

    let sink = service!(ctx, collector);
    match sink.connect().await {
        Ok(()) => ctx.logger.info(format_args!(
            "syslog: streaming to 192.168.1.50:514, priority {}",
            priority()
        )),
        Err(_) => ctx.logger.warn("syslog: collector unreachable"),
    }
}

pub async fn forever(ctx: &'static Context) {
    let sink = service!(ctx, collector);

    // `<logger level="debug">` lets this record through locally; the sink
    // would send the same text remotely.
    ctx.logger.info(format_args!(
        "syslog: heartbeat <{priority()}>"
    ));
    if sink.is_connected() {
        ctx.logger.info("syslog: collector link up");
    } else {
        ctx.logger.warn("syslog: collector link down");
    }

    ctx.delay.delay_ms(REPORT_MS).await;
}
