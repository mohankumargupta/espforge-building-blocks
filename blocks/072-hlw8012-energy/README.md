# 072 — Dual-Mode Frequency Energy Monitor (HLW8012 / BL0937)

**Status:** ❌ not possible yet
**ESPHome lego:** `sensor: platform: hlw8012`

## ESPHome

```yaml
sensor:
  - platform: hlw8012
    sel_pin: GPIO2
    cf_pin: GPIO3
    cf1_pin: GPIO4
    voltage:
      name: "Mains Voltage"
    current:
      name: "Mains Current"
    power:
      name: "Active Power"
    change_mode_every: 8
    update_interval: 10s
```

The chip does not speak I²C or SPI. It emits a square wave on CF whose
frequency is proportional to active power, and a second wave on CF1 that is
voltage or current depending on the SEL pin. `change_mode_every` alternates
SEL so both are measured, and every reading is a pulse count over a window.

## espforge

`hlw8012-energy.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `cf_pin` / `cf1_pin` | `<button id="cf" pin="3" />` / `<button id="cf1" pin="4" />` as inputs |
| `sel_pin` | `<led id="sel" pin="2" />`, toggled every `change_mode_every` windows |
| frequency -> power | none: the pulse count needs a hardware counter |
| `change_mode_every: 8` | a window counter in `app.rs` |
| `update_interval: 10s` | `delay_ms(10_000)` |
| `platform: hlw8012` | none: no driver |
| `name:` | none; the derived values are logged |

Mains power can reach tens of kilowatts, which is a CF frequency in the
kilohertz: polling cannot count those edges, and a `delay_ns` loop would lose
most of them. This is the same missing primitive as block 017's pulse counter,
which is why the block is ❌ even though the pins are ordinary GPIO.

### What is missing

A hardware edge counter (PCNT) with a windowed read, which blocks 017, 081 and
085 all need, plus an `<hlw8012>` driver that owns the SEL alternation and the
calibration constants.
