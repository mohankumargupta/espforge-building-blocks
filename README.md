# espforge-building-blocks

A companion to [`espforge`](espforge/README.md): imagine building an ESPHome
clone from scratch, one lego block at a time. Each block takes one ESPHome YAML
feature, states what a compiler/runtime needs to support it, and translates it
into an espforge project.

The ESPHome YAML is the specification. The espforge side is the target: a
project file (`.espf.txt`) plus the `src/app.rs` that drives it. Optional
overlay files next to the `.espf.txt` (`partitions.csv`, `Cargo.extra.toml`,
`.cargo/*.toml`, the Wokwi files) replace or extend what the generator writes;
`build/` is disposable output.

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

100 of the 100 planned blocks are written: 14 ✅, 39 🟡, 47 ❌.

| # | Block | ESPHome block | Status | What is missing today |
|---|---|---|---|---|
| 001 | [Minimal boot & serial logger](blocks/001-minimal/README.md) | `esphome`, `logger` | ✅ | — |
| 002 | [Wi-Fi & OTA](blocks/002-wifi-ota/README.md) | `wifi`, `ota` | 🟡 | Wi-Fi and `<ota>` A/B updates work; no upload endpoint or `password` — the transport is `app.rs` |
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
| 030 | [Health diagnostics & watchdog](blocks/030-diagnostics/README.md) | `uptime`, `wifi_signal`, `restart` | ❌ | no RSSI, heap or uptime API; no watchdog (reboot only via `<ota>`) |
| 031 | [IR remote & protocol decoding](blocks/031-ir-remote/README.md) | `remote_transmitter`, `remote_receiver` | ❌ | no RMT carrier, edge capture or NEC decoder |
| 032 | [Ultrasonic distance](blocks/032-ultrasonic/README.md) | `sensor: platform: ultrasonic` | 🟡 | trigger and echo are GPIO; the echo width is polled, not captured |
| 033 | [RTTTL melodies](blocks/033-rtttl/README.md) | `rtttl`, `output: ledc` | ❌ | no PWM/tone output and no RTTTL parser |
| 034 | [Modbus RTU over RS485](blocks/034-modbus-rtu/README.md) | `modbus`, `modbus_controller` | 🟡 | UART and DE pin work; framing and CRC16 are in `app.rs` |
| 035 | [RFID/NFC tag authentication](blocks/035-rfid-nfc/README.md) | `pn532_i2c`, `pn532` | 🟡 | raw I²C reaches the reader; no PN532 driver or tag entity |
| 036 | [Multi-speed fan](blocks/036-fan-speed/README.md) | `fan: platform: speed` | ❌ | no PWM output and no fan entity |
| 037 | [Matrix keypad scanning](blocks/037-matrix-keypad/README.md) | `matrix_keypad` | 🟡 | rows/columns are GPIO; scan and lockout are in `app.rs` |
| 038 | [BLE GATT client](blocks/038-ble-client/README.md) | `ble_client` | ❌ | no BLE central role, GATT discovery or notifications |
| 039 | [Safe mode recovery](blocks/039-safe-mode/README.md) | `safe_mode`, `ota` | ❌ | no RTC boot counter, boot deadline or AP fallback; `<ota>` now gives recovery a target |
| 040 | [Packages & composition](blocks/040-packages/README.md) | `packages` | ❌ | no `<include>` or config merge; file overlays only |
| 041 | [I²S digital microphone](blocks/041-i2s-microphone/README.md) | `i2s_audio`, `microphone` | ❌ | no I²S peripheral, DMA ring or microphone entity |
| 042 | [I²S audio output](blocks/042-i2s-speaker/README.md) | `i2s_audio`, `media_player` | ❌ | no I²S TX, DMA FIFO or media player |
| 043 | [TWAI / CAN bus](blocks/043-twai-can/README.md) | `canbus: esp32_twai` | ❌ | no TWAI controller, acceptance mask or frame API |
| 044 | [Stepper profiling](blocks/044-stepper-motor/README.md) | `stepper: a4988` | 🟡 | two GPIOs and the trapezoid work; the pulse train is software-timed |
| 045 | [HX711 scale](blocks/045-hx711-scale/README.md) | `sensor: hx711` | 🟡 | DOUT/SCK are GPIO; the 24-bit clocking and calibration are in `app.rs` |
| 046 | [INA226 power monitor](blocks/046-ina226-power/README.md) | `sensor: ina226` | 🟡 | raw I²C registers; no driver, and the scaling is in `app.rs` |
| 047 | [RGBWW light](blocks/047-rgbww-light/README.md) | `output: ledc`, `light: rgbww` | ❌ | no LEDC outputs and no light entity |
| 048 | [BLE proxy](blocks/048-ble-proxy/README.md) | `bluetooth_proxy` | ❌ | no BLE controller; a proxy also needs the native API |
| 049 | [Reentrant scripts](blocks/049-scripts/README.md) | `script` | 🟡 | the sequence is a state machine; no `<script>` or `wait_until` |
| 050 | [Improv serial](blocks/050-improv-serial/README.md) | `improv_serial` | ❌ | no framer, Wi-Fi scan or credential store; the console owns UART0 |
| 051 | [PID climate](blocks/051-pid-climate/README.md) | `climate: pid` | ❌ | the PID runs, but the heater duty has no PWM output |
| 052 | [e-paper display](blocks/052-epaper-display/README.md) | `display: waveshare_epaper` | ❌ | no e-paper driver: init table, LUTs, BUSY and framebuffer |
| 053 | [PCF8574 expander](blocks/053-pcf8574-expander/README.md) | `pcf8574` | 🟡 | the latch byte is in `app.rs`; no device or INT events |
| 054 | [mmWave radar](blocks/054-mmwave-radar/README.md) | `ld2410` | 🟡 | the LD2410 frame parser is in `app.rs`; no driver |
| 055 | [VOC index](blocks/055-voc-index/README.md) | `sensor: sgp40` | 🟡 | measure-raw with CRC is real; the VOC index algorithm is missing |
| 056 | [Wiegand keypad](blocks/056-wiegand/README.md) | `wiegand` | ❌ | needs edge capture with timestamps, not polling |
| 057 | [HTTP webhook](blocks/057-http-webhook/README.md) | `http_request` | ✅ | — |
| 058 | [Sun tracking](blocks/058-sun-tracking/README.md) | `sun` | 🟡 | NTP is real and the solar geometry is in `app.rs`; no `<sun>` |
| 059 | [BLE HID remote](blocks/059-ble-hid/README.md) | `ble_keyboard` | ❌ | no peripheral GATT server, HID descriptor or bonding |
| 060 | [WireGuard tunnel](blocks/060-wireguard/README.md) | — | ❌ | no UDP socket and no Curve25519/ChaCha20-Poly1305 |
| 061 | [NMEA GPS](blocks/061-gps-nmea/README.md) | `gps` | 🟡 | NMEA parsing is in `app.rs`; no `<gps>` device |
| 062 | [LED strip zones](blocks/062-led-zones/README.md) | `light: partition` | ❌ | no RMT strip and no shared framebuffer for zones |
| 063 | [DFPlayer](blocks/063-dfplayer/README.md) | `dfplayer` | 🟡 | the 10-byte command packet is in `app.rs`; no driver |
| 064 | [AC dimmer](blocks/064-ac-dimmer/README.md) | `output: ac_dimmer` | ❌ | needs timestamped zero-cross capture and a microsecond one-shot |
| 065 | [Multi-click](blocks/065-multi-click/README.md) | `on_multi_click` | ✅ | — |
| 066 | [DS3231 RTC](blocks/066-ds3231-rtc/README.md) | `time: ds3231` | 🟡 | BCD registers are read directly; no time-source element |
| 067 | [SD card logging](blocks/067-sd-card-logging/README.md) | `sd_card` | ❌ | no SPI block device and no FAT filesystem |
| 068 | [Display menu](blocks/068-display-menu/README.md) | `display_menu` | 🟡 | the menu engine is Rust over a real SSD1306 and buttons |
| 069 | [BLE iBeacon](blocks/069-ble-ibeacon/README.md) | `esp32_ble_beacon` | ❌ | no broadcaster-mode radio or advertising API |
| 070 | [UDP syslog](blocks/070-syslog/README.md) | `syslog` | ❌ | no UDP socket and no second logger sink |
| 071 | [MPU-6050 IMU](blocks/071-mpu6050-imu/README.md) | `sensor: mpu6050` | 🟡 | the 14-byte burst and scaling are in `app.rs`; no driver |
| 072 | [HLW8012 energy](blocks/072-hlw8012-energy/README.md) | `sensor: hlw8012` | ❌ | CF is a kilohertz pulse train; polling cannot count it |
| 073 | [H-bridge motor](blocks/073-hbridge-motor/README.md) | `fan: hbridge` | ❌ | no PWM outputs and no bridge driver with dead time |
| 074 | [PMS5003 particulate](blocks/074-pms5003/README.md) | `sensor: pmsx003` | 🟡 | the 32-byte frame parser is in `app.rs`; no driver |
| 075 | [VL53L0X ranging](blocks/075-vl53l0x/README.md) | `sensor: vl53l0x` | ❌ | a tuning sequence and calibration, not `app.rs` framing |
| 076 | [TM1637 display](blocks/076-tm1637-display/README.md) | `display: tm1637` | 🟡 | the two-wire protocol is bit-banged; DIO cannot read the ACK |
| 077 | [LED matrix](blocks/077-led-matrix/README.md) | `display: addressable_light` | ❌ | no RGB strip, colour framebuffer or font renderer |
| 078 | [Humidifier](blocks/078-humidifier/README.md) | `humidifier: template` | ✅ | — |
| 079 | [mDNS](blocks/079-mdns/README.md) | `mdns` | ❌ | no multicast UDP socket, mDNS responder or DNS-SD registry |
| 080 | [SNMP agent](blocks/080-snmp/README.md) | — | ❌ | no UDP server, ASN.1 BER codec or OID table |
| 081 | [Geiger counter](blocks/081-geiger-counter/README.md) | `pulse_counter` | 🟡 | a sampled edge count; a 100 µs pulse can slip between samples |
| 082 | [BH1750 illuminance](blocks/082-bh1750-illuminance/README.md) | `sensor: bh1750` | 🟡 | one command byte and a divide; no driver |
| 083 | [Soil moisture](blocks/083-soil-moisture/README.md) | `sensor: adc` | ❌ | no `<adc>` peripheral at all |
| 084 | [Irrigation zones](blocks/084-irrigation/README.md) | `sprinkler` | 🟡 | the relays and sequencing work; no `<sprinkler>` entity |
| 085 | [Tachometer fan](blocks/085-tach-fan/README.md) | `fan: speed`, `pulse_counter` | ❌ | needs 25 kHz PWM and a tach counter polling cannot keep up with |
| 086 | [LCD PCF8574](blocks/086-lcd-pcf8574/README.md) | `display: lcd_pcf8574` | 🟡 | the 4-bit LCD protocol is in `app.rs`; no driver |
| 087 | [Status LED](blocks/087-status-led/README.md) | `status_led` | ✅ | — |
| 088 | [PIR motion](blocks/088-pir-motion/README.md) | `binary_sensor`, `delayed_off` | ✅ | — |
| 089 | [ICMP ping](blocks/089-ping/README.md) | `binary_sensor: ping` | ❌ | no ICMP/raw socket, so no RTT |
| 090 | [Alarm panel](blocks/090-alarm-panel/README.md) | `alarm_control_panel` | 🟡 | the state machine works; no alarm entity or keypad code entry |
| 091 | [AS5600 angle](blocks/091-as5600-angle/README.md) | `sensor: as5600` | 🟡 | the 12-bit angle is read from 0x0E; no driver |
| 092 | [MAX31865 RTD](blocks/092-max31865-rtd/README.md) | `sensor: max31865` | 🟡 | the 15-bit RTD read and CVD math are in `app.rs`; no driver |
| 093 | [TCS34725 colour](blocks/093-tcs34725-color/README.md) | `sensor: tcs34725` | 🟡 | RGBC and the colour math are in `app.rs`; CCT is McCamy |
| 094 | [Gate cover](blocks/094-gate-cover/README.md) | `cover: endstop` | 🟡 | relays, endstops and the travel timeout work; no cover entity |
| 095 | [DMX512](blocks/095-dmx512/README.md) | — | ❌ | the UART is real, but BREAK/MAB timing is not |
| 096 | [Latching valve](blocks/096-latching-valve/README.md) | `switch: template` | ✅ | — |
| 097 | [BLE GATT server](blocks/097-ble-gatt-server/README.md) | `ble_server` | ❌ | no GATT attribute table, MTU negotiation or notifications |
| 098 | [Factory reset](blocks/098-factory-reset/README.md) | `on_click` | 🟡 | the hold timer works; there is no NVS to erase or plain reset |
| 099 | [OTA rollback](blocks/099-ota-rollback/README.md) | `ota: safe_mode` | ✅ | — |
| 100 | [Resilient edge node](blocks/100-edge-node/README.md) | the whole capstone | ✅ | — |

`# espforge` now also carries two project-wide elements: `<logger>`, which sets
per-subsystem levels (`drivers`, `network`, `hal`) and strips logging from flash
at `level="none"`, and `<ota />`, which adds A/B partitions, rollback and
`ctx.ota` (the app feeds it downloaded chunks, so the transport is up to you).
Block 001 predates `<logger>` and still shows the `ESP_LOG`-only form; block 002
uses the real `<ota />`, with the download left to `app.rs`.

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

The second half, 041–100, was written the same way as the first: one directory
per block with its README, `.espf.txt` and `src/app.rs`. Eight more blocks are
✅ because their behaviour is ordinary Rust over real elements — the HTTP
webhook (057), multi-click (065), humidity control (078), status LED (087), PIR
(088), the latching valve (096), OTA confirmation (099) and the capstone (100).

## Phase files: blocks 041–100

The one-file-per-phase documents at the repository root are the source for the
second half. Each block's README opens with the YAML from its phase file, then
says what espforge can and cannot do with it.

| File | Blocks | Theme |
|---|---|---|
| [`41to50.md`](41to50.md) | 041–050 | I²S microphone and speaker, TWAI/CAN, stepper profiling, HX711, INA226, RGBWW, BLE proxy, scripts, Improv serial |
| [`51to60.md`](51to60.md) | 051–060 | PID loop, e-paper, I²C port expander, mmWave radar, VOC index, Wiegand, webhooks, sun tracking, BLE HID, WireGuard |
| [`61to70.md`](61to70.md) | 061–070 | NMEA GPS, LED strip zones, DFPlayer, AC phase cutting, multi-click, DS3231 RTC, SD card/FAT32, display menus, iBeacon, syslog |
| [`71to80.md`](71to80.md) | 071–080 | MPU-6050, energy monitoring, H-bridge, PMS5003, VL53L0X, TM1637, LED matrix, humidity control, mDNS, SNMP |
| [`81to90.md`](81to90.md) | 081–090 | Geiger counter, BH1750, soil moisture, irrigation, tachometer, HD44780, status LED, PIR, ICMP ping, alarm panel |
| [`91to100.md`](91to100.md) | 091–100 | AS5600, MAX31865, TCS34725, safety cover, DMX512, latching valves, BLE GATT server, factory reset, OTA rollback, capstone |

The second half leans on the same missing vocabulary as the first — ADC (006,
083), LEDC/PWM (008, 047, 051, 073, 085), RMT (015, 062, 077), BLE (026, 038,
059, 069, 097), deep sleep (018) and a UDP socket layer (070, 079, 080, 089) —
plus a set of driver-only gaps where the bus exists but the part's protocol
does not (045, 046, 053, 055, 071, 076, 082, 086, 091, 092, 093). It is the
same project continued into audio, automotive buses and stage lighting.
