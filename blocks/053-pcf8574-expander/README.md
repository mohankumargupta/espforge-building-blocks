# 053 — I²C GPIO Port Expander & Hardware Interrupts

**Status:** 🟡 partly possible — the I²C bus reaches the expander; the port state is in `app.rs`
**ESPHome lego:** `pcf8574:` + `switch: platform: gpio` on an expander pin

## ESPHome

```yaml
pcf8574:
  - id: pcf8574_hub
    address: 0x20
    i2c_id: i2c0

switch:
  - platform: gpio
    name: "Expander Relay 1"
    pin:
      pcf8574: pcf8574_hub
      number: 0
      mode: OUTPUT
      inverted: true
```

The PCF8574 has no registers: the current port state *is* the output latch.
Writing one byte sets all eight pins; reading one byte returns the pin levels
(and the latch for pins driven low). An INT line pulses when any input changes.

## espforge

`pcf8574-expander.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pcf8574: id / address / i2c_id` | proposed `<pcf8574 id="expander" bus="$i2c0" address="0x20" int="3" />` |
| `pin: pcf8574: / number: 0` | `set_pin(...)` on the proposed device |
| `mode: OUTPUT` | read-modify-write of the latch byte |
| `inverted: true` | `!state` when the bit is written |
| the bus itself | `<i2c id="i2c0" sda="4" scl="5" />` is real |
| input change interrupts | none: no INT edge handling |

The latch arithmetic is honest and small — which is exactly why this block is
🟡 rather than ❌ — but without a `<pcf8574>` element the app has to own the
whole port image, and two parts sharing the byte would overwrite each other.

### What is missing

A `<pcf8574>` device that owns the latch, exposes eight pins as outputs and
inputs, and turns the INT line into real change events.
