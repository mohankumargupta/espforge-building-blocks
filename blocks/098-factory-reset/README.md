# 098 — Multi-Stage Factory Reset & NVS Wipe Recovery

**Status:** 🟡 partly possible — the hold timing and feedback are real; the flash wipe is not
**ESPHome lego:** `binary_sensor: on_click` with two duration bands

## ESPHome

```yaml
binary_sensor:
  - platform: gpio
    pin:
      number: GPIO9
      mode: INPUT_PULLUP
      inverted: true
    name: "Physical Recovery Button"
    on_click:
      - min_length: 5s
        max_length: 14s
        then:
          - logger.log: "Soft reboot initiated"
          - button.press: restart_btn
      - min_length: 15s
        then:
          - logger.log: "Factory reset triggered: wiping NVS flash"
          - lambda: |-
              nvs_flash_erase();
              esp_restart();
```

One button, two meanings by hold length: five seconds restarts, fifteen wipes
the saved credentials and restarts. The LED is the feedback that tells you
which band you are in before you commit to it.

## espforge

`factory-reset.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `pin: number: GPIO9`, `inverted: true` | `<button id="recovery_button" pin="9" pull_up="true" />` |
| `on_click` + `min_length` / `max_length` | a hold timer in `app.rs`, two thresholds |
| `button.press: restart_btn` | `ctx.ota.reboot()` when a build has `<ota />`; no plain reset otherwise |
| the LED feedback | `status_led` on an `<led>` |
| `nvs_flash_erase()` | none: no NVS API, so there is nothing to erase |
| `esp_restart()` | `ctx.ota.reboot()` exists, but only with `<ota />` |
| `name:` | none |

The multi-band hold is a timing problem and works; the second band's action is
the missing part. This block and block 019 share the gap: without a
`<globals>`/NVS layer there are no saved credentials for a factory reset to
remove.

### What is missing

An NVS namespace API (block 019) and a reset path that does not require an
`<ota />` partition table.
