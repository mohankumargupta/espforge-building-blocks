# 019 — Global State & Non-Volatile Storage (NVS)

**Status:** ❌ not possible yet
**ESPHome lego:** `globals:` + `restore_value:`

## ESPHome

```yaml
globals:
  - id: total_cycles
    type: int
    restore_value: yes
    initial_value: '0'

switch:
  - platform: template
    name: "Increment Cycle"
    turn_on_action:
      - lambda: 'id(total_cycles) += 1;'
```

A typed variable any component can read, plus an option to keep it across
power cycles: `restore_value` writes it to a flash key-value store and loads it
again at boot.

## espforge

`globals-nvs.espf.txt` + `src/app.rs`.

| ESPHome | espforge |
|---|---|
| `globals: id/type/initial_value` | no declaration: a Rust `static` (`AtomicU32`) in `app.rs` |
| `restore_value: yes` | proposed `<nvs id="flash" />` service with `load_u32` / `store_u32` |
| `lambda: id(total_cycles) += 1` | `TOTAL_CYCLES.fetch_add(1, ...)` |
| dashboard action | a `<button>` press handled in `forever` (block 009) |
| NVS itself | none: no flash key-value API and no wear handling |

Keeping the variable in Rust is the natural translation: espforge has no
expression evaluator, so a global is a `static` the app reads. What it cannot do
is survive a reset, which is the half `<nvs>` would add.

### What is missing

An NVS service: namespaced keys, typed values, commit/flush semantics, and an
erase command. A `globals:` declaration would only add naming on top — the
block works as soon as NVS exists.
