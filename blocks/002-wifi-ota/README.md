# 002 — Wi-Fi & OTA Updates

**Status:** 🟡 partly possible — Wi-Fi and `<ota>` A/B updates work; there is no upload endpoint and no `password`
**ESPHome lego:** `wifi:` + `ota:`

## ESPHome

```yaml
wifi:
  ssid: "MyNetwork"
  password: "MyPassword"

ota:
  password: "admin"
```

Associate with an access point, get an address over DHCP, and serve an OTA
endpoint so later flashes go over the network instead of USB.

## espforge

`wifi-ota.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `wifi.ssid` / `wifi.password` | `<wifi_station ssid="..." password="..." />` |
| the radio implied by `wifi:` | `<wifi id="radio" />` in `# peripherals`, referenced with `wifi="$radio"` |
| `wifi:` (implies networking in ESPHome) | `<runtime type="embassy" />` — esp-hal Wi-Fi needs the async scheduler |
| `ota:` | `<ota />` in `# espforge` — two-slot partitions, rollback and `ctx.ota` (`flash_size` and `rollback` tune it) |
| `ota.password` | none: there is no built-in upload endpoint, so nothing to authenticate |

`<ota />` is project-wide, not a service: it owns the A/B partition table and
the `FLASH` peripheral, and the generated `Context` gains an `ota` handle.
`src/app.rs` waits for the association and DHCP, logs the address, confirms the
slot, then streams `http://192.168.4.1:8000/blink.bin` through `<http_client>`
into `ctx.ota` — `begin(size, crc32)`, `write(chunk)`, `progress()`, `finish()`,
`reboot()`. `UPDATE_URL`, `IMAGE_SIZE` and `IMAGE_CRC32` in `app.rs` name the
image; with `IMAGE_SIZE` at 0 the block only joins Wi-Fi. The transport is the
app because the block has no server side.

With `rollback="true"` (the default), a freshly updated image must call
`ctx.ota.mark_app_valid()` once it is healthy, or the bootloader reverts it on
the next reset.

### What is missing

ESPHome's `ota:` is a server the device runs; espforge ships only the updater.
No upload endpoint, no `password`, no mDNS advertisement, and no progress report
back to the client, so `app.rs` has to fetch its own image.
