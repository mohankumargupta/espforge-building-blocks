# 096 — Bi-Stable Latching Solenoid Pulse Driver

**Status:** ✅ possible today, with the pulse pair in `app.rs`
**ESPHome lego:** `switch: platform: template` with `delay` actions

## ESPHome

```yaml
switch:
  - platform: template
    name: "Latching Gas Valve"
    turn_on_action:
      - switch.turn_on: pulse_forward
      - delay: 75ms
      - switch.turn_off: pulse_forward
    turn_off_action:
      - switch.turn_on: pulse_reverse
      - delay: 75ms
      - switch.turn_off: pulse_reverse

output:
  - platform: gpio
    pin: GPIO0
    id: pulse_forward
  - platform: gpio
    pin: GPIO1
    id: pulse_reverse
```

A latching solenoid flips on a short pulse of one polarity and stays flipped
with no current. Holding the coil energised is what burns it out, so the pulse
duration is a hard limit, not a preference, and both coils must never be on at
once.

## espforge

`latching-valve.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `output: platform: gpio` ×2 | `<led id="pulse_forward" pin="0" />`, `<led id="pulse_reverse" pin="1" />` |
| `switch.turn_on / turn_off` of the coil | `coil.on()` / `coil.off()` |
| `delay: 75ms` | `PULSE_MS = 75` |
| `turn_on_action` / `turn_off_action` | `set_latched(true/false)` |
| the valve's latched state | `LATCHED` in `app.rs` |
| `switch: platform: template` entity | none: no switch entity |
| `name:` | none |

The two things that matter are both here: the pulse is measured against a
deadline rather than left on, and the opposing coil is released before the
pulse starts. That is the whole driver, so this block is ✅ in the sense of
block 004 — the entity is absent, the behaviour is not.

### Notes

- The pulse guards are asserts in spirit: `PULSE_MS` is the coil's rated
  maximum, and the code never energises both outputs.
- A capacitor across the coil supply (100 µF or so) is what makes a 75 ms pulse
  actually flip the valve rather than brown out the rail.
