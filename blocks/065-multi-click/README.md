# 065 — Complex Multi-Click Pattern Discrimination Engine

**Status:** ✅ possible today, with the timing in `app.rs`
**ESPHome lego:** `binary_sensor: on_multi_click`

## ESPHome

```yaml
binary_sensor:
  - platform: gpio
    pin:
      number: GPIO4
      mode: INPUT_PULLUP
      inverted: true
    name: "Multi-Function Button"
    on_multi_click:
      - timing:
          - ON for at most 350ms
          - OFF for at most 350ms
          - ON for at most 350ms
          - OFF for at least 200ms
        then:
          - logger.log: "Double-click triggered"
      - timing:
          - ON for at least 1000ms
        then:
          - logger.log: "Long-press hold triggered"
```

The timing list is a pattern language: each entry bounds one edge, and only a
complete match fires. A double-click is two short presses with a short gap; a
long press is a single edge that has lasted a second.

## espforge

`multi-click.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin: number / mode: INPUT_PULLUP` + `inverted` | `<button id="multifunction_button" pin="4" pull_up="true" />` |
| `on_multi_click:` | none: the discriminator is a `forever` state machine |
| `ON for at most 350ms` | `SHORT_MS` bounds the press |
| `OFF for at most 350ms` | `GAP_MS` bounds the gap between clicks |
| `OFF for at least 200ms` | the trailing quiet window that decides the count |
| `ON for at least 1000ms` | `LONG_MS` fires while still held, not after release |
| `name:` | none; the pattern is logged |

The important detail is that the long press must not wait for the release, or
the user holds the button for a second *and then* has to let go to see any
effect. Sampling at 10 ms is the resolution of every bound in the table.

### Notes

- `filters: delayed_on` from block 005 and this discriminator compose: the
  state machine here also absorbs contact bounce, because a bounce is shorter
  than `SHORT_MS`.
- The YAML's first pattern is really "two presses, then quiet": the trailing
  `OFF for at least 200ms` is what makes it a double-click rather than the
  start of a triple.
