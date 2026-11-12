# 002 — Wi-Fi & OTA Updates

**Status:** 🟡 partly possible — Wi-Fi builds today, `<ota>` does not exist yet
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
| `ota.password` | proposed `<ota id="ota" password="admin" />` service |

`src/app.rs` waits for the association and DHCP, then logs the address. OTA has
no application code: like ESPHome, it would be a runtime service.

### What is missing

`<ota>` is the ideal API, not an implemented element. Delete that one line and
the Wi-Fi half builds today. The rest of the OTA design questions (which flash
partition, what authenticates the upload, how it reports progress) are exactly
what this block should pin down before it is implemented.
