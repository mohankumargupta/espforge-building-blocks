# 004 — Basic Output & Switch (The Blink Test)

**Status:** ✅ possible today
**ESPHome lego:** `output: platform: gpio` + `switch: platform: output`

## ESPHome

```yaml
output:
  - platform: gpio
    pin: GPIO2
    id: onboard_led

switch:
  - platform: output
    name: "Onboard LED"
    output: onboard_led
```

Two layers: a GPIO output (HAL pin mapping) and a switch entity (state, a
name, and a dashboard action) driving it.

## espforge

`gpio-switch.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: gpio, pin: GPIO2` | `<gpio id="gpio2" pin="2" direction="output" />` in `# peripherals` |
| a second output/switch pair for control | collapses to `<led id="onboard_led" gpio="$gpio2" />`: the recipe checks the pin's declared direction |
| `switch.name` | none: espforge has no entity names. The `id` is the handle, and actions are Rust in `app.rs` |
| dashboard toggle | `led.on()`, `led.off()`, `led.toggle()` |

The two ESPHome layers are one device here: espforge's `<led>` recipe owns both
the pin claim and the on/off behaviour. `active_low="true"` covers the inverted
wiring case.

### Notes

- Using `pin="2"` instead of `gpio="$gpio2"` skips the declaration and lets the
  LED claim the pin itself; the explicit `<gpio>` is what makes the direction
  check possible.
