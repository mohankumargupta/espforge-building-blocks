# 086 — I²C Character LCD Matrix (HD44780 + PCF8574 Backpack)

**Status:** 🟡 partly possible — the I²C bus reaches the backpack; the HD44780 4-bit protocol is in `app.rs`
**ESPHome lego:** `display: platform: lcd_pcf8574`

## ESPHome

```yaml
display:
  - platform: lcd_pcf8574
    dimensions: 20x4
    address: 0x27
    i2c_id: i2c0
    lambda: |-
      it.print(0, 0, "SYSTEM ONLINE");
      it.printf(0, 2, "Temp: %.1f C", id(room_temp).state);
```

The backpacks everyone uses turn the LCD's parallel bus into I²C: one PCF8574
byte carries four data lines, RS, and the enable strobe. A nibble is presented,
EN pulses high then low, and the LCD latches it on the falling edge; two
nibbles make a byte.

## espforge

`lcd-pcf8574.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0`, `address: 0x27` | `<i2c id="i2c0" sda="4" scl="5" />`, `ADDRESS` |
| `dimensions: 20x4` | `COLUMNS`/`ROWS` and the 4-bit init sequence |
| RS / EN / D4–D7 | bit positions in the expander byte |
| backlight | PCF8574 bit 3, always on here |
| `it.print` / `it.printf` | a small writer in `app.rs`; formatting is Rust |
| `platform: lcd_pcf8574` | none: no driver |
| `lambda:` | none: the string is built by hand |

The nibble protocol is compact enough to live in the app, which is why this is
🟡 — the same judgement as block 035's PN532. The missing driver would add the
font (actually a ROM table in the LCD), the scroll window and `printf`.

### What is missing

A `<lcd_pcf8574>` display driver with dimensions, address and backlight, plus
the shared text/framebuffer layer. Block 020's SSD1306 is the pattern.
