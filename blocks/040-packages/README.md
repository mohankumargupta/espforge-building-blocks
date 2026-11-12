# 040 — Modular Packages & Configuration Composition

**Status:** ❌ not possible yet
**ESPHome lego:** `packages:`

## ESPHome

```yaml
packages:
  core_sys: !include common/system.yaml
  wifi_base: !include common/wifi.yaml
  sensors:
    url: https://github.com/my-org/esphome-templates
    file: bme280_node.yaml
    refresh: 1d

esphome:
  name: composed-office-node
```

Packages are a build-time merge: the compiler loads the listed YAML files,
recursively resolves their own includes, merges their keys into the root
configuration, and lets the root override what a package supplied. A remote
package is fetched and cached before the merge.

## espforge

`packages.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `packages: !include` | proposed `<include file="common/system.espf.txt" />` in `# espforge` |
| remote `url:` / `file:` / `refresh:` | none: no build-time fetch |
| root-key override | none: no merge stage or precedence rules |
| `esphome: name:` | `<project name="composed_office_node" />` |
| duplicated `id` detection across files | none: ids are unique only within one file today |
| `substitutions` | none (block 010) |
| `src/app.rs` | unchanged: composition happens before code generation |

`include` is not an element with a runtime meaning, so the app is just the
composed node's behaviour — blink the status LED. The missing piece is entirely
in the compiler: today one project file is parsed and one `app.rs` is copied.

### What is missing

An include/merge pass before schema validation, with deterministic override
rules, cross-file duplicate-id checks, and a remote-package fetch with caching.
Without it there is no composition to compile, which is why this block adds no
new runtime element.
