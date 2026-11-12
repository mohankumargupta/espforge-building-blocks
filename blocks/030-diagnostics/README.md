# 030 — System Health Diagnostics & Hardware Watchdog

**Status:** ❌ not possible yet
**ESPHome lego:** `sensor: platform: uptime` + `wifi_signal`, `button: platform: restart`

## ESPHome

```yaml
sensor:
  - platform: uptime
    name: "System Uptime"
    update_interval: 60s
  - platform: wifi_signal
    name: "Wi-Fi RSSI"
    update_interval: 60s

button:
  - platform: restart
    name: "Restart Node"
```

A healthy node reports how long it has been up, how strong its Wi-Fi link is,
and how much heap is left; a restart button lets the dashboard reboot it without
physical access. ESPHome also feeds a task watchdog so a hung loop resets the
chip.

## espforge

`diagnostics.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `sensor: platform: uptime` | proposed `service!(ctx, health).uptime_secs()` |
| `sensor: platform: wifi_signal` | proposed `rssi_dbm()` on the same service |
| heap / leak tracking | proposed `free_heap_bytes()`; the Wi-Fi build already allocates a heap |
| `button: platform: restart` | proposed `<restart_button id="restart_node" />`; `health.restart()` |
| watchdog (WDT) | none: no feed API and no timeout policy |
| logging | `ctx.logger` is the only health signal today |

The Wi-Fi connection task logs disconnects and retries, and `wait_connected()`
gives the app a link event, but neither exposes a signal strength. A `static`
counter would measure loop iterations, not uptime, so the app does not pretend
that is the feature.

### What is missing

A diagnostics service that can read the radio's link metrics and the
allocator's free/heap-high-water numbers, a restart action that goes through
esp-hal's reset path, and a task watchdog with an explicit feed policy. Blocks
013 and 014 would then be the natural way to publish the readings.
