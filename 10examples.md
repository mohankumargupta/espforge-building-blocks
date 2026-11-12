Here is the condensed version formatted ready for `10examples.md`:

```markdown
# 10examples.md — ESPHome Clone: Vertical Slice Architecture

A progressive 10-step roadmap to implement an ESPHome-compatible compiler and runtime, from bare-metal boot to local automation logic.

---

## Phase 1: Core Runtime & Connectivity

### 1. Minimal Boot & Serial Logger
**Goal:** Validate code generator, toolchain compilation, basic event loop, and UART logging.
```yaml
esphome:
  name: test-node
  platform: ESP32
  board: esp32dev

logger:
  level: DEBUG
```

### 2. Wi-Fi & OTA Updates
**Goal:** Establish network connection and wireless flashing to enable fast iteration.
```yaml
wifi:
  ssid: "MyNetwork"
  password: "MyPassword"

ota:
  password: "admin"
```

### 3. Native API Transport
**Goal:** Implement the client transport protocol (protobuf/API or MQTT) for hub discovery and state streaming.
```yaml
api:
  encryption:
    key: "base64-encoded-key"
```

---

## Phase 2: Digital I/O & Hardware Abstractions

### 4. Basic Output & Switch (The Blink Test)
**Goal:** HAL pin mapping, state tracking, and exposing actionable entities to a dashboard.
```yaml
output:
  - platform: gpio
    pin: GPIO2
    id: onboard_led

switch:
  - platform: output
    name: "Onboard LED"
    output: onboard_led
```

### 5. Binary Sensor with Debounce Filters
**Goal:** GPIO input handling, pin interrupts, and component filter pipelines.
```yaml
binary_sensor:
  - platform: gpio
    pin:
      number: GPIO4
      mode: INPUT_PULLUP
      inverted: true
    name: "Physical Button"
    filters:
      - delayed_on: 50ms
```

### 6. Polling Sensor & Data Transformations
**Goal:** Periodic task scheduling (`update_interval`) and mathematical sensor filters.
```yaml
sensor:
  - platform: adc
    pin: GPIO34
    name: "Battery Voltage"
    update_interval: 10s
    filters:
      - multiply: 2.0
      - sliding_window_moving_average:
          window_size: 5
          send_every: 5
```

---

## Phase 3: Bus Protocols & Complex Peripherals

### 7. I²C Bus & Multi-Value Sensors
**Goal:** Shared bus management, address arbitration, and multi-metric sensor entities.
```yaml
i2c:
  sda: GPIO21
  scl: GPIO22
  scan: true

sensor:
  - platform: bme280
    address: 0x76
    update_interval: 60s
    temperature:
      name: "Room Temperature"
    humidity:
      name: "Room Humidity"
```

### 8. PWM Output & Dimmable Light
**Goal:** Hardware timers (LEDC/PWM), brightness curves, and non-blocking state transitions.
```yaml
output:
  - platform: ledc
    pin: GPIO18
    id: pwm_pin

light:
  - platform: monochromatic
    name: "Desk Lamp"
    output: pwm_pin
    default_transition_length: 1s
```

---

## Phase 4: Local Intelligence & Ergonomics

### 9. Local Automations (Triggers & Actions)
**Goal:** Compile YAML event triggers into embedded C++ callbacks that run offline without server hops.
```yaml
binary_sensor:
  - platform: gpio
    pin: GPIO4
    name: "Wall Switch"
    on_press:
      then:
        - light.toggle: desk_lamp
```

### 10. Substitutions & Secrets
**Goal:** Preprocessor templating, string interpolation, and external secrets resolution.
```yaml
substitutions:
  device_name: living-room-lamp

esphome:
  name: ${device_name}

wifi:
  ssid: !secret wifi_ssid
  password: !secret wifi_password
```
```
