# 014 — MQTT Transport & Auto-Discovery

**Status:** ❌ not possible yet
**ESPHome lego:** `mqtt:`

## ESPHome

```yaml
mqtt:
  broker: 192.168.1.100
  port: 1883
  discovery: true
  discovery_prefix: homeassistant
  topic_prefix: test-node
```

The node publishes its state to a broker, subscribes to command topics, and —
with discovery on — announces each entity so Home Assistant creates it without
manual configuration.

## espforge

`mqtt.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `mqtt.broker` / `port` | proposed `<mqtt id="broker" network="$home" broker="192.168.1.100" port="1883" />` service |
| `discovery` / `discovery_prefix` | proposed; needs an entity registry to build Home Assistant's discovery JSON |
| `topic_prefix` | proposed |
| pub/sub, retain, QoS | none: no broker client, and no socket API is exposed to the app |
| availability / LWT | none |
| `mqtt:` implies networking | `<wifi_station>` provides `network`, as for the other clients |

MQTT is a client service, so half its shape is already proven: `network=` names
the provider and the runtime spawns the connection task, like `http_client`
does. The unproven half is the entity side — topics and discovery payloads have
to be derived from `# devices`.

### What is missing

A broker client (TCP plus the MQTT state machine), retained-topic handling, and
the discovery serializer. Until the entity registry exists, the most honest
fallback is a hand-written topic per device in `app.rs`, which is what the
`mqtt` service would remove.
