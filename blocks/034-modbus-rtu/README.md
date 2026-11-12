# 034 — Modbus RTU & RS485 Half-Duplex Arbitration

**Status:** 🟡 partly possible — UART, the DE pin and the framing code work; there is no Modbus controller
**ESPHome lego:** `modbus:` + `sensor: platform: modbus_controller`

## ESPHome

```yaml
uart:
  id: modbus_uart
  tx_pin: GPIO21
  rx_pin: GPIO20
  baud_rate: 9600

modbus:
  id: modbus_bus
  uart_id: modbus_uart
  flow_control_pin: GPIO10

sensor:
  - platform: modbus_controller
    modbus_id: modbus_bus
    name: "Active Power"
    address: 0x01
    register_type: holding
    register_address: 0x0012
    value_type: FP32
```

Modbus RTU is a binary request/response protocol on a shared two-wire bus. The
master asserts the RS485 driver, sends `slave, function, start, count, CRC16`,
releases the driver, and waits for a response that must arrive after the 3.5
character silent interval so every node recognises a new frame.

## espforge

`modbus-rtu.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `uart: tx_pin, rx_pin, baud_rate` | `<uart id="uart1" tx="0" rx="1" baud_rate="9600" />` |
| `modbus: flow_control_pin` | `<led id="rs485_de" pin="10" />`: `on()` while transmitting |
| `uart_id` | the `<uart id="uart1">` handle, `device!(ctx, uart1)` |
| `modbus_controller` request/CRC | none: `app.rs` builds the 8-byte request and checks CRC16 |
| `register_type: holding`, `register_address: 0x0012` | constants in `app.rs` |
| `value_type: FP32` | `f32::from_be_bytes` over the two response registers |
| `update_interval` | `delay_ms(10_000)` |
| `name:` | none; the reading is logged |

The roadmap's GPIO20/21 are UART0 on the C3 — the console pins a devkit logs
and flashes over — so this block moves the bus to GPIO0/1 on UART1 and leaves
the console alone. The request, the CRC and the direction toggle are real; only
the protocol layer is hand-written, exactly as block 016 does for the MHZ19.

### What is missing

A Modbus controller that owns register maps, function codes, byte order and
response timeouts, plus a real inter-frame timer for t3.5. Here the turnaround
is a fixed 10 ms poll and a 100 ms deadline, which is adequate at 9600 baud but
is not the same as measuring 3.5 character times.
