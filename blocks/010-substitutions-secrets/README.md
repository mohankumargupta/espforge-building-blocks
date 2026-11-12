# 010 — Substitutions & Secrets

**Status:** ❌ not possible yet (secrets only half work today)
**ESPHome lego:** `substitutions:` + `!secret`

## ESPHome

```yaml
substitutions:
  device_name: living-room-lamp

esphome:
  name: ${device_name}

wifi:
  ssid: !secret wifi_ssid
  password: !secret wifi_password
```

Two preprocessors: `${name}` is string interpolation performed before the
config is compiled, and `!secret` reads a value out of `secrets.yaml` so it
never lives in the checked-in file.

## espforge

`substitutions-secrets.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `substitutions:` | no section, no `${}` interpolation |
| `esphome.name: ${device_name}` | `<project name="living-room-lamp" />` — the literal name today |
| `!secret wifi_ssid` | none |
| `!secret wifi_password` | `<wifi_station password="env:WIFI_PASSWORD" />` reads the env var at build time (and warns that it is compiled into the firmware) |
| `wifi:` | `<wifi>` + `<wifi_station>` |

Only `wifi_station.password` accepts an env-var reference; `ssid` does not, so
even the secret half of this block cannot be expressed completely. The ideal
file shows the two syntaxes the grammar would need: a textual `${}` pass over
attribute values, and a `!secret`-style resolver for named values.

### What is missing

A preprocessor stage, and a decision about where secrets resolve: at parse time
(like ESPHome) or in the emitter (like `env:` today). The `env:` form is the
existing precedent, and it is a one-attribute special case that this block would
generalise.
