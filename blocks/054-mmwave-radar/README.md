# 054 — 24 GHz mmWave Radar & Micro-Motion Stream Parsing

**Status:** 🟡 partly possible — the UART is real; the LD2410 frame parser is in `app.rs`
**ESPHome lego:** `uart:` + `ld2410:`

## ESPHome

```yaml
uart:
  id: radar_uart
  tx_pin: GPIO21
  rx_pin: GPIO20
  baud_rate: 256000

ld2410:
  uart_id: radar_uart

binary_sensor:
  - platform: ld2410
    has_target:
      name: "Target Detected"
    has_moving_target:
      name: "Moving Target"
    has_still_target:
      name: "Still Target (Micro-Motion)"
```

The LD2410 streams two kinds of frame: engineering configuration frames (type
0x01) and periodic target reports (type 0x02) at 10 Hz. Each frame is
`F4 F3 F2 F1 | length | data | F8 F7 F6 F5`; the target report carries a state
byte, then moving/still distance, energy and detection distance.

## espforge

`mmwave-radar.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `uart: tx_pin / rx_pin` | `<uart id="uart1" uart="UART1" tx="0" rx="1" baud_rate="256000" />` — GPIO20/21 are UART0, the console |
| `baud_rate: 256000` | `baud_rate="256000"` |
| `ld2410:` | proposed `<ld2410 id="radar" uart="$uart1" />` |
| `has_target` / `has_moving_target` / `has_still_target` | one state byte: bit 0 target, bit 1 moving, bit 2 still |
| engineering mode and its config commands | none: only the periodic report is parsed here |
| `name:` | none; the readings are logged |

The parser is a real one: it hunts the magic header, validates the declared
length and tail, then decodes the report. What it does not have is a driver
that owns the UART, so the frame loop is the app's.

### What is missing

A `<ld2410>` device that owns the UART, exposes the three binary sensors and
the distances, and can send the engineering-mode enable/parameter commands.
