# RMT hello

The esp-hal `rmt-hello` example as an espforge project, run by `esp-emu`.
`peripherals/rmt-hello-example.rs` is the original; the project here produces
the same waveform on GPIO 4:

- `<rmt id="rmt0" />` claims the chip's RMT block at its default 80 MHz.
- `<rmt_tx id="uart_tx" bus="$rmt0" pin="4" clk_divider="80" idle_level="high" />`
  configures channel 0 the way the example configures its TX channel.
- `src/app.rs` turns `hello\n` into 31 `PulseCode`s (start + 8 data + stop for
  each of six bytes, plus an end marker) and transmits them once a second.

No virtual part is attached: the target is a raw channel, not a device on a
bus. The emulator still runs the transmission, and the firmware logs
`TEST: rmt hello sent` when the blocking transmit has drained.

| File | What it is |
|---|---|
| `rmt-hello.espf.txt` | the espforge project: `<rmt>` + one `<rmt_tx>` |
| `src/app.rs` | the pulse builder and the one-second transmit loop |

## Run it

```sh
just rmt-hello-check   # schema and resource graph
just rmt-hello-image   # compile -> build/rmt-hello/rmt-hello.bin (+ .elf)
just rmt-hello-run     # boot it under esp-emu for 20s
just rmt-hello-test    # gate on one completed transmission
```

`just rmt-hello-run` prints the firmware console; the emulator's own log goes
to `build/rmt-hello/emu.log`.

## Timing

| Quantity | Value |
|---|---|
| RMT source clock | 80 MHz (default for `<rmt>` on esp32c3) |
| `clk_divider` | 80, so one tick is 1 µs |
| Bit length | 9 ticks (115200 baud is 8.68 µs) |
| Idle level | high, like an idle UART line |

## Limits

- Transmit only: no RMT receive channel and no decoding.
- The emulator has no listener on GPIO 4, so the test proves the channel
  drained, not the voltage on a wire. Add an `<rmt_rx>` plus
  `--rmt-loopback 0:2` to decode the frame in the emulator.
- `build/` is disposable; the fixture itself stays.
