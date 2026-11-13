# 076 — 7-Segment 4-Digit Serial Display (TM1637)

**Status:** 🟡 partly possible — two GPIOs and the bit-banged protocol are real; the driver is in `app.rs`
**ESPHome lego:** `display: platform: tm1637`

## ESPHome

```yaml
display:
  - platform: tm1637
    clk_pin: GPIO2
    dio_pin: GPIO3
    intensity: 4
    lambda: |-
      it.printf("12:34");
```

The TM1637 is a two-wire part with its own protocol: a start condition (CLK
high, DIO falls), then bits clocked in LSB-first with DIO driven on the falling
edge, an acknowledge bit, and a stop condition (CLK high, DIO rises). Commands
set the display on/off, the brightness and the address to write.

## espforge

`tm1637-display.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `clk_pin` / `dio_pin` | `<led id="tm_clk" pin="2" />` and `<led id="tm_dio" pin="3" />` |
| reading the acknowledge bit | not possible: `<led>` cannot read back, so ACKs are ignored |
| `intensity: 4` | command 0x88 \| 4 |
| `it.printf("12:34")` | `SEGMENTS` lookup plus per-digit addresses 0xC0.. |
| start/stop conditions | `start()` / `stop()` in `app.rs` |
| `platform: tm1637` | none: no driver |
| `name:` | none; the digits are the display |

Everything here is real except the driver, which is why the block is 🟡 — but
with a caveat the ESPF file cannot express: a TM1637 acknowledge happens on a
bidirectional DIO line, and two `<led>`s are write-only. Ignoring the ACK works
because the protocol is one-way in practice.

### What is missing

A `<tm1637>` display driver with a bidirectional DIO pin, the segment font and
brightness as an attribute, and `printf`-style drawing.
