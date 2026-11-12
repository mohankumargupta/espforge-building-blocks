# 038 — Active BLE Client & GATT Characteristic Polling

**Status:** ❌ not possible yet
**ESPHome lego:** `ble_client:` + `sensor: platform: ble_client`

## ESPHome

```yaml
esp32_ble_tracker:

ble_client:
  - mac_address: "DC:23:4F:91:0A:BC"
    id: ble_sensor_node

sensor:
  - platform: ble_client
    ble_client_id: ble_sensor_node
    name: "Remote Soil Moisture"
    service_uuid: "181A"
    characteristic_uuid: "2A6E"
    update_interval: 60s
```

This is the central role, not the scanner of block 026: connect to a known
peripheral, negotiate an MTU, discover the primary service, find a
characteristic, then read it or subscribe to notifications.

## espforge

`ble-client.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp32_ble_tracker:` | proposed `<ble id="radio" />` peripheral |
| `ble_client: mac_address, id` | proposed `<ble_client id="soil_node" radio="$radio" mac="DC:23:4F:91:0A:BC" />` |
| `service_uuid` / `characteristic_uuid` | proposed `read_characteristic(0x181A, 0x2A6E)` |
| GATT discovery / MTU | none: no GATT client |
| notifications | none: no subscription or callback |
| `sensor: platform: ble_client` | none: no entity to publish the value |
| `update_interval: 60s` | `delay_ms(60_000)` |
| `name:` | none |

The app's shape is a blocking read every minute; everything below it — link
establishment, service discovery, characteristic handles and value decoding —
is missing.

### What is missing

A BLE central stack with connection management, GATT client discovery, MTU
negotiation and notification handling. Block 026 needs the observer role; this
block needs the connection role, and the two share the missing controller.
