# 041 — I²S Digital Microphone & PCM Streaming

**Status:** ❌ not possible yet
**ESPHome lego:** `i2s_audio:` + `microphone: platform: i2s_audio`

## ESPHome

```yaml
i2s_audio:
  i2s_lrclk_pin: GPIO4
  i2s_bclk_pin: GPIO5

microphone:
  - platform: i2s_audio
    id: digital_mic
    i2s_din_pin: GPIO6
    adc_type: external
    pdm: false
```

The I²S master drives the bit clock and word select; the microphone shifts
16-bit samples back on DIN for every word select edge. DMA fills a ring buffer
while the CPU does something else, which is what makes continuous capture
possible without a sample being missed.

## espforge

`i2s-microphone.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `i2s_audio:` (shareable bus) | proposed `<i2s id="mic_bus" lrclk="4" bclk="5" rate="16000Hz" />` peripheral |
| `i2s_din_pin: GPIO6` | proposed `din="6"` on the peripheral |
| `microphone: platform: i2s_audio` | proposed `<microphone id="digital_mic" i2s="$mic_bus" channels="mono" />` |
| `adc_type: external`, `pdm: false` | the standard I²S mode: no attribute |
| DMA ring, 16-bit PCM frames | none: no I²S channel and no DMA claim |
| sample consumption | `read_samples(&mut pcm)` in `forever` is the intended handle |

Everything below the app is missing: the I²S peripheral, its DMA channel, and
the microphone entity. The DMA channel matters even once I²S exists — SPI and
I²S share the pool, so the compiler has to allocate it, not the codegen.

### What is missing

An `<i2s>` peripheral with a DMA ring, a `<microphone>`/`<media_player>` pair
that consumes it, and DMA-channel allocation in the `ResourceGraph`. Blocks 042
and 079 both need the same two pieces.
