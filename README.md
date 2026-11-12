# espforge-building-blocks

A companion to [`espforge`](espforge/README.md): imagine building an ESPHome
clone from scratch, one lego block at a time. Each block takes one ESPHome YAML
feature, states what a compiler/runtime needs to support it, and translates it
into an espforge project.

The ESPHome YAML is the specification. The espforge side is the target: a
project file (`.espf.txt`) plus the `src/app.rs` that drives it.

espforge itself lives at `./espforge` (git-ignored: it is its own checkout).

## Layout

```
blocks/
  001-minimal/
    README.md          # the ESPHome YAML for this block + translation notes
    minimal.espf.txt   # the espforge project file
    src/app.rs         # the application half
```

Every block's README opens with its status:

| Symbol | Meaning |
|---|---|
| ✅ | possible today: only implemented espforge elements are used |
| 🟡 | partly possible: part of the block already exists, part is proposed |
| ❌ | not possible yet: the files are an ideal API proposal |

## Status

| # | Block | ESPHome block | Status | What is missing today |
|---|---|---|---|---|
| 001 | [Minimal boot & serial logger](blocks/001-minimal/README.md) | `esphome`, `logger` | ✅ | — |
| 002 | [Wi-Fi & OTA](blocks/002-wifi-ota/README.md) | `wifi`, `ota` | 🟡 | Wi-Fi works; no `<ota>` service |
| 003 | [Native API transport](blocks/003-native-api/README.md) | `api` | ❌ | no `<api>` service; only client services (HTTP/WS/NTP) |
| 004 | [Output & switch (blink)](blocks/004-gpio-switch/README.md) | `output`, `switch` | ✅ | no entity names; the `id` is the handle |
| 005 | [Binary sensor & debounce](blocks/005-binary-sensor/README.md) | `binary_sensor`, `filters` | ✅ | no declarative filter pipeline: debounce is Rust in `app.rs` |
| 006 | [ADC sensor & filters](blocks/006-adc-sensor/README.md) | `sensor: platform: adc` | ❌ | no `<adc>` element; bundled sensors are bus parts |
| 007 | [I²C BME280](blocks/007-i2c-bme280/README.md) | `i2c`, `sensor: platform: bme280` | 🟡 | I²C and `<scd4x>` work; no `<bme280>` driver |
| 008 | [PWM output & dimmable light](blocks/008-pwm-light/README.md) | `output: platform: ledc`, `light` | ❌ | no `<pwm>`/LEDC peripheral, no `<light>` device |
| 009 | [Local automations](blocks/009-automations/README.md) | `on_press` / `then` | ✅ | triggers compile to Rust in `forever`, not to declarative actions |
| 010 | [Substitutions & secrets](blocks/010-substitutions-secrets/README.md) | `substitutions`, `!secret` | ❌ | no `${}` interpolation or `!secret`; `wifi_station password="env:NAME"` only |

The three ❌ blocks (and the missing half of 002) are the clearest gaps between
ESPHome's vocabulary and espforge's: an API server, analog input, and PWM/light
output are the next lego blocks to design.
