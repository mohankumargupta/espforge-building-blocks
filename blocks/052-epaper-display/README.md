# 052 — SPI Electronic Paper Display & Partial Refresh

**Status:** ❌ not possible yet
**ESPHome lego:** `spi:` + `display: platform: waveshare_epaper`

## ESPHome

```yaml
spi:
  clk_pin: GPIO6
  mosi_pin: GPIO7

display:
  - platform: waveshare_epaper
    cs_pin: GPIO10
    dc_pin: GPIO2
    reset_pin: GPIO3
    busy_pin: GPIO4
    model: 2.90in
    full_update_every: 30
    lambda: |-
      it.print(10, 10, id(font_default), "Status: Running");
```

E-paper is not an LCD. The controller is initialised by a register sequence,
every refresh blocks until the BUSY line releases, and a full refresh flashes
the panel through its lookup-table waveform while a partial refresh only
touches the changed region — which is why ESPHome counts refreshes and does a
full one every 30.

## espforge

`epaper-display.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `spi:` bus | `<spi id="spi2" sclk="6" mosi="7" mode="0" />` is real; write-only, so no MISO |
| `cs_pin`, `dc_pin`, `reset_pin`, `busy_pin` | proposed `<epaper id="panel" bus="$spi2" cs="10" dc="2" reset="3" busy="4" />` |
| `model: 2.90in` | proposed `model="2.9in"`: resolution and LUT selection |
| `full_update_every: 30` | a refresh counter in `app.rs` |
| the BUSY poll | none: no display driver to poll it |
| `lambda:` drawing | none: no framebuffer or font for this controller |

The SPI bus reaches the panel today, but a display controller is a driver, not
framing: an init table of dozens of registers, two waveform LUTs and a
framebuffer. Unlike MAX6675 (block 025), that does not belong in `app.rs`.

### What is missing

An `<epaper>` display driver with an init sequence, BUSY handling, full and
partial refresh modes, and the shared framebuffer/font layer that block 020
already has for SSD1306.
