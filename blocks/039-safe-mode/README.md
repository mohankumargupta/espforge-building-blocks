# 039 — Boot Loop Detection & Safe Mode Recovery

**Status:** ❌ not possible yet
**ESPHome lego:** `safe_mode:` + `ota: safe_mode:`

## ESPHome

```yaml
safe_mode:
  boot_attempts: 10
  reboot_timeout: 5min

ota:
  safe_mode: true
```

A boot counter survives resets in RTC memory. If the firmware panics or fails to
reach a healthy state ten times in a row, the next boot skips the normal
application, starts a fallback access point, and waits for an OTA upload instead
of rebooting forever.

## espforge

`safe-mode.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `safe_mode: boot_attempts` | proposed `<safe_mode id="guard" boot_attempts="10" reboot_timeout="5min" />` |
| RTC-memory boot counter | none: no persistent counter across resets |
| `reboot_timeout` | none: no boot watchdog timestamp |
| fallback AP | none: block 012 has no AP mode or DNS responder |
| `ota: safe_mode` | none: block 002 has no OTA service at all |
| logging the decision | `ctx.logger` is the only signal today |
| `name:` | none |

The app can log a boot count only if `guard` exists; there is nowhere to store
it that survives a reset. A `static` in RAM is reinitialised on every boot, so
it would always read zero.

### What is missing

An RTC-memory counter with an increment-at-boot and clear-on-healthy policy, a
boot deadline, an AP/OTA fallback, and a reset path. This block depends on
blocks 002 (OTA) and 012 (captive portal) before it can mean anything.
