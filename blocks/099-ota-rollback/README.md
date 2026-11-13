# 099 — Firmware Self-Diagnostic & Automated OTA Rollback

**Status:** ✅ possible today — `<ota />` supplies the A/B slots and `ctx.ota.mark_app_valid()`
**ESPHome lego:** `ota: safe_mode:` and the ESP-IDF rollback guard

## ESPHome

```yaml
ota:
  safe_mode: true
  num_attempts: 5

interval:
  - interval: 60s
    then:
      - lambda: |-
          if (esphome::api::global_api_server->is_connected()) {
            esp_ota_mark_app_valid_cancel_rollback();
          }
```

A freshly booted firmware starts in a pending-verify state. If it proves it can
reach the network and its services before the bootloader's deadline, it is
confirmed; if it panics or resets first, the bootloader returns to the previous
slot. `num_attempts` is how many times that is allowed to happen before safe
mode.

## espforge

`ota-rollback.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp_ota_mark_app_valid_cancel_rollback()` | `ctx.ota.mark_app_valid()`, real |
| `ota: safe_mode` | `<ota rollback="true" />` is the default |
| A/B partitions (`ota_0`, `ota_1`) | generated `partitions.csv`, real |
| `api: is_connected()` as the health signal | `service!(ctx, home).wait_connected()` — the station is real |
| `num_attempts: 5` | not exposed: the bootloader's own retry count |
| `interval: 60s` self-check | a periodic diagnostics pass in `forever` |
| safe-mode AP | none: block 039 and block 012 |

This is the one recovery block that is ✅: the runtime already owns the updater,
so confirming the image is a call. The important design point is *what* is
checked before confirming — a firmware that marks itself valid before its
Wi-Fi works has locked itself out of future updates.

### Notes

- `mark_app_valid()` only means something with `rollback="true"`; with rollback
  off it is a no-op.
- The deadline matters: if the health check is slow, the bootloader may roll
  back before it finishes.
