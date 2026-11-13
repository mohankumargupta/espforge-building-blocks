# 094 — Motorized Gate & Dual-Endstop Interlock Safety Cover

**Status:** 🟡 partly possible — the relays, limit switches and the travel timer are real; there is no cover entity
**ESPHome lego:** `cover: platform: endstop`

## ESPHome

```yaml
cover:
  - platform: endstop
    name: "Automated Driveway Gate"
    open_action:
      - switch.turn_on: gate_open_relay
    open_duration: 25s
    open_endstop: gate_open_limit
    close_action:
      - switch.turn_on: gate_close_relay
    close_duration: 25s
    close_endstop: gate_close_limit
    stop_action:
      - switch.turn_off: gate_open_relay
      - switch.turn_off: gate_close_relay
```

A time-based cover with physical limits: the relays run the motor, the
endstops prove the gate arrived, and `open_duration` is the timeout that turns
the motor off if a limit switch never closes. Both relays on at once is a short
across the motor, so the interlock is not optional.

## espforge

`gate-cover.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `open_action` / `close_action` | `<led id="gate_open_relay" pin="4" />`, `<led id="gate_close_relay" pin="5" />` |
| `open_endstop` / `close_endstop` | `<button id="gate_open_limit" pin="2" pull_up="true" />` (inverted in the YAML, so active low) |
| `open_duration: 25s` | `TRAVEL_MS = 25_000`: the timeout, not a delay |
| `stop_action` | `stop()` clears both relays |
| `cover: platform: endstop` | none: no cover entity or position estimate |
| `name:` | none |

The three safety properties are implemented directly: relays are mutually
exclusive, the travel timer aborts a stuck move, and the endstop stops the
motor before the timeout. What is missing is the entity that would let Home
Assistant ask for a position.

### What is missing

A `<cover>` element with open/close/stop and position reporting, which block
022 also waits on, plus the declarative endstop bindings.
