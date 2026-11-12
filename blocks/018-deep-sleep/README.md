# 018 — Deep Sleep & Wakeup Triggers

**Status:** ❌ not possible yet
**ESPHome lego:** `deep_sleep:`

## ESPHome

```yaml
deep_sleep:
  id: deep_sleep_control
  run_duration: 15s
  sleep_duration: 10min
  wakeup_pin:
    number: GPIO3
    mode: INVERT_WAKEUP
```

Deep sleep is how a battery node gets a long life: run for a few seconds, then
cut the CPU and most of the chip, leaving the RTC domain to count down or watch
a pin. Waking is a reset, so the firmware starts again from the top.

## espforge

`deep-sleep.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `deep_sleep.run_duration` / `sleep_duration` | proposed `<deep_sleep id="deep_sleep_control" run_duration="15s" sleep_duration="10min" />` service |
| `wakeup_pin.number` / `mode` | proposed `wakeup_pin="3"` and `wakeup_mode="invert"` |
| RTC timer alarm / GPIO wake | none: no runtime handle and no sleep API |
| `id:` | proposed handle; the app would call `sleep_now()` |
| where the app ends | `setup` does the work and sleeps; `forever` becomes unreachable |

`wakeup_pin` is a literal pin rather than a `$gpio` reference because services
never refer to peripherals — the same split `<led>` uses when it takes
`pin="2"`. There is also no duration value kind yet: `run_duration` would be the
first attribute needing one.

### What is missing

The element, the RTC-timer and GPIO wake configuration, and a policy for what
happens to radios and buses before sleep (ESPHome powers Wi-Fi down
explicitly). RAM is lost across sleep, so anything that must survive belongs to
block 019.
