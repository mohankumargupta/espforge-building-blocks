# 087 — Hardware Status LED & Runtime Life-Cycle Signaling

**Status:** ✅ possible today, with the cadence in `app.rs` (no `status_led:` element)
**ESPHome lego:** `status_led:`

## ESPHome

```yaml
status_led:
  pin:
    number: GPIO3
    inverted: false
```

The status LED is not an entity: it is wired into the firmware's state machine.
ESPHome blinks it differently for "connecting", "no Wi-Fi, fallback AP" and
"everything is fine", so a device with no serial console still tells you what it
is doing.

## espforge

`status-led.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin: number: GPIO3` | `<led id="status_led" pin="3" />` |
| `inverted: false` | no attribute: `<led>` is active high unless `active_low="true"` |
| the runtime hook | none: the blink pattern is a `forever` state machine |
| Wi-Fi state | `<wifi_station id="home" ...>` is real; `wait_connected` gives the edge |
| cadence states | `Cadence` in `app.rs`: Boot, Connecting, Online, Offline |
| `name:` | none |

The pattern has to be non-blocking to be useful: a device that blinks while
blocked in a connect loop cannot also serve requests. Each `forever` tick
advances the blink, so the cadence survives a slow association.

### Notes

- Fast blink (100 ms) while connecting, a slow double-blink while associated
  but without an address, and a steady LED once online: that is the common
  ESPHome vocabulary.
- `active_low="true"` on the `<led>` covers boards whose LED is wired to 3V3.
