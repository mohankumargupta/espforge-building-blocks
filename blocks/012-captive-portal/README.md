# 012 — Captive Portal & Wi-Fi Provisioning

**Status:** ❌ not possible yet
**ESPHome lego:** `wifi: ap:` + `captive_portal:`

## ESPHome

```yaml
wifi:
  ssid: "PrimarySSID"
  password: "PrimaryPassword"
  ap:
    ssid: "FallBack-Hotspot"
    password: "provisioningpass"

captive_portal:
```

Two fallback layers: when the station cannot associate, the radio beacons its
own access point, and a DNS responder sends every lookup back to the device so
a phone opens the provisioning page. Credentials written there are stored in
flash and used on the next boot.

## espforge

`captive-portal.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `wifi.ssid` / `wifi.password` | `<wifi_station ssid="..." password="..." />` (block 002) |
| `wifi.ap.ssid` / `wifi.ap.password` | proposed `<wifi_ap id="fallback" wifi="$radio" ssid="..." password="..." />` service |
| `captive_portal:` | proposed `<captive_portal id="portal" network="$fallback" />` service |
| fallback policy | none: `<wifi_station>` only associates; it has no failure mode |
| credentials in flash | none: credentials are attributes compiled into the firmware |

The ideal app uses a `try_connect` / `start` / `wait_for_credentials` /
`reconnect` sequence; none of those methods exist. `<wifi_ap>` would
`provides "network"` like `<wifi_station>` does, so the portal can resolve it by
capability. The portal itself needs the HTTP server of block 013 plus a DNS
responder.

### What is missing

- AP mode: the `<wifi>` peripheral only describes the radio, and the only
  service on it is station mode.
- A credential store (block 019) and a reboot-after-provisioning policy.
- Two servers — DNS and HTTP — plus the redirect behaviour that makes a phone
  show the provisioning page.
