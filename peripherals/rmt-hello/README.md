# RMT hello

The esp-hal `rmt-hello` example as an espforge project, with the UART console
showing the text the waveform carried.

The chip transmits `hello\n` on one raw RMT channel and its own UART1 receives
it, so the test proves the waveform, not just that a transmit finished:

- `<rmt id="rmt0" />` claims the chip's RMT block at its default 80 MHz.
- `<rmt_tx id="uart_tx" bus="$rmt0" pin="4" channel="0" clk_divider="80"
  idle_level="high" />` configures channel 0 the way the example configures
  its TX channel: 115200 8N1 timing, 9 ticks a bit at 1 µs a tick.
- `<uart id="uart1" uart="UART1" tx="6" rx="5" baud_rate="115200" />` is the
  receive side. `src/app.rs` prints what the port read.
- `parts/uartwire` is the wire between them, as a virtual part. The emulator
  has no way to feed an RMT channel into a UART, so the part watches the RMT
  pin as runs of edges (`reads_edges`), decodes the 8N1 bytes by sampling the
  timeline at each bit's centre, and puts them on the UART1 line (`serial`).
  `board.json` wires it: the RMT pin on GPIO 4, its TX on the chip's RX GPIO 5
  and its RX on the chip's TX GPIO 6.

| File | What it is |
|---|---|
| `rmt-hello.espf.txt` | the espforge project: `<rmt>`, `<rmt_tx>`, `<uart>` |
| `src/app.rs` | the pulse builder, the transmit, and the UART read |
| `board.json` | the virtual wire, on GPIO 4 and the UART1 line |
| `parts/uartwire/` | the part: edge-run decoding into UART bytes |

## Run it

```sh
just rmt-hello-check        # schema and resource graph
just rmt-hello-part-test    # the part's framing decode, on the host
just rmt-hello-part         # build the part -> uartwire.wasm
just rmt-hello-image        # compile -> build/rmt-hello/rmt-hello.bin (+ .elf)
just rmt-hello-run          # boot it under esp-emu for 20s
just rmt-hello-test         # gate on the console printing the received text
```

`just rmt-hello-run` prints the firmware console, including
`TEST: uart "hello"`; the emulator's own log goes to
`build/rmt-hello/emu.log`.

## Timing

| Quantity | Value |
|---|---|
| RMT source clock | 80 MHz (default for `<rmt>` on esp32c3) |
| `clk_divider` | 80, so one tick is 1 µs |
| Bit length | 9 ticks (115200 baud is 8.68 µs) |
| Idle level | high, like an idle UART line |
| Byte | low start bit, 8 data bits least significant first, high stop bit |

An RMT receiver records where the line changes, so equal neighbouring bits
arrive merged: `h`'s first three data bits are one 27 µs low. The part decodes
from the timeline, not from the transmitted pulse pairs.

## Limits

- Transmit only: no RMT receive channel and no RMT decoding in the firmware.
  The waveform is decoded by the virtual part.
- The wire is a part, not a chip feature: attach it with
  `--cosim-board board.json`, as the recipes do.
- `build/` is disposable; the fixture itself stays.
