# 042 — I²S Audio Output & Network Media Player

**Status:** ❌ not possible yet
**ESPHome lego:** `i2s_audio:` + `media_player: platform: i2s_audio`

## ESPHome

```yaml
i2s_audio:
  i2s_lrclk_pin: GPIO4
  i2s_bclk_pin: GPIO5

media_player:
  - platform: i2s_audio
    name: "Alert Speaker"
    i2s_dout_pin: GPIO7
    dac_type: external
    mode: mono
```

The transmit half of block 041: the master clocks samples out to an external
DAC, and the player paces itself against the DMA FIFO so the buffer neither
under-runs (clicks) nor over-runs (dropped chunks).

## espforge

`i2s-speaker.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2s_audio:` (shareable bus) | proposed `<i2s id="spk_bus" lrclk="4" bclk="5" dout="7" rate="16000Hz" />` peripheral |
| `dac_type: external` | the only mode: the C3 has no internal DAC |
| `mode: mono` | proposed `channels="mono"` |
| `media_player: platform: i2s_audio` | proposed `<media_player id="speaker" i2s="$spk_bus" volume="0.8" />` |
| volume scaling, sample-rate conversion | none: no mixer or resampler |
| network playback | none: there is no audio source service |

Both audio elements are shape only. The network side is not the blocker —
`<http_client>` is real — the missing piece is an I²S transmitter with a DMA
FIFO for the download to feed.

### What is missing

The `<i2s>` TX peripheral, a `<media_player>` entity that queues PCM, and a
DMA-backed source. Once 041 exists this block is mostly the same code with the
direction flipped.
