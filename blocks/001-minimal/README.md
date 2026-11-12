# 001 — Minimal Boot & Serial Logger

**Status:** ✅ possible today
**ESPHome lego:** the core `esphome:` block + `logger:`

## ESPHome

```yaml
esphome:
  name: test-node
  platform: ESP32
  board: esp32-c3-devkitm-1

logger:
  level: DEBUG
```

The smallest ESPHome firmware: name the node, pick a board, and turn on serial
logging at DEBUG. There is no hardware and no logic yet.

## espforge

`minimal.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `esphome.name` | `<project name="minimal" />` (lowercased into the crate name) |
| `platform: ESP32` + `board: esp32-c3-devkitm-1` | `<chip type="esp32c3" />` — espforge targets a chip, not a board |
| `logger.level: DEBUG` | no element: the generated `main()` installs esp-println's logger, and the level comes from `ESP_LOG` (e.g. `ESP_LOG=debug`) in the build environment |

`src/app.rs` is the whole behaviour: log once in `setup`, idle in `forever`.
The generated entry point already created the logger, delay and (empty) devices
before calling it, so there is no runtime wiring in the app.

### Notes

- `<console type="uart" />` is the default; `jtag-serial` selects the chip's
  built-in USB serial instead.
- The four sections are mandatory even when empty (`# peripherals`,
  `# devices`, `# services`), which is why the file spells them out.
