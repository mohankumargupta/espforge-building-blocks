# 063 — Serial MP3 Media Player (DFPlayer Mini)

**Status:** 🟡 partly possible — the UART is real; the command framing is in `app.rs`
**ESPHome lego:** `dfplayer:` + `button: platform: template` calling it

## ESPHome

```yaml
uart:
  id: dfplayer_uart
  tx_pin: GPIO6
  rx_pin: GPIO7
  baud_rate: 9600

dfplayer:
  uart_id: dfplayer_uart
  busy_pin: GPIO10
  id: sound_fx

button:
  - platform: template
    name: "Play Doorbell Chime"
    on_press:
      then:
        - dfplayer.play_folder:
            folder: 1
            file: 1
```

The DFPlayer Mini takes 10-byte packets: `7E FF 06 <command> 00 <param hi>
<param lo> <checksum hi> <checksum lo> EF`, where the checksum is the two's
complement of the bytes between the start and the checksum. BUSY is low while
audio plays.

## espforge

`dfplayer.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `uart: tx_pin / rx_pin` | `<uart id="uart1" uart="UART1" tx="6" rx="7" baud_rate="9600" />` |
| `dfplayer: uart_id` | proposed `<dfplayer id="sound_fx" uart="$uart1" busy="10" volume="20" />` |
| `busy_pin: GPIO10` | `<button id="player_busy" pin="10" />` — active low |
| `play_folder: folder 1, file 1` | command 0x0F with `(folder << 8) | file` |
| `volume:` | command 0x06 with the level in the low byte |
| `button: platform: template` | `<button id="chime_button" pin="4" pull_up="true" />` |
| `name:` | none; the file is chosen in `app.rs` |

Frame building and the checksum are real, and the busy line is a real input, so
the state that "a chime is still playing" is observable. The driver is missing,
which is why the packet lives here.

### What is missing

A `<dfplayer>` device that owns the UART, tracks playback state from BUSY, and
exposes volume, folder and track commands as services.
