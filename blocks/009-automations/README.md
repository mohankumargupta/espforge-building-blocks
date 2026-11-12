# 009 — Local Automations (Triggers & Actions)

**Status:** ✅ possible today, as Rust in `forever` (not declarative)
**ESPHome lego:** `on_press:` + `then:` actions

## ESPHome

```yaml
binary_sensor:
  - platform: gpio
    pin: GPIO4
    name: "Wall Switch"
    on_press:
      then:
        - light.toggle: desk_lamp
```

The trigger compiler: an input event runs an action list in firmware, with no
server involved. ESPHome looks the action target up by entity id.

## espforge

`automations.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `on_press:` | `if button.is_pressed()` inside `forever` |
| `then: light.toggle: desk_lamp` | `lamp.toggle()` on the device handle |
| action ids resolved by name | no name resolution: devices are fields on `ctx`, reached with `device!(ctx, id)` |
| — | `<led>` is the target because `<light>` is still block 008 |

The app also debounces and waits for release, so one physical press is exactly
one toggle. This is the honest translation of an ESPHome automation: espforge
has no trigger/action language, and its answer is ordinary Rust on the same
context the generated code uses.

### Notes

- If a trigger vocabulary is ever added, this block is its acceptance test: a
  flat grammar would need something like `on_press="desk_lamp.toggle"` on the
  button, and compiling it would have to produce the same loop shown here.
- The `<led>`/`<button>` split also shows why espforge keeps actions in the app:
  no cross-element registry exists to resolve `light.toggle` by name.
