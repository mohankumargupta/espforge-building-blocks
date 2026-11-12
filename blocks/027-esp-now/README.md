# 027 — ESP-NOW Peer-to-Peer Messaging

**Status:** ❌ not possible yet
**ESPHome lego:** `esp_now:`

## ESPHome

```yaml
esp_now:

binary_sensor:
  - platform: gpio
    pin: GPIO3
    on_press:
      then:
        - esp_now.send:
            address: "11:22:33:44:55:66"
            message: "DOORBELL"
```

ESP-NOW sends 802.11 vendor frames straight to a peer's MAC address: no access
point association, no DHCP, no router hop. A button press becomes a small
payload on another device's radio.

## espforge

`esp-now.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp_now:` | proposed `<esp_now id="peer" wifi="$radio" peer="11:22:33:44:55:66" />` service |
| `esp_now.send: address/message` | proposed `service!(ctx, peer).send(&PEER, b"DOORBELL")` |
| `binary_sensor: platform: gpio, on_press` | `<button id="doorbell" pin="3" pull_up="true" />` and an edge check in `forever` |
| send/delivery callbacks | none |
| `name:` | none; the `id` is the handle |

The input side is real: a pulled-up button and a debounce. The transport is the
proposal. ESP-NOW does not join a network, but espforge's only radio element is
`<wifi>` and it exists to feed `<wifi_station>`, so there is no "radio up but
unassociated" mode to send from.

### What is missing

The ESP-NOW transport, peer registration, send and receive callbacks, and a
payload size limit. It also needs a way to start the Wi-Fi radio without
associating, which is why the proposed `<esp_now>` takes the `<wifi>` reference
directly instead of a `<wifi_station>`.
