# 049 — Reentrant Scripts & Asynchronous Flow Control

**Status:** 🟡 partly possible — the sequence is a real state machine over a real button and relay; there is no `<script>` element
**ESPHome lego:** `script:` with `mode: restart`

## ESPHome

```yaml
script:
  - id: sequence_alert
    mode: restart
    then:
      - switch.turn_on: alert_relay
      - delay: 500ms
      - switch.turn_off: alert_relay
      - wait_until:
          binary_sensor.is_off: physical_button
      - logger.log: "Alert cleared and button released"
```

A script is a compiled coroutine. `mode: restart` means a second trigger
cancels the run in flight and starts a new one; `delay` yields to the scheduler
instead of blocking; `wait_until` parks the coroutine until a condition holds.

## espforge

`scripts.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `script: - id:` | none: no script element; the sequence is a `Stage` enum |
| `mode: restart` | `GENERATION` is bumped on every trigger; a run that sees a newer value abandons itself |
| `switch.turn_on` / `turn_off` | `relay.on()` / `relay.off()` on an `<led>` |
| `delay: 500ms` | the stage machine advances on elapsed time, never blocking |
| `wait_until: binary_sensor.is_off` | the `WaitRelease` stage polls `button.is_pressed()` |
| `logger.log` | `ctx.logger.info` |
| `mode: queued` / `parallel` | not modelled: only the restart semantics are here |

The important property is that no stage blocks: `forever` returns every tick, so
the button can trigger a restart mid-sequence. That is the whole difference
between a script and a function that sleeps.

### What is missing

A `<script>` element with execution modes, and a condition/action vocabulary
for `wait_until`. Block 009's triggers and this block's flow control are the
two halves of the same missing action language.
