# 005 — Binary Sensor with Debounce Filters

**Status:** ✅ possible today, with the filter in `app.rs`
**ESPHome lego:** `binary_sensor: platform: gpio` + `filters: delayed_on`

## ESPHome

```yaml
binary_sensor:
  - platform: gpio
    pin:
      number: GPIO4
      mode: INPUT_PULLUP
      inverted: true
    name: "Physical Button"
    filters:
      - delayed_on: 50ms
```

An input pin with an internal pull-up, active-low logic, and a filter pipeline
that only accepts a press that stays pressed for 50 ms.

## espforge

`binary-sensor.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin.number: GPIO4` + `mode: INPUT_PULLUP` | `<gpio id="gpio4" pin="4" direction="input" />` + `<button pull_up="true" />` |
| `inverted: true` | no attribute: `Button::is_pressed()` already reads a pulled-up line as active-low |
| `filters: delayed_on: 50ms` | no filter pipeline: the sampling loop in `app.rs` implements it |
| `name:` | none; use the `id` |

espforge's split is hardware in the project file, timing in Rust. The app samples
every 5 ms and only reports a press that survived 50 ms, then waits for the
release so one press is one event — the same edge behaviour `delayed_on` gives
ESPHome.

### Notes

- `filters:` is a mini-language (delayed_on/off, throttle, debounce, lambda).
  If espforge ever grows it, this block is the test case: the elements would
  have to be flat, e.g. `delayed_on="50ms"` on the `<button>`.
