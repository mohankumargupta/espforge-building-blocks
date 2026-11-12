# 011 — Network Time & Scheduled Automations

**Status:** 🟡 partly possible — `<ntp>` syncs and keeps a local clock; no timezones or `on_time` triggers
**ESPHome lego:** `time: platform: sntp` + `on_time:`

## ESPHome

```yaml
time:
  - platform: sntp
    id: sntp_time
    timezone: "UTC"
    on_time:
      - seconds: 0
        minutes: 0
        hours: 8
        then:
          - switch.turn_on: daily_switch
```

Three layers: an SNTP source, a local clock advanced by the monotonic timer,
and a trigger that compares wall-clock fields. `on_time` fires when the
hour/minute/second match.

## espforge

`network-time.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `time: platform: sntp, id` | `<ntp id="clock" network="$home" />` service (`$home` is a `<wifi_station>`) |
| `timezone: "UTC"` | none: the clock is Unix seconds, read back as UTC |
| the internal clock | `sync().await` anchors once; `now()` adds elapsed monotonic time with no network I/O |
| `on_time: hours/minutes/seconds` | Rust in `forever`: compare `now().seconds % 86_400` |
| `then: switch.turn_on: daily_switch` | `daily_switch.on()` on the `<led>` handle |

The app checks once a minute and fires at 08:00 UTC. A `static AtomicU32`
remembers the Unix day that already fired, because the generated main calls
`forever` repeatedly; without it the trigger would re-fire on every check
inside the 08:00 minute.

### What is missing

- Timezone offsets and DST: both are arithmetic over the Unix timestamp, but a
  DST table has no home in the project file today.
- `on_time` as a trigger: there is no trigger/action vocabulary (block 009).
- The "fired today" state is RAM-only, so a reboot inside 08:00 fires again;
  block 019 is where that state would persist.
