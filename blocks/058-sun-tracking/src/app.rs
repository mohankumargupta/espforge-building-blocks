use crate::{service, Context};
use embassy_executor::Spawner;

/// `latitude: 52.5200°` north, `longitude: 13.4050°` east.
const LATITUDE_DEG: f64 = 52.52;
const LONGITUDE_DEG: f64 = 13.405;
/// `on_sunset: elevation: -4.0°` — civil twilight.
const SUNSET_ELEVATION_DEG: f64 = -4.0;
/// The elevation moves ~0.25 degrees a minute; 30 s is finely enough resolved.
const SAMPLE_MS: u32 = 30_000;
const SECONDS_PER_DAY: u64 = 86_400;

/// Solar elevation in degrees for a UTC timestamp (NOAA's low-precision form).
fn solar_elevation(unix_seconds: u64) -> f64 {
    let seconds_in_day = (unix_seconds % SECONDS_PER_DAY) as f64;
    let day_of_year = ((unix_seconds / SECONDS_PER_DAY) % 365) as f64 + 1.0;

    // Fractional hours of UTC, then the hour angle (15 degrees per hour,
    // zero at solar noon, which is 12:00 UTC minus the longitude correction).
    let utc_hours = seconds_in_day / 3_600.0;
    let hour_angle = (utc_hours - 12.0) * 15.0 + LONGITUDE_DEG;

    // Cooper's equation: declination from the day of the year.
    let declination = 23.45 * libm::sin(360.0 / 365.0 * (284.0 + day_of_year) * core::f64::consts::PI / 180.0);

    let lat = LATITUDE_DEG * core::f64::consts::PI / 180.0;
    let dec = declination * core::f64::consts::PI / 180.0;
    let ha = hour_angle * core::f64::consts::PI / 180.0;

    let sin_elevation = libm::sin(lat) * libm::sin(dec) + libm::cos(lat) * libm::cos(dec) * libm::cos(ha);
    libm::asin(sin_elevation) * 180.0 / core::f64::consts::PI
}

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    ctx.logger.info("sun-tracking: connecting...");
    service!(ctx, home).wait_connected().await;

    if service!(ctx, clock).sync().await.is_err() {
        ctx.logger.warn("sun-tracking: NTP sync failed; the sun cannot be located");
    } else {
        ctx.logger.info("sun-tracking: 52.52 N 13.41 E, sunset at -4 degrees");
    }
}

pub async fn forever(ctx: &'static Context) {
    let elevation = match service!(ctx, clock).now() {
        Ok(now) => solar_elevation(now.seconds),
        Err(error) => {
            ctx.logger.warn(format_args!("sun-tracking: {error}"));
            ctx.delay.delay_ms(SAMPLE_MS).await;
            return;
        }
    };

    let light = device!(ctx, porch_light);
    if elevation < SUNSET_ELEVATION_DEG {
        light.on();
        ctx.logger.info(format_args!(
            "sun: elevation {elevation:.1} deg, porch light on"
        ));
    } else {
        light.off();
        ctx.logger.info(format_args!(
            "sun: elevation {elevation:.1} deg, porch light off"
        ));
    }

    ctx.delay.delay_ms(SAMPLE_MS).await;
}
