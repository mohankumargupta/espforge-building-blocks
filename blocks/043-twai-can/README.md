# 043 — TWAI / CAN Bus Controller & Identifier Filtering

**Status:** ❌ not possible yet
**ESPHome lego:** `canbus: platform: esp32_twai`

## ESPHome

```yaml
canbus:
  - platform: esp32_twai
    tx_pin: GPIO6
    rx_pin: GPIO7
    can_id: 0x100
    bit_rate: 500kbps
    on_frame:
      - can_id: 0x200
        then:
          - switch.turn_on: auxiliary_relay
```

TWAI is the C3's ISO 11898 controller: it serialises 11-bit (standard) and
29-bit (extended) frames, computes and checks the CRC in hardware, and applies
acceptance masks so only matching identifiers reach the CPU.

## espforge

`twai-can.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `platform: esp32_twai` | proposed `<twai id="can" tx="6" rx="7" bitrate="500kbps" />` peripheral |
| `can_id: 0x100` | proposed `can_id="0x100"` (the send identifier) |
| `bit_rate: 500kbps` | `bitrate="500kbps"`; the timing registers are derived from it |
| `on_frame: can_id: 0x200` | proposed `<can_listener id="aux_frames" can="$can" can_id="0x200" />` |
| `switch.turn_on` | `device!(ctx, auxiliary_relay).on()` on an `<led>` |
| mask filtering | none: no acceptance filter, so every frame reaches the app |

The relay and the frame handler are ordinary espforge; what is missing is the
controller. A masked filter belongs in the peripheral, because dropping frames
in software at 500 kbit/s already costs more than the bus delivers.

### What is missing

A `<twai>` peripheral (bit timing, acceptance mask, error counters) and a
listener element that decodes frames into app callbacks. Block 075's ranging
sensor and block 034's RS485 bus are the closest existing analogues.
