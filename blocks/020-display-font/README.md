# 020 — Display Buffer & Font Graphics Engine

**Status:** 🟡 partly possible — `<ssd1306>` and bitmap fonts work; no TTF `font:` block or lambdas
**ESPHome lego:** `font:` + `display: platform: ssd1306_i2c`

## ESPHome

```yaml
font:
  - file: "fonts/Roboto-Medium.ttf"
    id: font_default
    size: 16

display:
  - platform: ssd1306_i2c
    model: "SSD1306 128x64"
    address: 0x3C
    i2c_id: i2c0
    lambda: |-
      it.printf(0, 0, id(font_default), "Node Online");
      it.strftime(0, 30, id(font_default), "%H:%M:%S", id(sntp_time).now());
```

A display block is three pieces: a bus, a driver with a framebuffer, and a font
that turns text into pixels. The lambda writes into the framebuffer; a redraw
pushes it to the panel.

## espforge

`display-font.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `font: file/id/size` | none: no font block and no filesystem. Bitmap fonts are compiled in (`FONT_6X10` from embedded-graphics) |
| `display: platform: ssd1306_i2c` | `<ssd1306 id="oled" bus="$i2c0" address="0x3C" size="128x64" />` |
| `i2c_id: i2c0` | `bus="$i2c0"` |
| `it.printf(...)` | `Text::new(...).draw(&mut *oled)` |
| `it.strftime(...)`, `id(sntp_time).now()` | a manual `hhmmss()` over `service!(ctx, clock).now()`; UTC only |
| redraw | `clear()`, draw, `flush().await`, once a second |
| `lambda:` | ordinary Rust in `app.rs` |

ESPHome's lambda is C++ over a generated `it`; espforge's answer is the driver's
`DrawTarget` API: the framebuffer is a real object the app draws into. An async
display is always deferred, so the first draw awaits `init()` — that is why the
app calls it even though the panel has a boot path.

### What is missing

- A `font:` block and a TTF rasterizer. With no filesystem, a usable version
  would convert the font to a bitmap at build time.
- `printf`/`strftime` helpers and timezone-aware time (block 011).
- Partial redraw: `flush()` currently sends the whole framebuffer.
- Colour text for the TFT: `<ili9341>` works, but styles are
  embedded-graphics types rather than YAML attributes.
