# 095 — DMX512 Stage Lighting Serial Universe Master

**Status:** ❌ not possible yet
**ESPHome lego:** no `dmx512:` element; this is the roadmap's extension

## ESPHome

```yaml
uart:
  id: dmx_uart
  tx_pin: GPIO21
  baud_rate: 250000
  stop_bits: 2

dmx512:
  id: stage_dmx
  uart_id: dmx_uart

output:
  - platform: dmx512
    dmx512_id: stage_dmx
    channel: 1
    id: spotlight_ch1
```

DMX512 is 250 kbaud with two stop bits over RS-485, and the frame boundary is
electrical, not a byte: a BREAK (line low for at least 88 µs), then
mark-after-break (high for at least 8 µs), then a start code and 512 channel
bytes. Fixtures use the BREAK to find channel 1.

## espforge

`dmx512.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `uart: baud_rate: 250000` | `<uart id="uart1" uart="UART1" tx="0" rx="1" baud_rate="250000" stop_bits="2" />` — real |
| `tx_pin: GPIO21` | GPIO20/21 are UART0, the console, so the bus moves to GPIO0/1 |
| `dmx512:` universe | proposed `<dmx512 id="stage_dmx" uart="$uart1" />` service |
| `output: platform: dmx512, channel: 1` | proposed `<dmx_output id="spotlight" universe="$stage_dmx" channel="1" />` |
| BREAK / MAB | none: the UART cannot assert a line break of a specified length |
| the 512-byte frame | a `[u8; 513]` in `app.rs`, writeable today |
| `id:` | none |

The baud rate, the stop bits and the byte stream are all expressible; the one
thing that makes a DMX frame a DMX frame is missing. Many receivers will
ignore a frame with no BREAK, which is why this is ❌ rather than 🟡.

### What is missing

A UART with an explicit break-and-mark-after-break primitive at microsecond
precision, plus a universe service that owns the channel buffer and an output
element that binds a channel to a level.
