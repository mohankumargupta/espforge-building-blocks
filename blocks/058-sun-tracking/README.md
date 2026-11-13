# 058 — Astronomical Sun Tracking & Celestial Triggers

**Status:** 🟡 partly possible — NTP time is real and the solar geometry is in `app.rs`; there is no `<sun>` element
**ESPHome lego:** `sun:` with `on_sunrise` / `on_sunset`

## ESPHome

```yaml
sun:
  latitude: 52.5200°
  longitude: 13.4050°
  on_sunset:
    - elevation: -4.0°
      then:
        - light.turn_on: porch_light
```

`sun:` computes solar elevation from the date, the time and the position, so
sunset can be defined as an angle rather than a clock time. An elevation of
-4° is civil twilight, which is when a porch light is actually wanted.

## espforge

`sun-tracking.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `latitude: 52.5200°` | `LATITUDE_DEG` (positive north) |
| `longitude: 13.4050°` | `LONGITUDE_DEG` (positive east) |
| `on_sunset: elevation: -4.0°` | `SUNSET_ELEVATION_DEG`, checked every 30 s |
| time base | `<ntp id="clock" network="$home" />` is real, one sync then local reads |
| timezones | none: the calculation works in UTC, so no zone is needed |
| `light.turn_on` | `porch_light.on()` on an `<led>` |
| `on_sunrise` | the same check with the opposite comparison |

The declination and hour-angle arithmetic is genuine (Cooper's equation for
declination, 15° per hour of hour angle). It uses `libm`'s `sin`/`cos`/`asin`,
which a project using this block adds through `Cargo.extra.toml`.

### What is missing

A `<sun>` service that owns the location, evaluates the two events once per
minute, and fires them as triggers — plus a light entity to switch.
