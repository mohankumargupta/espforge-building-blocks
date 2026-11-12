# 031 — Infrared Remote Carrier Modulation & Protocol Decoding

**Status:** ❌ not possible yet
**ESPHome lego:** `remote_transmitter:` + `remote_receiver:`

## ESPHome

```yaml
remote_transmitter:
  pin: GPIO3
  carrier_duty_percent: 50%

remote_receiver:
  pin: GPIO4
  tolerance: 25%

binary_sensor:
  - platform: remote_receiver
    name: "IR Remote Power Key"
    nec:
      address: 0x00FF
      command: 0x45BA
```

Sending IR means modulating a 38 kHz carrier with a mark/space pattern; NEC
encodes a 9 ms leader, 32 bits (address, ~address, command, ~command) and a
stop bit. Receiving means sampling an edge-timed demodulator output and
measuring each mark and space within a tolerance.

## espforge

`ir-remote.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `remote_transmitter: pin` | proposed `<ir_transmitter id="ir_tx" pin="3" carrier="38kHz" duty="50" />` |
| `remote_receiver: pin, tolerance` | proposed `<ir_receiver id="ir_rx" pin="4" tolerance="25" />` |
| `nec: address` / `command` | proposed `nec()` returning `(address, command)` |
| `binary_sensor: platform: remote_receiver` | none: the app compares the frame |
| `name:` | none; the ids are the handles |
| RMT carrier generation | none: there is no RMT element, and software timing cannot hold 38 kHz |
| edge capture | none: no input capture or interrupt |

The app would poll `nec()` once per 20 ms and compare against the 0x00FF/0x45BA
pair; both the carrier and the receiver are proposals.

### What is missing

An RMT channel for carrier generation and edge capture, a mark/space decoder
with a tolerance window, and NEC/Samsung protocol tables. Software PWM is not a
substitute here: block 021's busy-wait loop already drifts by microseconds,
which is most of a 560 µs NEC bit.
