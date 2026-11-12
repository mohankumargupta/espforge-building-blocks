# 022 — Motorized Covers & Time-Based State Machines

**Status:** 🟡 partly possible — relays and transit timing run in `app.rs`; there is no cover entity or command transport
**ESPHome lego:** `cover: platform: time_based`

## ESPHome

```yaml
cover:
  - platform: time_based
    name: "Living Room Blinds"
    open_action:
      - switch.turn_on: motor_open_relay
    open_duration: 15s
    close_action:
      - switch.turn_on: motor_close_relay
    close_duration: 15s
    stop_action:
      - switch.turn_off: motor_open_relay
      - switch.turn_off: motor_close_relay
```

A cover with no position sensor infers its position from time: run the motor
for a known transit time and count how much of it has elapsed. The state machine
also interlocks the two relays, so open and close can never be energised
together.

## espforge

`cover-time-based.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `cover: platform: time_based` | none: no cover entity and no open/close/stop commands |
| `open_action` / `close_action` | `<led id="motor_open" pin="4" />` / `<led id="motor_close" pin="5" />` |
| `open_duration` / `close_duration` | `TRANSIT_TICKS` at 100 ms per tick (15 s each way) |
| `stop_action` | both relays off |
| position estimate | `POSITION` in 0..=150 ticks, logged as a percentage |
| local controls | three `<button>`s (open, close, stop) standing in for the missing transport |
| interlock | `close_relay.off()` before `open_relay.on()`, and vice versa |
| `name:` | none; the ids are the handles |

The buttons are edge-detected with a `LAST_BUTTONS` mask, so a held button
starts one move rather than restarting it every tick. Position is estimated,
never measured: stopping halfway and later closing takes the same 15 s from the
estimated position, exactly as ESPHome's time-based cover does.

### What is missing

The cover entity and a command path to reach it (block 003's API, block 013's
web server or block 014's MQTT), endstop inputs, and a `cover` action
vocabulary. The motor interlock and transit timer themselves are plain Rust and
already work.
