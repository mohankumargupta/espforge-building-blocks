# 097 — Custom BLE GATT Server Profile & Characteristics

**Status:** ❌ not possible yet
**ESPHome lego:** `esp32_ble_server:` + `ble_server:` services

## ESPHome

```yaml
esp32_ble_server:

ble_server:
  - id: custom_service
    uuid: "12345678-1234-5678-1234-56789abcdef0"
    characteristics:
      - uuid: "12345678-1234-5678-1234-56789abcdef1"
        value: "espforge-ready"
        permissions: [read, write]
        on_write:
          - logger.log: "Received custom BLE command payload"
```

A peripheral hosts a service table: a primary service with a 128-bit UUID, its
characteristics, each with its own UUID, value, permissions and descriptor
table. Clients discover the service, read the value, write a command, or
subscribe to notifications.

## espforge

`ble-gatt-server.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esp32_ble_server:` | proposed `<ble id="radio" role="peripheral" />` peripheral |
| `ble_server: id / uuid` | proposed `<gatt_service id="custom_service" ble="$radio" uuid="..." />` |
| `characteristics: uuid / value / permissions` | proposed `<characteristic uuid="..." value="espforge-ready" permissions="read,write" />` |
| `on_write:` | proposed an action hook; `ctx.logger` is real |
| notifications | none: no subscription table or notify path |
| MTU negotiation | none |
| `id:` | none |

The whole peripheral role is missing, which block 059 also waits on. Unlike a
beacon, a GATT server needs an attribute table built at compile time plus a
connection event loop.

### What is missing

A BLE controller in peripheral role, a compile-time GATT attribute table, MTU
negotiation, bonding, and notification scheduling.
