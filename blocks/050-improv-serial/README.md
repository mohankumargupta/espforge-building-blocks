# 050 — Improv Serial Pairing & Web Installer Protocol

**Status:** ❌ not possible yet
**ESPHome lego:** `improv_serial:`

## ESPHome

```yaml
esp32:
  board: esp32-c3-devkitm-1
  framework:
    type: esp-idf

improv_serial:
```

Improv is the pairing protocol esp-web-tools speaks: the browser sends a framed
command over the same serial port it just flashed (`IMPROV` magic, a command
byte, length and checksum), the device scans for networks, reports them back,
accepts credentials and reboots onto the chosen AP.

## espforge

`improv-serial.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `improv_serial:` | proposed `<improv id="pairing" console="uart" />` service |
| `esp32: framework: esp-idf` | not needed: espforge is esp-hal based |
| `IMPROV` framing | none: no framer or RPC dispatcher |
| Wi-Fi scan + credentials | none: `<wifi_station>` takes an SSID and password at build time |
| the serial line itself | `<console type="uart" />` owns UART0, so logging and pairing would have to share it |
| pairing results | `ctx.logger` only |

The port is the interesting conflict: Improv needs exclusive access to the same
UART the logger writes to, and to be useful it must also survive without any
credentials in the image. espforge's `<wifi_station>` is a compile-time
credential, so this block is a provisioning story, not just a protocol.

### What is missing

A console with a shared-write mode, an Improv framer/RPC service, a Wi-Fi scan
API, and a credential store to persist the chosen network (block 012's
provisioning half).
