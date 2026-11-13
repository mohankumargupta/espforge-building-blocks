# 048 — Home Assistant Bluetooth Proxy Forwarding

**Status:** ❌ not possible yet
**ESPHome lego:** `esp32_ble_tracker:` + `bluetooth_proxy:`

## ESPHome

```yaml
bluetooth_proxy:
  active: true

esp32_ble_tracker:
```

The radio becomes a remote HCI endpoint for Home Assistant: passive scan
results stream out as advertisements, and with `active: true` Home Assistant
can also open connections *through* the proxy to GATT peripherals it cannot
reach directly.

## espforge

`ble-proxy.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp32_ble_tracker:` | proposed `<ble id="radio" />` peripheral (shared with 026, 038, 059, 069) |
| `bluetooth_proxy:` | proposed `<bluetooth_proxy id="proxy" ble="$radio" active="true" />` service |
| streaming advertisements to Home Assistant | none: no advertisement parser and no API transport |
| proxied GATT connections | none: no central role (block 038) |
| connection slots | proposed `connections="3"`, a static budget of the controller |

The radio is the same missing peripheral as the rest of the BLE blocks; what is
unique here is the consumer. A proxy is not an entity — it is a service that
forwards raw HCI frames over the native API, so it depends on block 003 as
much as on the controller.

### What is missing

The BLE controller, a passive advertisement parser, connection proxying with a
bounded slot pool, and the native API transport to carry the frames.
