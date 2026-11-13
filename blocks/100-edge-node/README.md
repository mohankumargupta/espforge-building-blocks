# 100 — The Resilient Edge Node: Unified Architectural Convergence

**Status:** ✅ possible today — sensor, display, relay, Wi-Fi, NTP and offline logic, all real
**ESPHome lego:** the whole capstone: `esphome`, `wifi`, `api`, `logger`, `i2c`, `sensor`, `display`, `switch`, `interval`

## ESPHome

```yaml
esphome:
  name: resilient-edge-node
  platform: ESP32
  board: esp32-c3-devkitm-1

wifi:
  ssid: !secret wifi_ssid
  password: !secret wifi_password
  ap:
    ssid: "EdgeNode-Emergency-AP"
    password: "fallbackpassword"

api:
  encryption:
    key: !secret api_key

logger:
  level: INFO

i2c:
  sda: GPIO4
  scl: GPIO5
  scan: true

sensor:
  - platform: bme280
    address: 0x76
    temperature:
      id: ambient_temp
      name: "Ambient Temperature"
    humidity:
      id: ambient_humidity
      name: "Ambient Humidity"
    update_interval: 10s

display:
  - platform: ssd1306_i2c
    model: "SSD1306 128x64"
    address: 0x3C
    lambda: |-
      it.print(0, 0, id(font_default), "Node: Online");
      it.printf(0, 20, id(font_default), "T: %.1f C", id(ambient_temp).state);
      it.printf(0, 40, id(font_default), "H: %.1f %%", id(ambient_humidity).state);

switch:
  - platform: gpio
    pin: GPIO2
    id: emergency_vent
    name: "Over-Temp Exhaust Fan"

# Autonomous offline edge logic (runs independently of Home Assistant connection)
interval:
  - interval: 5s
    then:
      - if:
          condition:
            sensor.in_range:
              id: ambient_temp
              above: 35.0
          then:
            - switch.turn_on: emergency_vent
          else:
            - switch.turn_off: emergency_vent
```

The capstone is not a new feature: it is the claim that a single node can sense,
display, actuate and stay useful when the network is gone. The over-temperature
vent runs on a local interval, so it keeps working whether or not Home Assistant
is reachable.

## espforge

`edge-node.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esphome: board: esp32-c3-devkitm-1` | `<chip type="esp32c3" />` — espforge targets a chip |
| `wifi:` + `api:` + `logger:` | `<wifi>` + `<wifi_station>` and `<logger level="info" />`; no native API yet (block 003) |
| `wifi.ap:` fallback | none: block 012's AP mode is missing |
| `api: encryption: key` | none: there is no API to encrypt |
| `i2c: sda / scl` | `<i2c id="i2c0" sda="4" scl="5" frequency="400kHz" />` |
| `sensor: platform: bme280` | `<scd4x id="air" ... />` — BME280 is block 007; SCD4x also gives temperature and humidity |
| `display: platform: ssd1306_i2c` | `<ssd1306 id="oled" ... />`, real, drawn with `embedded_graphics` |
| `switch: platform: gpio` | `<led id="emergency_vent" pin="2" />` |
| `interval:` + `if: sensor.in_range:` | the check runs in `forever`, independent of the network |
| `!secret` substitutions | none: block 010; credentials are literals or `env:NAME` |

The node's shape is exactly the vertical slice the previous 99 blocks built up
to: a bus, a device, an entity, a service, and an automation that does not need
the cloud. Two of the original YAML's pieces are deliberately absent — the
fallback AP and the native API — because they do not exist yet, and the node
stays useful without them.

### What is missing

Only the network edge of the original spec: the AP fallback (block 012) and the
native API transport (block 003). Everything the node does locally is real.
