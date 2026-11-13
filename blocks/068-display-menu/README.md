# 068 — Interactive Display Menu Navigation Engine

**Status:** 🟡 partly possible — the SSD1306 framebuffer is real; the menu tree is in `app.rs`
**ESPHome lego:** `display_menu:`

## ESPHome

```yaml
display_menu:
  id: main_menu
  display_id: main_display
  items:
    - type: command
      title: "Reboot System"
      on_select:
        - button.press: restart_btn
    - type: custom
      title: "Backlight On"
      on_select:
        - switch.turn_on: screen_backlight

binary_sensor:
  - platform: gpio
    pin: GPIO4
    name: "Menu Next"
    on_press:
      - display_menu.next: main_menu
```

A menu is a small state machine over a tree of items: a cursor, a redraw on
every move, and an action when the current item is selected. Hierarchical
menus add enter/back.

## espforge

`display-menu.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `display_id: main_display` | `<ssd1306 id="oled" bus="$i2c0" address="0x3C" size="128x64" />`, which is real |
| `items: type: command, title:` | `ITEMS` in `app.rs`, a slice of titles |
| `display_menu.next / prev / enter` | `cursor` and `Redraw` in `app.rs` |
| `on_select: button.press` | `ctx.ota.reboot()` for reboot (block 099's handle) |
| `on_select: switch.turn_on` | `backlight.on()` on an `<led>` |
| `binary_sensor: on_press` | two `<button>` inputs sampled for edges |
| `id:` | none |

The display driver, the buttons and the redraw loop are all real, so the menu
engine is ordinary Rust and the block is 🟡 rather than ❌. What the element
would add is the declarative tree, the cursor handling common to every menu,
and the enter/back plumbing.

### What is missing

A `<display_menu>` element with items, actions and navigation actions, so a
menu is data instead of a hand-written state machine.
