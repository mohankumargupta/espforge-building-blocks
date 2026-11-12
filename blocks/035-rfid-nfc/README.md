# 035 — Contactless RFID/NFC Tag Authentication

**Status:** 🟡 partly possible — a raw I²C bus reaches the PN532; there is no driver or tag entity
**ESPHome lego:** `pn532_i2c:` + `binary_sensor: platform: pn532`

## ESPHome

```yaml
pn532_i2c:
  id: nfc_module
  i2c_id: i2c0

binary_sensor:
  - platform: pn532
    pn532_id: nfc_module
    uid: "4B-12-8A-3F"
    name: "Access Card Granted"
    on_press:
      - switch.turn_on: door_strike
```

A PN532 is a 13.56 MHz reader behind I²C (or SPI/UART). The host sends
`InListPassiveTarget`, the reader runs the ISO 14443A anticollision loop and
answers with the tag's UID; the firmware compares that UID with an allow-list.

## espforge

`rfid-nfc.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pn532_i2c: i2c_id` | `<i2c id="i2c0" sda="4" scl="5" />`; no device binds, so `device!(ctx, i2c0)` is a raw `I2cBus` |
| `pn532_i2c:` address | `PN532_ADDRESS = 0x24` in `app.rs` |
| anticollision / UID read | `app.rs` writes the PN532 command frame and parses the response |
| `uid: "4B-12-8A-3F"` | `ALLOWED_UID` compared in `app.rs` |
| `binary_sensor: platform: pn532` | none: the app logs granted/denied |
| `on_press: switch.turn_on` | `<led id="door_strike" pin="10" />` energised for 3 s |
| `name:` | none |

This is the same split as blocks 016 and 034: the bus is real, the part's
protocol is hand-written. Unlike those, the PN532 frame has a length byte, a
length checksum, a data checksum and a status byte in front of every I²C read,
so the code here is a sketch of a driver, not a complete one.

### What is missing

A PN532 driver (frame assembly, ACK handling, firmware-version check, IRQ pin)
and a tag entity with a UID allow-list. Anticollision itself happens inside the
reader, so the missing work is host-side framing, not RF.
