# 084 — Multi-Zone Sprinkler & Irrigation Valve Controller

**Status:** 🟡 partly possible — the three valve relays are real; the scheduler is in `app.rs`
**ESPHome lego:** `sprinkler:`

## ESPHome

```yaml
sprinkler:
  - id: garden_irrigation
    main_switch: "Master Water Valve"
    auto_advance_switch: "Auto Cycle"
    valves:
      - valve_switch: zone1_valve
        run_duration: 15min
      - valve_switch: zone2_valve
        run_duration: 20min
      - valve_switch: zone3_valve
        run_duration: 10min

switch:
  - platform: gpio
    pin: GPIO6
    id: zone1_valve
    name: "Lawn Sprinklers"
  - platform: gpio
    pin: GPIO7
    id: zone2_valve
    name: "Flower Bed Drip"
  - platform: gpio
    pin: GPIO10
    id: zone3_valve
    name: "Vegetable Patch Drip"
```

Three valves, one at a time: opening two at once drops pressure everywhere. The
sprinkler entity owns the interlock, the per-zone runtime, auto-advance from
zone to zone, and a master switch that shuts everything off.

## espforge

`irrigation.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `switch: platform: gpio` ×3 | `<led id="zone1_valve" pin="6" />` ×3 (valves are relays) |
| `main_switch` | a fourth `<led>` master valve, held open while any zone runs |
| `valves: run_duration:` | `ZONES: [("zone1", 15 min), ...]` |
| `auto_advance_switch` | `AUTO_ADVANCE` in `app.rs` |
| valve interlock | only one zone is ever `on()`; the others are cleared first |
| `sprinkler:` entity | none: a scheduler state machine |
| rain-sensor lockout | proposed `<button id="rain_sensor" .../>`; a real input |
| `name:` | none |

The relays and the sequencing are ordinary Rust, so this is 🟡 in the same way
as block 022's cover: the entity, its switches and the countdowns are missing,
not the control logic. The interlock is worth having in the app either way —
it is the difference between watering the garden and bursting a hose.

### What is missing

A `<sprinkler>` element with zones, run durations, a master switch, an
auto-advance switch, per-zone countdowns and a rain-sensor lockout input.
