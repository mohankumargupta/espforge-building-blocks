# 032 — Ultrasonic Distance & Flight Timing

**Status:** 🟡 partly possible — the trigger is a GPIO pulse and the echo is polled; there is no input capture
**ESPHome lego:** `sensor: platform: ultrasonic`

## ESPHome

```yaml
sensor:
  - platform: ultrasonic
    trigger_pin: GPIO5
    echo_pin: GPIO6
    name: "Sump Pit Water Level"
    update_interval: 2s
    timeout: 3m
```

An HC-SR04 is strobed with a 10 µs pulse, then drives ECHO high for as long as
the sound takes to fly out and back. At 343 m/s that is 58 µs per centimetre of
round trip, so the echo width is the measurement.

## espforge

`ultrasonic.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `trigger_pin: GPIO5` | `<led id="trig" pin="5" />`: `on()` for 10 µs, then `off()` |
| `echo_pin: GPIO6` | `<button id="echo" pin="6" />`, no pull-up (the module drives it) |
| `timeout: 3m` | `TIMEOUT_US = 25_000` bounds both waits (~4 m round trip) |
| `update_interval: 2s` | `delay_ms(2_000)` |
| duration measurement | `delay_ns(2_000)` polls; each poll counts 2 µs |
| distance conversion | `high_us * 0.0343 / 2.0` cm |
| `name:` | none; the reading is logged |

Two limits are worth stating. First, the echo width is quantised by the poll
period plus loop overhead, so expect centimetre-level noise rather than the
millimetre resolution a capture timer gives. Second, many HC-SR04 modules drive
ECHO at 5 V; the ESP32-C3 is a 3.3 V part, so a divider belongs between ECHO
and the pin.

### What is missing

Input capture (or a pin interrupt with a hardware timestamp) and a timeout that
does not depend on the polling loop. The GPIO pulse and the arithmetic are
already expressible, which is why this block is 🟡 rather than ❌.
