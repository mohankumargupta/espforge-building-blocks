# `top5.md`: Top 5 Architectural Changes to Eliminate Structural Weaknesses in `espforge`

**Document Version:** 1.0.0  
**Target Architecture:** `espforge` Next-Gen Core & Runtime  
**Scope:** `espforge-schema`, `espforge-elements`, `espforge-runtime`, `espforge-emit`, and `espforge-core`

---

## Executive Summary

While `espforge` versions 0.8 through 0.13 established a sound 4-section grammar (`# espforge`, `# peripherals`, `# devices`, `# services`), transactional pin claiming, unified `device!` macro mechanics, and a v2 driver manifest format, several foundational structural ceilings remain.

Without addressing these weaknesses, `espforge` cannot scale to:
1. **Industrial and mixed-interface hardware** (ADCs, PWM timers, pulse engines, CAN bus).
2. **True multi-homed networking** (wired Ethernet, Cellular PPP, Wi-Fi SoftAP).
3. **Quiet, flash-efficient firmware** (fine-grained logging control without serial noise or dead-string flash bloat).
4. **Battery-operated and mission-critical devices** (power gating, deep-sleep RTC power domains, and dynamic bus recovery).
5. **Real-world composite hardware modules** (cross-bus touch screens, multi-sensor breakout boards).

Below are the **Top 5 Architectural Changes** required to remove these latent limitations.

---

```
                                      CURRENT STATE                                                    TARGET ARCHITECTURE
  ┌───────────────────────────────────────────────────────────────────────────────────┐    ┌───────────────────────────────────────────────────────────────────────────────────┐
  │ 1. Silicon Hardware: Only GPIO, I2C, SPI, UART, Wi-Fi                             │    │ 1. Silicon Resource Matrix: ADC, LEDC (PWM), RMT, TWAI/CAN, DMA, & Silicon Locks │
  │ 2. Networking: Single-homed Wi-Fi Station only                                    │ ═► │ 2. Multi-Transport Network Engine: Ethernet (W5500/EMAC), Cellular PPP, SoftAP   │
  │ 3. Logging: Hardcoded ESP_LOG="info", noisy third-party crates, flash bloat       │    │ 3. Zero-Cost Compile-Time Level Gating & Subsystem Target Filtering (<logger>)     │
  │ 4. Lifecycle: Static boot or deferred init; no power gating or deep-sleep RTC     │    │ 4. Formal Lifecycle State Machine: Sleep, Power Rails, and Bus Lockup Recovery   │
  │ 5. Composition: Single-bus, flat sibling devices (tft, tft_touch); no cross-bus   │    │ 5. Hierarchical Multi-Bus Device Composition (e.g. SPI Display + I2C Touch)      │
  └───────────────────────────────────────────────────────────────────────────────────┘    └───────────────────────────────────────────────────────────────────────────────────┘
```

---

## Change 1: Comprehensive Silicon Resource Matrix (ADC, PWM/LEDC, RMT, TWAI & Silicon Interlocks)

### 1.1 The Structural Weakness
The `# peripherals` model in `espforge-elements/src/peripherals/` only knows five peripherals: `gpio`, `i2c`, `spi`, `uart`, and `wifi`. The `ResourceGraph` only models GPIO pins and named peripheral singletons.

Crucial on-chip silicon engines are unmodelled:
* **Analog-to-Digital Conversion (ADC):** Analog sensors, dials, and battery voltage dividers cannot be represented as devices because there is no `<adc>` peripheral to calibrate reference voltages or attenuation.
* **Silicon Hardware Interlocks (The ADC2/Wi-Fi Hazard):** On Espressif silicon, **`ADC2` is shared with the Wi-Fi baseband**. Calling `adc2_read` while Wi-Fi transmits corrupts the reading or crashes the Wi-Fi PHY. `espforge` has no concept of shared silicon blocks with mutually exclusive operational modes.
* **Hardware Timers & Precision Pulse Engines:** LEDC (hardware PWM for dimming and servos), RMT (microsecond pulse engines required for WS2812B NeoPixels and IR remotes), and TWAI/CAN (automotive and industrial robotics) cannot be configured.
* **DMA Channels:** High-speed SPI and I2S controllers share a pool of DMA channels; current codegen assigns them implicitly, risking DMA allocation panics when multiple high-throughput peripherals run simultaneously.

### 1.2 Target Architecture
1. **Extend `ResourceGraph` with Silicon Block Capabilities & Interlocks:**
   Introduce hardware domain constraints into `espforge-elements/src/model.rs`:
   ```rust
   pub enum SiliconResource {
       Peripheral(String),
       AdcUnit(u8),
       DmaChannel(u8),
       PwmTimer(u8),
       RmtChannel(u8),
   }
   ```
2. **Mutual Exclusion Interlocks:**
   When `<wifi id="radio">` is claimed, `LowerCx` automatically stages an exclusion lock on `SiliconResource::AdcUnit(2)`. Any subsequent device attempting to sample ADC2 emits `E0710: SILICON_INTERLOCK_CONFLICT`:
   ```
   error: ADC2 cannot be used concurrently with Wi-Fi
     <potentiometer id="dial" adc="adc2" pin="4" />
     `radio` claims the Wi-Fi baseband, which shares physical SAR logic with ADC2.
     fix: Move `dial` to ADC1 (GPIO0..=GPIO3 on ESP32-C3).
   ```
3. **Core Silicon Elements in `espf.schema.kdl`:**
   Add `<adc>` (attenuation, resolution), `<pwm>` / `<ledc>` (timer frequency, resolution), `<rmt>` (clock dividers), and `<twai>` (CAN bitrates and filter masks).

### 1.3 Example Specification
```xml
# peripherals
  <adc  id="adc1" unit="1" attenuation="11dB" resolution="12bit" />
  <pwm  id="servo_timer" frequency="50Hz" resolution="14bit" />
  <rmt  id="pulse_engine" channel="0" />
  <twai id="can_bus" tx="4" rx="5" bitrate="500kbps" />

# devices
  <potentiometer id="speed_knob" adc="$adc1" pin="2" />
  <servo         id="steering"   pwm="$servo_timer" pin="18" />
  <ws2812        id="status_led" rmt="$pulse_engine" pin="8" leds="1" />
```

---

## Change 2: Multi-Transport Network Provider Engine (Ethernet, Cellular PPP & SoftAP)

### 2.1 The Structural Weakness
In `espforge-runtime/src/hw/wifi.rs` and `espforge-elements/src/services/recipe.rs`, networking is conceptually and practically fused with the on-chip Wi-Fi station:
* All network protocol services (`<http_client>`, `<https_client>`, `<ws_client>`, `<wss_client>`, `<ntp>`) take an interface that `provides "network"`. While `network.md` established this capability seam, **only `<wifi_station>` implements it**.
* Real-world gateways, industrial controllers, and agricultural monitors cannot use:
  * **Wired Ethernet:** SPI-attached W5500/ENC28J60 or native EMAC via RMII (LAN8720).
  * **Cellular / LTE Modems:** SIMCom/Quectel modems communicating via PPP over UART (now that `BusTransport::Uart` exists).
  * **Wi-Fi Access Point (`<wifi_ap>`):** Provisioning portals or local configuration interfaces cannot serve HTTP endpoints without joining an upstream router.
* Furthermore, `HttpClient` and TLS clients cannot bind to specific interfaces on multi-homed devices (e.g., primary Ethernet with cellular backup).

### 2.2 Target Architecture
1. **Generic Network Provider Abstraction in Runtime:**
   Formalize `NetworkInterface` in `espforge-runtime` into a first-class stack factory:
   ```rust
   pub trait NetworkProvider: 'static {
       fn stack(&self) -> embassy_net::Stack<'static>;
       async fn wait_connected(&self);
       fn is_connected(&self) -> bool;
   }
   ```
2. **Implement Wired & Cellular Providers:**
   * **`<ethernet_w5500>` / `<ethernet_lan8720>`:** Implemented as a `# devices` or `# peripherals` element that attaches to an SPI bus or internal MAC, runs `embassy_net::new()` over the driver's runner, and declares `provides "network"`.
   * **`<cellular_ppp>`:** Implemented as a service/device that binds to a `bus="$uartX"`, runs the AT command sequence to establish PDP context, and feeds frames to `embassy-net` via an async PPP runner.
3. **Multi-Interface Routing:**
   Allow network services to specify fallback chains or interface pools:
   ```xml
   <http_client id="telemetry" network="$eth_primary" fallback="$lte_backup" />
   ```

### 2.3 Example Specification
```xml
# peripherals
  <spi  id="spi2" sclk="8" mosi="7" miso="6" />
  <uart id="uart1" tx="16" rx="17" baud_rate="115200" />

# devices
  <!-- SPI Ethernet Controller -->
  <w5500 id="eth0" bus="$spi2" cs="10" int="4" reset="5" />

# services
  <!-- W5500 provides "network" identically to wifi_station -->
  <ethernet_station id="wired_net" device="$eth0" dhcp="true" />
  
  <!-- HTTP Client talks over Ethernet with zero Wi-Fi dependencies -->
  <http_client id="cloud_client" network="$wired_net" />
```

---

## Change 3: Declarative Subsystem Logging & Zero-Cost Compile-Time Flash Pruning

### 3.1 The Structural Weakness
As detailed in `logging.md`:
* **Chatty Subsystem Flooding:** Enabling `<wifi_station>` or `<http_client>` pulls in `esp-radio`, `smoltcp`, `esp-rtos`, and `embassy-net`. These third-party crates flood the serial monitor with internal PHY calibration, DHCP discoveries, TCP ACKs, and socket state transitions.
* **Unconditional Feature Activation:** `espforge-emit/src/codegen/cargo.rs` unconditionally adds `"log-04"` to `esp-hal`, `esp-radio`, `esp-rtos`, and `"log"` to `smoltcp` and `embassy-*`.
* **Flash & RAM Bloat on Resource-Constrained Chips:** On small chips like the ESP32-C2 and ESP32-C3, formatting strings and display routines for quiet background libraries consume 30 KB to 70 KB of flash.
* **`SeedOnce` Invalidation Failure:** `.cargo/esp-config.toml` sets `ESP_LOG="info"`, but is marked `Ownership::SeedOnce`. If a developer attempts to customize log filters, subsequent `espforge build` runs **never update the file**.

### 3.2 Target Architecture
1. **First-Class `<logger>` Element under `# espforge`:**
   Add a declarative logging syntax to `espf.schema.kdl`:
   ```kdl
   element "logger" section="espforge" {
       attr "default" kind="choice" default="info" {
           choices "none" "error" "warn" "info" "debug" "trace"
       }
       attr "network"  kind="choice" default="inherit" { choices "inherit" "none" "error" "warn" "info" "debug" }
       attr "drivers"  kind="choice" default="inherit" { choices "inherit" "none" "error" "warn" "info" "debug" }
       attr "hal"      kind="choice" default="warn"    { choices "inherit" "none" "error" "warn" "info" "debug" }
       attr "app"      kind="choice" default="info"    { choices "none" "error" "warn" "info" "debug" "trace" }
   }
   ```
2. **Compile-Time Level Stripping:**
   When `default="none"` or `app="none"`, `espforge-emit/src/codegen/cargo.rs`:
   * Injects `features = ["max_level_off", "release_max_level_off"]` into the `log` crate dependency.
   * Omits the `"log"` / `"log-04"` features from `esp-radio`, `smoltcp`, and `esp-hal`.
   * Strips all formatting logic from the final ELF via link-time garbage collection.
3. **Structured Target Filter Dispatcher in `espforge-runtime`:**
   Replace the raw call to `esp_println::logger::init_logger_from_env()` with a runtime target filter in `generated.rs`:
   ```rust
   // Emitted into src/generated.rs
   pub static LOG_FILTERS: &[(&str, log::LevelFilter)] = &[
       ("esp_radio", log::LevelFilter::Error),
       ("smoltcp", log::LevelFilter::Off),
       ("espforge_runtime::hw::wifi", log::LevelFilter::Warn),
       ("espforge_drivers::scd4x", log::LevelFilter::Debug),
       ("app", log::LevelFilter::Info),
   ];
   ```

### 3.3 Example Specification
```xml
# espforge
  <project name="silent_logger" />
  <chip type="esp32c3" />
  <runtime type="embassy" />
  <!-- Silence all system noise, but allow debug logs from our air quality sensor -->
  <logger default="error" network="none" hal="none" app="info" />

# peripherals
  <i2c id="i2c0" sda="6" scl="7" />
  <wifi id="radio" />

# devices
  <scd4x id="air" bus="$i2c0" log="debug" />

# services
  <wifi_station id="home" wifi="$radio" ssid="Office" />
```

---

## Change 4: First-Class Power Domains, Deep-Sleep Wake & Dynamic Hardware Recovery

### 4.1 The Structural Weakness
Currently, device initialization in `espforge` is static and binary:
* **The "Panic-on-Boot" Ceiling:** Devices are either built at boot in `Devices::new() -> Result<Devices, DeviceError>` (which halts on failure) or wrapped in `Deferred<T>` (which must be manually initialized once by user code).
* **No Power Rail Sequencing:** Real-world sensors (e.g. GPS, air quality sensors, LTE modems) sit on switchable power rails controlled by a P-channel MOSFET or a dedicated low-dropout regulator (LDO) enable pin. Booting the device before the power rail is stabilized produces I2C NACKs.
* **No Bus Lockup Recovery:** I2C devices frequently latch the SDA line LOW if the MCU resets during a read cycle. `espforge-runtime` has no recovery routine (e.g., manual 9-clock SCL toggling to clear the slave shift register).
* **Zero Deep-Sleep & RTC Domain Integration:** The ESP32's Always-On (AON) power domain, Ultra-Low-Power (ULP) coprocessor, and RTC wake-up pins (`esp_hal::rtc_cntl`) are completely unrepresented.

### 4.2 Target Architecture
1. **Power Rail Gating (`power_pin` attribute):**
   Allow devices to declare a dedicated power enable pin:
   ```xml
   <scd4x id="air" bus="$i2c0" power_pin="4" power_delay="50ms" />
   ```
   Codegen stages the power pin assertion *before* running the device constructor or bus probe:
   ```rust
   // Inside Devices::new / Factory::init
   let mut pwr = esp_hal::gpio::Output::new(registry.peripherals.GPIO4, Level::High, ...);
   delay.delay_millis(50);
   ```
2. **Automatic I2C Bus Un-wedge Sequence:**
   Before initializing `I2cBus`, the runtime runs a hardware bus recovery: if SDA is detected low, it clocks SCL 9 times to free the bus slave before attaching the hardware peripheral.
3. **RTC Deep-Sleep Wake Elements in `# peripherals`:**
   Add `<rtc>` and `<wake_trigger>`:
   ```xml
   # peripherals
     <rtc id="pwr_ctrl" sleep_mode="deep" />

   # devices
     <button id="wake_btn" pin="0" rtc_wake="low" />
   ```
   `espforge-emit` generates the appropriate sleep entry calls and preserves context across reboots.

### 4.3 Architectural State Diagram
```
                     ┌──────────────────┐
                     │   Power Off      │
                     └────────┬─────────┘
                              │ power_pin.set_high()
                              ▼
                     ┌──────────────────┐
                     │ Power Stabilized │
                     └────────┬─────────┘
                              │ bus.recover_if_wedged()
                              ▼
                     ┌──────────────────┐
                     │ Hardware Probed  │◄────────────┐
                     └────────┬─────────┘             │
         init() error         │ init() ok             │ Bus NACK / Timeout
     ┌────────────────────────┴────────┐              │
     ▼                                 ▼              │
┌──────────────┐              ┌────────────────┐      │
│ Fault/Retry  │              │     Ready      ├──────┘
└──────────────┘              └────────┬───────┘
                                       │ sleep_enter()
                                       ▼
                              ┌────────────────┐
                              │ RTC Deep Sleep │
                              └────────────────┘
```

---

## Change 5: Hierarchical Multi-Bus Device Composition

### 5.1 The Structural Weakness
In `composition.md`, `compose "<tag>"` was introduced as a single-bus sibling generator:
```kdl
compose "touch" {
    attr "cs" from="touch_cs"
    attr "irq" from="touch_irq"
}
```
While this successfully allowed an SPI display to generate `pub tft` and `pub tft_touch` in `Devices`, it suffers from major structural limitations:
1. **Single-Bus Constraint:** It requires the composed part to share the exact same bus controller as the parent. In real-world hardware, **composite modules almost always cross bus boundaries**. 
   * *Example:* 90% of commodity touch screens on the market (e.g. 2.8" / 3.5" TFT shields) use **SPI for the ILI9341 display and I2C for the FT6236 capacitive touch controller**.
   * Under current `espforge`, this cannot be modelled as one physical module; the user is forced to declare them as disconnected devices.
2. **Flat Context Namespace:** Composed parts emit flat sibling names (`ctx.devices.tft_touch`). There is no encapsulation or grouped ownership.
3. **Lack of Internal Event Plumbing:** A composite part cannot route internal signals (e.g., touch interrupt pin firing an internal wake event on the display) without exposing raw pins to user application logic.

### 5.2 Target Architecture
1. **Multi-Bus Binding in v2 Manifests:**
   Allow a driver manifest to declare secondary bus requirements:
   ```kdl
   driver "ili9341_cst816s" {
       summary "2.8 inch TFT with Capacitive Touch"
       category "display"

       bus "spi" as="display_bus" { mode 0; frequency "40MHz" }
       bus "i2c" as="touch_bus"   { address default="0x15" }

       pin "dc"        direction="output"
       pin "touch_irq" direction="input" pull="up"

       compose "cst816s" {
           bus  "touch_bus"
           attr "irq" from="touch_irq"
       }
       ...
   }
   ```
2. **Hierarchical Device Accessor Pattern:**
   Instead of flattening all sub-devices into the top-level `Devices` struct, generate a nested composite struct:
   ```rust
   pub struct Ili9341Cst816s {
       pub display: Ili9341Display,
       pub touch: Cst816sTouch,
   }
   ```
   Application code interacts with the module cleanly:
   ```rust
   let panel = device!(ctx, screen);
   panel.display.clear();
   if let Some(point) = panel.touch.read_point() {
       ...
   }
   ```

### 5.3 Example Specification
```xml
# peripherals
  <spi id="spi2" sclk="8" mosi="7" />
  <i2c id="i2c0" sda="4" scl="5" />

# devices
  <!-- Single physical shield binding cleanly across both buses -->
  <ili9341_cst816s id="screen" 
                   display_bus="$spi2" cs="9" dc="3" reset="2"
                   touch_bus="$i2c0"   touch_irq="1" />
```

---

## 6. Synthesis & Execution Priority Matrix

| Change | Core Subsystems | Primary Value | Complexity | Target Release |
|---|---|---|---|---|
| **1. Silicon Resource Matrix** | `espforge-elements`, `ResourceGraph`, `schema` | Unlocks ADCs, PWM/LEDC, RMT (WS2812), TWAI/CAN; prevents ADC2/Wi-Fi silicon lockup crashes. | Medium | 0.9.0 |
| **2. Multi-Transport Network Engine** | `espforge-runtime`, `espforge-elements`, `services` | Unlocks wired Ethernet (W5500/EMAC) and Cellular LTE modems (PPP over UART). | High | 0.10.0 |
| **3. Declarative Fine-Grained Logging** | `espforge-emit`, `espforge-runtime`, `Cargo.toml` | Eliminates serial monitor flooding from Wi-Fi/TCP; strips 30–70 KB of dead format strings from Flash. | Low-Medium | 0.9.0 |
| **4. Power Domains & Bus Recovery** | `espforge-runtime`, `hw/i2c.rs`, `devices` | Prevents bootloops on unplugged/unpowered sensors; adds deep-sleep battery operations. | Medium | 0.10.0 |
| **5. Multi-Bus Composition** | `espforge-schema`, `espforge-elements`, `codegen` | Models real-world hardware shields (SPI Display + I2C Touch) under a single coherent device. | Medium-High | 0.11.0 |

---

## 7. Migration & Non-Breaking Evolution

All five changes maintain backward compatibility:
1. **Existing Specifications Remain Valid:** If a project omits `<logger>`, it retains the default logging behavior. If a device has no `power_pin`, it boots on default rails.
2. **Unified Macros Unchanged:** `device!(ctx, name)` continues to serve as the single, universal access macro across all blocking and Embassy targets.
3. **4-Section Grammar Preserved:** All changes fit within the standard four sections (`# espforge`, `# peripherals`, `# devices`, `# services`), requiring zero syntactic regressions.
