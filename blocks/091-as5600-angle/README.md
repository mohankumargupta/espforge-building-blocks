# 091 — Contactless Magnetic Rotary Angle Encoder (AS5600)

**Status:** 🟡 partly possible — the I²C bus and the angle registers are real
**ESPHome lego:** `sensor: platform: as5600`

## ESPHome

```yaml
sensor:
  - platform: as5600
    name: "Weather Vane Direction"
    i2c_id: i2c0
    address: 0x36
    update_interval: 200ms
```

A diametrically magnetised magnet spins over the AS5600's Hall pair, and the
part reports the angle as 12 bits: 4096 counts per revolution, about 0.09° per
count. Two registers at 0x0E give the raw angle; 0x0C/0x0D give a scaled angle
after the part's own zero position and range are programmed.

## espforge

`as5600-angle.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0`, `address: 0x36` | `<i2c id="i2c0" sda="4" scl="5" frequency="400kHz" />`, `ADDRESS` |
| raw angle | registers 0x0E/0x0F, 12 bits in the low half of each |
| scaled angle | registers 0x0C/0x0D after a zero position is burned in |
| magnetic field status | register 0x1B: MD/ML/MH flags; a weak magnet reads garbage |
| `platform: as5600` | none: no driver |
| `update_interval: 200ms` | `delay_ms(200)` |
| `name:` | none; degrees are logged |

Two registers and a 12-bit mask is about as simple as an I²C part gets, so the
app is a nearly complete driver. The part's programmable zero is the one piece
worth a driver: writing it burns OTP and needs care.

### What is missing

An `<as5600>` sensor with the raw/scaled angle choice, the magnet-status flag
and the zero-position programming action.
