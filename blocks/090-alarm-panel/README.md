# 090 — Security Alarm Control Panel & Partition State Machine

**Status:** 🟡 partly possible — buttons, a siren relay and the state machine are real; there is no alarm entity
**ESPHome lego:** `alarm_control_panel: platform: template`

## ESPHome

```yaml
alarm_control_panel:
  - platform: template
    name: "Home Security"
    codes:
      - "1234"
    on_triggered:
      - switch.turn_on: siren_relay
    on_disarmed:
      - switch.turn_off: siren_relay
```

An alarm panel is a finite state machine: `disarmed`, `armed_home`, `armed_away`,
`pending` (exit delay running), `triggered`. Entry runs a delay before the siren
fires; disarming needs the code; and the siren must stop in every path that
leaves `triggered`.

## espforge

`alarm-panel.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `codes: ["1234"]` | `CODE_MS_TAPS`: the PIN is entered as button presses, not digits |
| `armed_home` / `armed_away` | one `<button>` each, with edges in `forever` |
| `triggered` | a PIR (`<button>`) while armed |
| `on_triggered: switch.turn_on` | `siren.on()` on an `<led>` |
| `on_disarmed: switch.turn_off` | `siren.off()` |
| entrance / exit delay | `EXIT_MS` / `ENTRY_MS` countdowns |
| `alarm_control_panel:` | none: no entity, no code validation API |
| `name:` | none |

The panel works as written: it arms, counts an exit delay, triggers on motion
and silences on the disarm button. What is missing is the entity — publishing
the state, accepting `alarm_control_panel.alarm_arm_away` from Home Assistant,
and a numeric keypad for the code (block 037 scans one).

### What is missing

An `<alarm_control_panel>` element with states, delays, code storage and
service calls, plus the keypad entity from block 037 for the PIN.
