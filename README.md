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

40 of the 100 planned blocks are written: 6 ✅, 14 🟡, 20 ❌.

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
| 011 | [Network time & scheduled automations](blocks/011-network-time/README.md) | `time: sntp`, `on_time` | 🟡 | SNTP sync and local reads work; no timezones or `on_time` triggers |
| 012 | [Captive portal & Wi-Fi provisioning](blocks/012-captive-portal/README.md) | `wifi.ap`, `captive_portal` | ❌ | no AP mode, DNS responder or credential store |
| 013 | [Embedded web server & REST API](blocks/013-web-server/README.md) | `web_server` | ❌ | server-side HTTP absent; only client services exist |
| 014 | [MQTT transport & discovery](blocks/014-mqtt/README.md) | `mqtt` | ❌ | no MQTT client or entity registry for discovery |
| 015 | [Addressable RGB strip (RMT)](blocks/015-rmt-led-strip/README.md) | `esp32_rmt_led_strip` | ❌ | no RMT peripheral, `light` entity or effects |
| 016 | [UART sensor parsing](blocks/016-uart-sensor/README.md) | `uart`, `mhz19` | 🟡 | UART works; MHZ19 framing and checksum are in `app.rs` |
| 017 | [Pulse counter & rate metering](blocks/017-pulse-counter/README.md) | `pulse_counter` | 🟡 | software polling only; no PCNT or hardware edge counter |
| 018 | [Deep sleep & wakeup](blocks/018-deep-sleep/README.md) | `deep_sleep` | ❌ | no sleep element, RTC timer API or wake-pin config |
| 019 | [Globals & NVS](blocks/019-globals-nvs/README.md) | `globals`, `restore_value` | ❌ | no globals declaration or NVS persistence |
| 020 | [Display buffer & fonts](blocks/020-display-font/README.md) | `font`, `display` | 🟡 | SSD1306 framebuffer and bitmap fonts work; no TTF or lambdas |
| 021 | [Servos & calibrated PWM](blocks/021-servo-pwm/README.md) | `servo`, `output: ledc` | 🟡 | no LEDC/PWM; the 50 Hz pulse is software-timed |
| 022 | [Time-based cover & motors](blocks/022-cover-time-based/README.md) | `cover: platform: time_based` | 🟡 | relays and transit timing work; no cover entity or command path |
| 023 | [Quadrature rotary encoder](blocks/023-rotary-encoder/README.md) | `sensor: platform: rotary_encoder` | 🟡 | two polled GPIO inputs; no PCNT or interrupts |
| 024 | [1-Wire & temperature network](blocks/024-one-wire/README.md) | `one_wire`, `dallas_temp` | ❌ | no 1-Wire timing, ROM search or driver |
| 025 | [SPI sensor transceiver](blocks/025-spi-sensor/README.md) | `spi`, `max6675` | 🟡 | SPI and raw CS work; MAX6675 framing is in `app.rs` |
| 026 | [BLE scanner & presence](blocks/026-ble-tracker/README.md) | `esp32_ble_tracker`, `ble_presence` | ❌ | no BLE controller, scan or advertisement parser |
| 027 | [ESP-NOW peer messaging](blocks/027-esp-now/README.md) | `esp_now` | ❌ | no peer transport or unassociated radio mode |
| 028 | [Thermostat regulation](blocks/028-thermostat/README.md) | `climate: platform: thermostat` | ✅ | no climate entity; hysteresis and min-off are Rust |
| 029 | [Template sensor & derived math](blocks/029-template-sensor/README.md) | `sensor: platform: template` | ✅ | no template entity; the calculation is Rust |
| 030 | [Health diagnostics & watchdog](blocks/030-diagnostics/README.md) | `uptime`, `wifi_signal`, `restart` | ❌ | no RSSI, heap, uptime or reset API; no watchdog |
| 031 | [IR remote & protocol decoding](blocks/031-ir-remote/README.md) | `remote_transmitter`, `remote_receiver` | ❌ | no RMT carrier, edge capture or NEC decoder |
| 032 | [Ultrasonic distance](blocks/032-ultrasonic/README.md) | `sensor: platform: ultrasonic` | 🟡 | trigger and echo are GPIO; the echo width is polled, not captured |
| 033 | [RTTTL melodies](blocks/033-rtttl/README.md) | `rtttl`, `output: ledc` | ❌ | no PWM/tone output and no RTTTL parser |
| 034 | [Modbus RTU over RS485](blocks/034-modbus-rtu/README.md) | `modbus`, `modbus_controller` | 🟡 | UART and DE pin work; framing and CRC16 are in `app.rs` |
| 035 | [RFID/NFC tag authentication](blocks/035-rfid-nfc/README.md) | `pn532_i2c`, `pn532` | 🟡 | raw I²C reaches the reader; no PN532 driver or tag entity |
| 036 | [Multi-speed fan](blocks/036-fan-speed/README.md) | `fan: platform: speed` | ❌ | no PWM output and no fan entity |
| 037 | [Matrix keypad scanning](blocks/037-matrix-keypad/README.md) | `matrix_keypad` | 🟡 | rows/columns are GPIO; scan and lockout are in `app.rs` |
| 038 | [BLE GATT client](blocks/038-ble-client/README.md) | `ble_client` | ❌ | no BLE central role, GATT discovery or notifications |
| 039 | [Safe mode recovery](blocks/039-safe-mode/README.md) | `safe_mode`, `ota` | ❌ | no RTC boot counter, AP fallback or OTA |
| 040 | [Packages & composition](blocks/040-packages/README.md) | `packages` | ❌ | no includes, merge or override stage |

Blocks 011–020 move from pins and buses to provisioning, services and power.
The 🟡 blocks run today with the missing half written as Rust in `app.rs`
(`<ntp>`, `<uart>` and `<ssd1306>` are real); the ❌ blocks are the next
vocabulary to design: an AP/portal, an HTTP server, an MQTT client, RMT output,
deep sleep and NVS.

Blocks 021–030 leave the pin-and-bus basics for motion, single-wire and SPI
parts, radios and diagnostics. The 🟡 blocks run their behaviour in `app.rs` on
real elements (`<led>`, `<button>`, `<spi>`); the ❌ blocks are the next
vocabulary to design: 1-Wire, BLE, ESP-NOW and a health service. 028 and 029
are ✅ because their state machine and math are ordinary Rust over a real
`<scd4x>` reading, even though no climate or template entity exists to publish
them.

Blocks 031–040 are mostly protocols the runtime does not implement yet. 032,
034, 035 and 037 are 🟡 because the pins, the UART or a raw I²C bus exist and
the missing protocol lives in `app.rs` — with the caveat that polling replaces
input capture and the PN532 driver is hand-written. The ❌ blocks need a
hardware peripheral (RMT for IR, LEDC for tones and fan speed) or a compiler
feature (packages), not just a driver.

## Roadmap: blocks 041–100

`blocks/` stops at 040. The remaining 60 blocks are planned in one file per
phase at the repository root. They are proposals, not projects: there is no
`blocks/041-*` directory, and none of them has an `.espf.txt` or `app.rs` to
check.

| File | Blocks | Theme |
|---|---|---|
| [`41to50.md`](41to50.md) | 041–050 | I²S microphone and speaker, TWAI/CAN, stepper profiling, HX711, INA226, RGBWW, BLE proxy, scripts, Improv serial |
| [`51to60.md`](51to60.md) | 051–060 | PID loop, e-paper, I²C port expander, mmWave radar, VOC index, Wiegand, webhooks, sun tracking, BLE HID, WireGuard |
| [`61to70.md`](61to70.md) | 061–070 | NMEA GPS, LED strip zones, DFPlayer, AC phase cutting, multi-click, DS3231 RTC, SD card/FAT32, display menus, iBeacon, syslog |
| [`71to80.md`](71to80.md) | 071–080 | MPU-6050, energy monitoring, H-bridge, PMS5003, VL53L0X, TM1637, LED matrix, humidity control, mDNS, SNMP |
| [`81to90.md`](81to90.md) | 081–090 | Geiger counter, BH1750, soil moisture, irrigation, tachometer, HD44780, status LED, PIR, ICMP ping, alarm panel |
| [`91to100.md`](91to100.md) | 091–100 | AS5600, MAX31865, TCS34725, safety cover, DMX512, latching valves, BLE GATT server, factory reset, OTA rollback, capstone |

Each phase leans on elements the status table already calls missing — ADC
(006), LEDC/PWM (008, 033, 036), RMT (015, 031), deep sleep (018), packages
(040) — so the roadmap is the same vocabulary continued into audio, automotive
buses and stage lighting rather than a different project.
