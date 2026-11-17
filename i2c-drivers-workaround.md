# I²C drivers on esp-hal + esp-emu: the root cause and the local workaround

**Status as of 2026-11-18:** `just i2cscanner-test` **passes** — the scan finds the
emulator's built-in EEPROM at 0x50 and nothing else. That is the only I²C gate
that has been run green so far. Every other I²C fixture (SHT31 and friends)
still needs the same two-file overlay described in
[§5](#5-putting-the-workaround-in-a-new-i²c-project) before it can be tested at
all.

This document is the complete "why" behind that overlay, so that whoever tests
the next I²C driver does not rediscover it. Read it before concluding that a
driver, a part model, a `board.json` or the wiring is wrong.

| Thing | Version used for every measurement below |
|---|---|
| Chip | ESP32-C3 only (`--chip esp32c3`, target `riscv32imc-unknown-none-elf`) |
| `esp-emu` | 1.1.0 (`/home/pi/.local/bin/esp-emu`) |
| `esp-hal` | 1.2.2, from the vendored patched copy in `esp-hal/` |
| Rust | 1.95 (the generated `rust-version`) |

Everything runs through the root `justfile`; `AGENTS.md` forbids calling
`espforge` or `esp-emu` directly.

---

## 1. Symptoms

Firmware that talks to *anything* over I²C — the emulator's built-in 24Cxx
EEPROM at 0x50, an ESP-IDF example, a co-simulated part at 0x44 — behaves the
same way:

- every `write`, `read` and `write_read` returns
  `Err(AcknowledgeCheckFailed(Data))` when a device answers, and
  `Err(AcknowledgeCheckFailed(Unknown))` when nothing does;
- the espforge `i2c_scanner` example prints `TEST: i2c scanner begin` and
  `Scan complete`, with **no** `Found device at address …` lines;
- a driver's `new()`/`Devices::new` therefore fails, and the firmware logs
  `device `air`: initialization failed` before the application runs;
- the co-simulation log is innocent — the part *does* receive the transaction
  and ACK it:

  ```text
  co-simulation: part sensor (sht31) attached: ... answers I2C 0x44 on GPIO 6/5
  {"dir":"to_part","msg":{"BusRequest":{"op":{"I2c":{"addr":68,"write":[48,162],"read_len":0}}}}}
  {"dir":"from_part","msg":{"BusReply":{"ack":true, ...}}}}
  ```

Frequency (100 kHz vs 400 kHz), address, empty vs non-empty writes, and
"board vs no board" make no difference. The transaction dies in the **data
phase**, after the address has been ACKed.

`AcknowledgeCheckFailed(Data)` vs `…(Unknown)` is the split that names the
cause: `Unknown` is a real NACK (nothing at that address), `Data` is esp-hal
rejecting a transaction that the device accepted.

---

## 2. Root cause, in layers

There are three independent problems stacked on top of each other. Fixing any
one alone does nothing, which is why this looked like a dead end.

### 2.1 The emulator does not model `sr.resp_rec`

esp-hal's I²C master driver checks two things after a transfer:

```rust
if r.nack().bit_is_set() { … }                       // real NACK → kept, still works
…
if r.trans_complete().bit_is_set()
    && self.regs().sr().read().resp_rec().bit_is_clear()   // ← the culprit
{
    return Err(Error::AcknowledgeCheckFailed(
        AcknowledgeCheckFailedReason::Data,
    ));
}
```

`SR.RESP_REC` is a hardware bit the controller sets once it has received a byte
from the slave in a read phase. Implying nothing about the ACK; it is an
internal receive-status flag.

`esp-emu` 1.1.0 never sets it (there is no `resp_rec` model anywhere in the
emulator source, including the vendored tree in `esp-emulator/`). So every
transaction where `TRANS_COMPLETE` is set and `RESP_REC` is clear — i.e.
**every successful transaction** — is reported as a data-phase ACK failure.
Nothing reaches the firmware as `Ok`.

The NACK check above stays intact because of this: a bus with no device at
0x44 still fails with `Unknown`, and the scanner still cannot see phantom
devices. Only the false-positive data-phase rejection is removed.

### 2.2 esp-hal 1.2.2 has no way to switch that check off

`esp-hal` 1.2.2 ships the check unconditionally. Since there is no emulator
fix available (the emulator is a prebuilt binary and its source has no
`resp_rec` model to enable), the check has to be compiled out on the firmware
side. That is what the local patch does — behind a feature, so it cannot be
enabled by accident on hardware:

```toml
# esp-hal/Cargo.toml
[features]
# Local patch, not from esp-hal: esp-emu does not model `sr.resp_rec`, so the
# I2C master RESP_REC check rejects every transaction there. Never enable on
# hardware.
esp-emu = []
```

```rust
// esp-hal/src/i2c/master/low_level/mod.rs, in `check_errors()`
// `esp-emu` does not model `sr.resp_rec`, so this check rejects every
// transaction there. Local patch only: hardware must keep it.
#[cfg(not(feature = "esp-emu"))]
#[cfg(not(i2c_master_version = "1"))]
if r.trans_complete().bit_is_set() && self.regs().sr().read().resp_rec().bit_is_clear() {
    return Err(Error::AcknowledgeCheckFailed(
        AcknowledgeCheckFailedReason::Data,
    ));
}
```

The `i2c_master_version = "1"` gate is upstream's; the new one is
`esp-emu`.

### 2.3 espforge owns the `esp-hal` dependency line, so `features` edits vanish

Even with 2.1 and 2.2 done, the feature has to reach the compiler. Two traps
here:

**Trap A — the dependency line is a managed key.** `espforge` regenerates
`build/build/Cargo.toml` on every build and merges the user's
`Cargo.extra.toml` into it (`merge()` in
`espforge/espforge-emit/src/codegen/cargo.rs`). Inline tables such as

```toml
esp-hal = { version = "~1.2.2", features = ["esp32c3", "unstable", "log-04"] }
```

are merged **key by key**, and for keys the generated document also has, the
generated value wins — with a warning:

```text
regenerating managed key `dependencies.esp-hal.features`
```

So `features = [..., "esp-emu"]` in `Cargo.extra.toml` is silently (well, one
warning line) replaced by the chip/unstable/log list that
`dependencies()` builds in `cargo.rs`
(`deps["esp-hal"] = Item::Value(dep("~1.2.2", &hal))`). The version
requirement `~1.2.2` is managed the same way.

**Trap B — `[patch.crates-io]` is silently ignored on a version mismatch.**
Cargo only applies a path patch whose version satisfies the requirement. The
copy of `esp-hal` that shipped in this repo was the **1.2.0 monorepo crate**,
so the `~1.2.2` requirement did not match, and cargo compiled
**esp-hal 1.2.2 from crates.io** instead. The proof is in the lock file:

```toml
[[patch.unused]]
name = "esp-hal"
version = "1.2.0"
```

That means the patched source was never compiled — no amount of
`cargo clean` could have helped, because the code being built was not the
patched code.

The monorepo copy also could not have built on its own: its manifest has
`esp-rom-sys = { path = "../esp-rom-sys" }`, `esp-metadata-generated =
{ path = "../esp-metadata-generated" }`, `procmacros = { path =
"../esp-hal-procmacros" }` and friends, none of which exist here. That is why
the vendored copy is now the **published** 1.2.2 crate, whose manifest is
self-contained.

---

## 3. What is in place now

### `esp-hal/` (untracked, repo-local)

Published `esp-hal` 1.2.2 source, plus exactly two edits:

1. `[features] esp-emu = []` in `Cargo.toml`;
2. the `#[cfg(not(feature = "esp-emu"))]` gate around the `resp_rec` check in
   `src/i2c/master/low_level/mod.rs`.

The repo's own `CHANGELOG.md` / `MIGRATING-*.md` were kept. `Cargo.toml.orig`
(found in every published `.crate`) is the **monorepo** manifest with the
`path = "../…"` deps — do not point anything at it; cargo only reads
`Cargo.toml`.

**This directory is not tracked by git.** A fresh clone starts without it, and
the workaround does nothing there until §8 is run.

### `drivers/i2cscanner/Cargo.extra.toml`

```toml
# esp-emu 1.1.0 never sets the I2C master's `sr.resp_rec` bit, so esp-hal's
# RESP_REC check fails every transaction. The vendored patch in ../../esp-hal
# drops that check behind an `esp-emu` feature.
#
# espforge owns the generated `dependencies.esp-hal` line (a `features` edit
# there is overwritten on every build), so the feature is pulled in through
# this crate's own default feature instead.
[features]
default = ["esp-hal/esp-emu"]

[patch.crates-io]
esp-hal = { path = "../../esp-hal" }
```

Both halves are load-bearing, and neither is a managed key:

- `[features]` is a plain table espforge does not generate, so the merge keeps
  it verbatim. `default = ["esp-hal/esp-emu"]` switches the patch on for
  `cargo build` / `cargo espflash save-image`, both of which run with default
  features. That is why no recipe needs a `--features` flag.
- `[patch.crates-io]` is likewise preserved. The path is relative to the
  **generated project** (`build/build/`), so `../../esp-hal` always means the
  repo root's `esp-hal/`, no matter which driver the project came from.

The generated manifest then looks like this (the `[features]` table lands
wherever the merge puts it, which is fine — order does not matter in TOML):

```toml
[features]
default = ["esp-hal/esp-emu"]
# Generated by espforge from your .espf. Do not edit: this file is
# rewritten on every build. Put extra dependencies in Cargo.extra.toml.
[package]
…
[dependencies]
esp-hal = { version = "~1.2.2", features = ["esp32c3", "unstable", "log-04"] }
…
[patch.crates-io]
esp-hal = { path = "../../esp-hal" }
```

---

## 4. Verifying any I²C fixture before blaming it

Run these in order. The first two are cheap and catch "the workaround is not
active"; only after they pass is a failing driver result meaningful.

```sh
# 1. The gate itself: esp-hal's I²C transactions complete at all.
just i2cscanner-test
```

Expected (exit 0):

```text
INFO - Found device at address 0x50
INFO - Scan complete
PASS: scan found the emulator's built-in EEPROM at 0x50, and nothing else
```

The captured console is `build/i2cscanner/scan.stdout`; the emulator log is
`build/i2cscanner/emu.log`. The firmware is
`build/i2cscanner/scanner.bin` / `.elf`, generated from
`drivers/i2cscanner/i2cscanner.espf.txt`
(`<i2c id="i2c0" sda="5" scl="6" frequency="100kHz" />`, no board: the
emulator's own bus).

```sh
# 2. Is the patched esp-hal the one being compiled?
cd build/build && cargo tree -e features -i esp-hal | grep -E 'esp-hal v|esp-emu'
```

Expected:

```text
esp-hal v1.2.2 (/…/esp-hal)
├── esp-hal feature "esp-emu"
```

a **path**, not a registry source, with the feature switched on (the feature
tree is long, so grep rather than eyeball it). The same is visible without the
tree:

```sh
cd build/build && cargo metadata --no-deps --format-version 1 \
  | grep -o '"default":\["esp-hal/esp-emu"\]'
```

Cross-check the lock file:

```sh
grep -A4 '^name = "esp-hal"$' build/build/Cargo.lock      # no `source = "registry+…"` line
grep -n 'esp-emu' esp-hal/Cargo.toml                      # feature declared
grep -n -B3 'resp_rec().bit_is_clear' esp-hal/src/i2c/master/low_level/mod.rs   # gate present
```

If `[[patch.unused]]` is still in the lock, the version does not satisfy
`~1.2.2` — see §7.

Only then run the driver fixture, e.g. `just sht31-scan`, `just sht31-sensor`.
The board still has to be right: the emulator suppresses its built-in 0x50
EEPROM on a port that a co-simulated part claims, so with `board.json` wired to
SDA 5 / SCL 6 the scan must find **0x44 and no 0x50**.

---

## 5. Putting the workaround in a new I²C project

Any espforge project that has an `<i2c …>` peripheral needs the overlay, in the
directory that holds its `.espf.txt` (the path passed to `just build`):

```toml
# Cargo.extra.toml — no [dependencies] table needed; espforge generates it.
[features]
default = ["esp-hal/esp-emu"]

[patch.crates-io]
esp-hal = { path = "../../esp-hal" }
```

The `path` is relative to the **merged manifest**, which espforge always writes
to `build/build/Cargo.toml` — *not* to the directory the `Cargo.extra.toml`
lives in, and not to the `.espf.txt`. So it is `"../../esp-hal"` for every
project in the repo, however deep its source sits (`drivers/i2cscanner`,
`drivers/001-sht31/firmware`, the regenerated `build/sht31-src/scanner`, …).
Only a project generated into a different `--out` directory would need a
different number of `../`.

Everything still goes through the recipe layer (`just check`, `just build`,
`just <fixture>-image`, `just <fixture>-run`). Do not run `espforge` or
`esp-emu` by hand, and do not edit `build/build/Cargo.toml` — it is rewritten.

Status of the I²C fixtures in this repo:

| Fixture | Project dir | Overlay present? |
|---|---|---|
| `just i2cscanner-test` | `drivers/i2cscanner/` | **yes** — green |
| `just sht31-image` / `sht31-sensor` | `drivers/001-sht31/firmware/` | no — still red at `Devices::new` |
| `just sht31-scan` | regenerated at `build/sht31-src/scanner/` | no — the recipe creates a fresh project, so it needs an extra file written (or copied in) by the recipe |

Drivers that ride on I²C and therefore inherit all of this
(`espforge/espforge-drivers/manifests/`): `sht31`, `sht4x`, `scd4x`, `bme680`,
`ads111x`, `ssd1306`.

---

## 6. What the workaround does *not* fix

- **Hardware.** With `esp-emu` on, a genuine data-phase ACK failure is not
  reported. Never flash such a build to a board. The feature name is deliberately
  ugly for that reason.
- **The emulator's I²C model in general.** `sr.resp_rec` is the one bit that
  blocks esp-hal; clock stretching, arbitration, bus errors and RX timing are
  still whatever `esp-emu` 1.1.0 does. Anything a driver fails on *after* the
  gate is green is a real finding.
- **Frame-level truth.** NACK detection still works (`Unknown`), but no
  address/data byte is validated against a wire model.
- **Other chips.** Only ESP32-C3 paths were exercised; `check_errors()` gates
  per `i2c_master_version`, and version-1 masters never had this check.
- **`testsht31.md` §1a and the comment on `just i2cscanner-run`** still describe
  I²C as red at 1.0.0. They are stale relative to this workaround.

---

## 7. Troubleshooting matrix

| Symptom | Meaning | Fix |
|---|---|---|
| `[[patch.unused]] esp-hal` in `build/build/Cargo.lock` | the path version does not satisfy `~1.2.2`; crates.io 1.2.2 is being compiled | make `esp-hal/Cargo.toml` say `version = "1.2.2"` (published tarball), or `cargo update -p esp-hal` |
| `error[E0761]: file for module 'system' found at both …` | mixed trees: the old monorepo copy overlaid with the published crate (`system.rs` next to `system/mod.rs`) | replace `esp-hal/` wholesale, do not `cp` over an existing checkout (§8) |
| `failed to load manifest for dependency esp-rom-sys … path does not exist` | you vendored the monorepo crate instead of the published one | §8: extract the published `.crate` |
| `regenerating managed key 'dependencies.esp-hal.features'` on build | you put the feature on the dependency line in `Cargo.extra.toml` | use the `[features] default = ["esp-hal/esp-emu"]` table instead |
| scan prints `Scan complete` and nothing else | feature not reaching the build, or gate missing | §4 checks: `cargo tree -e features`, lock file, grep the source |
| every transaction `AcknowledgeCheckFailed(Data)` | the `resp_rec` gate is not compiled in | as above |
| `AcknowledgeCheckFailed(Unknown)` at one address | nothing answers there — real, expected | check `board.json` `addr`/pins, or the address in the `.espf.txt` |
| build is green but the image behaves like an older one | stale artifacts from before the patch | `cd build/build && cargo clean -p esp-hal`, then rebuild |
| `unknown feature 'esp-emu'` | the vendored manifest lost its feature (re-vendored without the edit) | re-apply §8 step 3 |

---

## 8. Re-creating the vendored esp-hal (fresh clone, or new version)

`esp-hal/` is untracked, so this is the whole setup. Everything is repo-local
and network access is only needed for the crates.io download.

```sh
# 1. Replace the tree with the PUBLISHED crate (self-contained manifest).
#    Never use a monorepo checkout: it has ../esp-rom-sys-style path deps.
tmp=$(mktemp -d)
curl -sL https://static.crates.io/crates/esp-hal/esp-hal-1.2.2.crate | tar xz -C "$tmp"
rm -rf esp-hal
cp -a "$tmp/esp-hal-1.2.2" esp-hal
rm -rf "$tmp"
```

Then the two edits, both required:

```toml
# esp-hal/Cargo.toml — under [features]
esp-emu = []
```

```rust
// esp-hal/src/i2c/master/low_level/mod.rs — inside `check_errors()`
#[cfg(not(feature = "esp-emu"))]
#[cfg(not(i2c_master_version = "1"))]
if r.trans_complete().bit_is_set() && self.regs().sr().read().resp_rec().bit_is_clear() {
```

Finally, per project that should use it, the `Cargo.extra.toml` from §5, then:

```sh
just i2cscanner-test
```

**Version rule:** the vendored `version` must satisfy the generated
requirement `~1.2.2` (i.e. `>=1.2.2, <1.3.0`). A new esp-hal release that
espforge does not generate yet can be vendored, but bumping espforge's
`dep("~1.2.2", …)` in `espforge/espforge-emit/src/codegen/cargo.rs` would be a
separate, tracked change — the local overlay alone cannot move the requirement.

**Version of the emulator is irrelevant to the gate:** both 1.0.0 (as recorded
in `testsht31.md` §1a) and 1.1.0 (installed here) lack the `resp_rec` model, so
upgrading `esp-emu` does not remove the need for the patch. When an emulator
release does model it, the overlay should be dropped rather than kept — verify
by reverting `[features] default` in one fixture and rerunning
`just i2cscanner-test`.

---

## 9. Evidence from the fix (2026-11-18)

Reproducing the diagnosis, in the order it was established:

1. `drivers/i2cscanner` built and booted; `build/i2cscanner/scan.stdout` showed
   `TEST: i2c scanner begin` then `Scan complete`, no `Found device` line.
2. `i2cissue.md` (standalone probe): 0x50 → `AcknowledgeCheckFailed(Data)`,
   0x44 → `AcknowledgeCheckFailed(Unknown)`; identical at 100 kHz and 400 kHz,
   with and without a board. Confirms §2.1, rules out frequency/board.
3. The gate was already present in the vendored source
   (`esp-hal/src/i2c/master/low_level/mod.rs`), but the generated manifest had
   `features = ["esp32c3", "unstable", "log-04"]`; the `esp-hal` entry in
   `build/build/Cargo.lock` had `source = "registry+https://github.com/rust-lang/crates.io-index"`
   and the lock carried `[[patch.unused]] esp-hal 1.2.0`. Confirms §2.3 trap B.
4. `espforge build` compiles with plain `cargo build` and no `--features`, so
   the feature had to come from `[features] default` — §2.3 trap A.
5. Replacing `esp-hal/` with the published 1.2.2 crate (1.2.0 → 1.2.2, and no
   `../esp-rom-sys` path deps), re-applying the gate and the feature, and adding
   the extra-file overlay made the lock resolve esp-hal from the path; the
   scanner then reported exactly one device, 0x50, and `just i2cscanner-test`
   exited 0.

Before/after, same fixture, same emulator:

| | Before | After |
|---|---|---|
| esp-hal in graph | 1.2.2 (crates.io) | 1.2.2 (`/…/esp-hal` path) |
| `esp-emu` feature | absent | on, via `[features] default` |
| scan output | `Scan complete` only | `Found device at address 0x50`, `Scan complete` |
| `just i2cscanner-test` | fail | **PASS** |

---

## 10. Related documents

- `testsht31.md` §1a — the original red-gate write-up (esp-emu 1.0.0), still
  accurate about the mechanism, stale about it being unfixable.
- `i2cscannerfix.md` — the proposed patch. Correct on the HAL side; wrong in
  practice only about *how the feature gets enabled* (a root `Cargo.toml` with
  `--features esp-emu`, which espforge's managed manifest cannot express).
- `i2cissue.md` — the minimal probe transcript quoted in §9.
