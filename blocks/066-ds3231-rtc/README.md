# 066 — Battery-Backed I²C Real-Time Clock (DS3231)

**Status:** 🟡 partly possible — the I²C bus and the register layout are real; the driver is in `app.rs`
**ESPHome lego:** `time: platform: ds3231` + `sensor: platform: ds3231`

## ESPHome

```yaml
time:
  - platform: ds3231
    id: ds3231_rtc
    i2c_id: i2c0
    address: 0x68

sensor:
  - platform: ds3231
    temperature:
      name: "RTC Crystal Temperature"
```

The DS3231 keeps time on a coin cell and reports it as BCD bytes at registers
0x00–0x06 (seconds, minutes, hours, weekday, day, month, year). Register 0x11
is the die temperature, which is how the part compensates its own crystal
against drift.

## espforge

`ds3231-rtc.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2c_id: i2c0`, `address: 0x68` | `<i2c id="i2c0" sda="4" scl="5" />`, `ADDRESS` |
| `time: platform: ds3231` | none: no time source element; the registers are read directly |
| `temperature` sensor | signed byte at 0x11, quarter-degree steps |
| setting the clock | nobody sets it here: a battery-backed part should keep time on its own |
| BCD conversion | `from_bcd` in `app.rs` |
| no network needed | this block runs with no Wi-Fi at all |

This is the offline counterpart of block 011: an RTC is what keeps a schedule
running when SNTP cannot be reached. The bytes are real, the conversion is
real, and the only missing part is the time-source element that would make
`clock.now()` work without a network.

### What is missing

A `<ds3231>` time source that registers with the runtime clock, so `now()`
reads the RTC when SNTP is unavailable — plus a way to set it once from NTP.
