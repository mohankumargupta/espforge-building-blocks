# `plantmp123.md`: TMP123 happy path as an external (project-local) driver

**Goal:** add a TMP123 temperature sensor fixture under `drivers/tmp123/` whose
driver is **external** to `espforge-drivers` — a project-local KDL manifest read
from `drivers/tmp123/drivers/`, not a bundled module — and prove the happy path
end to end against the emulator's virtual TMP123 SPI part.

**Status:** built and verified on 2026-11-16, ESP32-C3 emulator, `esp-emu`
1.0.0. All five checks in §12 pass; `just tmp123-sensor` prints the five
`PASS` lines. Two places where reality differed from the plan are corrected
below: the attach assertion reads `emu.log` (§5, §7), and the fixture README
is `drivers/tmp123/README.md` (§11).

**Baseline measured against the code, not assumed:**

| Fact | Where it comes from |
|---|---|
| A project-local driver is `<spec dir>/drivers/*.kdl`, read on every `check`/`gen`/`build` | `espforge/espforge-core/src/drivers.rs:27-48`, `collect()` at `:91` |
| The scan is a flat `*.kdl` listing; subdirectories are not read | `read_dir()` at `drivers.rs:116-140` |
| `codegen.feature` absent + a `dependency` block = manifest-only: the crate goes into the generated project's `Cargo.toml`, no `espforge-drivers` module or feature | `espforge-schema/src/driver.rs:1310`, `espforge-emit/src/codegen/cargo.rs:230-250` |
| A dependency `path` is relative to the manifest that declared it and is canonicalized; it must stay inside the project | `drivers.rs:512-577` |
| The first `gen`/`build` writes `<spec dir>/.espforge/drivers.lock`; later runs refuse a changed manifest | `espforge-core/src/project.rs:75-112`, `drivers.rs:482-509` |
| `check` validates but does **not** run cargo | `project::check()` at `project.rs:157` |
| `<spi>` attributes are `sclk`, `mosi` (required), `miso`, `mode`, `frequency`, `cs` | `espforge-schema/src/espf.schema.kdl:166-186` |
| The blocking SPI endpoint `espforge_runtime::ConfiguredSpiDevice` implements `embedded_hal::spi::SpiDevice` | `espforge-runtime/src/hw/spi.rs:607-611` |
| A TMP123 virtual part already exists, with `CS`/`SCK`/`SO` pin names and `warmer`/`cooler`/`set:temperature=` inputs | `esp-emulator/examples/parts/tmp123/README.md` |
| No crates.io driver for the TMP121/TMP123 exists (checked: `tmp1x2`, `tmp117`, `tmp451`, `microchip-tc72r-rs`, `embedded-sht3x`) | crates.io / docs.rs search, 2026-11-16 |

---

## 1. Why not a pure manifest-only driver

Manifest-only does not mean "no Rust". It means the Rust type and constructor
come from a crate the manifest names, so the manifest is the whole definition.
That needs a crate whose API fits espforge's three naming slots:

- `type` — a concrete type over the endpoint espforge's bus helper returns;
- `constructor` — a call on a plain path whose first argument is `{bus_device}`;
- `endpoint` — the bus type, only when `init="deferred"` is offered.

For TMP123 there is no such crate:

| Candidate | Why it does not fit |
|---|---|
| `tmp1x2` 1.1.0 | TMP102/TMP112 **I2C**, `nb`-based, `new(i2c, SlaveAddr)` |
| `tmp117` 3.0.0 | different part (I2C/SPI address scheme), `device-register` based |
| `tmp451` | I2C remote/local sensor |
| `microchip-tc72r-rs` | TC72 SPI, different framing |
| `embedded-sht3x` | SHT3x I2C; the bundled `sht31` driver's crate |

So "manifest only against crates.io" is not reachable today. The two ways out:

1. **Thin local crate + project-local manifest** (this plan). The manifest is
   still external and still the only place the wiring is described; the Rust
   half is a ~40-line crate beside the fixture. No publish, no edit to the
   espforge checkout.
2. **Bundled driver** — a module and Cargo feature in `espforge-drivers`, plus
   `MANIFESTS`/`DRIVERS_SCHEMA_KDL` registration. That is not an external
   driver, and it means changing the espforge checkout; rejected here.

When a `tmp123` crate is eventually published, (1) becomes (2)-free pure
manifest-only with a one-line change: `path` → `version`. Section 10 has the
exact diff.

---

## 2. Target layout

```
drivers/tmp123/
  tmp123-test.espf.txt        # the project file (the spec dir is this folder)
  src/app.rs                  # the firmware's application half
  drivers/tmp123.kdl          # THE external driver: project-tier manifest
  tmp123-driver/              # its thin Rust half, named by a path dependency
    Cargo.toml
    src/lib.rs
  board.json                  # --cosim-board: CS 7, SCK 6, SO 2
  tmp123.wasm                 # built from esp-emulator/examples/parts/tmp123
  (git-ignored) .espforge/drivers.lock   # written by the first build
  (git-ignored) target/? not here — the crate keeps its own target/
```

The nested `drivers/` is not a typo. The project tier is literally
`<spec dir>/drivers/*.kdl`; with the spec directly in `drivers/tmp123`, that is
`drivers/tmp123/drivers/tmp123.kdl`. If the firmware were nested instead
(`drivers/tmp123/firmware/`, the `sht31`/`mhz19b` shape), the manifest would go
in `drivers/tmp123/firmware/drivers/`. Nothing else changes.

**No changes to `espforge/`.** That is the point of the exercise.

---

## 3. Milestone 1: the thin driver crate

**File `drivers/tmp123/tmp123-driver/Cargo.toml`**

```toml
# The Rust half of the external `<tmp123>` driver. Generic over
# `embedded_hal::spi::SpiDevice`, so it does not depend on esp-hal, and the
# manifest instantiates it with the endpoint espforge builds.
[workspace]

[package]
name = "tmp123-driver"
version = "0.1.0"
edition = "2021"
description = "Platform-agnostic TI TMP121/TMP123 SPI temperature sensor driver"
license = "MIT"

[dependencies]
embedded-hal = "1.0"
```

The empty `[workspace]` keeps the crate independent of any Cargo workspace that
might later appear above it.

**File `drivers/tmp123/tmp123-driver/src/lib.rs`**

```rust
//! A TI TMP121/TMP123 temperature sensor, over any `embedded-hal` SPI device.
//!
//! The sensor has no data input: while its chip select is low it clocks out one
//! 16-bit word, most significant bit first, whose bits 15..3 are the
//! temperature in 13-bit two's complement at 0.0625 °C a count (SBOS273C,
//! Table 2). Bits 2..0 are 0.
//!
//! The endpoint is generic, so espforge's `ConfiguredSpiDevice`, the emulator's
//! virtual part and a host test double all take the same code.

#![no_std]

use embedded_hal::spi::{Operation, SpiDevice};

/// A TMP121/TMP123 on an SPI device endpoint.
pub struct Tmp123<SPI> {
    spi: SPI,
}

impl<SPI: SpiDevice> Tmp123<SPI> {
    /// Take the endpoint. The sensor needs no configuration, so this cannot
    /// fail and the manifest declares `fallible #false`.
    pub const fn new(spi: SPI) -> Self {
        Self { spi }
    }

    /// One measurement, in degrees Celsius.
    ///
    /// A read is one 16-bit transfer. The part converts continuously; this
    /// does not wait for a conversion to finish, so a read returns the last
    /// result, which is what the datasheet's interface allows.
    pub fn read_celsius(&mut self) -> Result<f32, SPI::Error> {
        let mut bytes = [0u8; 2];
        self.spi.transaction(&mut [Operation::Read(&mut bytes)])?;
        let raw = i16::from_be_bytes(bytes);
        // Arithmetic shift: the sign is bit 15, and 13 bits remain.
        Ok((raw >> 3) as f32 * 0.0625)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use embedded_hal::spi::{ErrorType, Operation, SpiDevice};

    /// The part, reduced to what a transfer sees: one 16-bit word, then 0xFF.
    struct Fake {
        word: u16,
    }

    impl ErrorType for Fake {
        type Error = Infallible;
    }

    impl SpiDevice for Fake {
        fn transaction(
            &mut self,
            operations: &mut [Operation<'_, u8>],
        ) -> Result<(), Self::Error> {
            for operation in operations {
                if let Operation::Read(buf) = operation {
                    let word = self.word.to_be_bytes();
                    for (i, byte) in buf.iter_mut().enumerate() {
                        *byte = if i < 2 { word[i] } else { 0xFF };
                    }
                }
            }
            Ok(())
        }
    }

    fn read(word: u16) -> f32 {
        Tmp123::new(Fake { word }).read_celsius().unwrap()
    }

    #[test]
    fn every_row_of_the_datasheets_table_2_reads_as_listed() {
        // SBOS273C Table 2, "Temperature Data Format".
        let table = [
            (150.0, 0x4B00),
            (125.0, 0x3E80),
            (25.0, 0x0C80),
            (0.0625, 0x0008),
            (0.0, 0x0000),
            (-0.0625, 0xFFF8),
            (-25.0, 0xF380),
            (-55.0, 0xE480),
        ];
        for (celsius, word) in table {
            assert_eq!(read(word), celsius, "{celsius} °C");
        }
    }

    #[test]
    fn a_floating_bus_reads_minus_one_count_not_garbage() {
        // 0xFFFF sign-extends to -1 and shifts to -1, so a missing part reads
        // -0.0625 °C. That looks plausible: the emulator test must assert exact
        // values and the attach line, never just "some reading arrived".
        assert_eq!(read(0xFFFF), -0.0625);
    }
}
```

**Verify**

```sh
just tmp123-driver-test    # recipe added in Milestone 3
```

This is the protocol gate: no toolchain, no emulator, no esp-hal. If the shift
or the sign extension is wrong, it fails here rather than as a puzzling reading
under the emulator.

---

## 4. Milestone 2: the external manifest, the spec and the app

**File `drivers/tmp123/drivers/tmp123.kdl`**

```kdl
// The external `<tmp123>` driver: a project-local manifest, not a module in
// espforge-drivers. `codegen.feature` is absent, so the dependency below is
// written into the generated project's Cargo.toml instead of selecting an
// espforge-drivers feature.
driver "tmp123" {
    format_version 2
    summary "Texas Instruments TMP121/TMP123 SPI temperature sensor"
    description "Reads one 16-bit word over SPI: bits 15..3 are the temperature in 13-bit two's complement at 0.0625 °C a count (SBOS273C Table 2). The Rust half is the `tmp123-driver` crate beside this manifest, so the driver is external to espforge-drivers. It does not wait for a conversion; a read returns the sensor's last result."
    category "sensor"

    dependency "tmp123-driver" {
        path "../tmp123-driver"
    }

    bus "spi" {
        mode 0
        frequency "1000kHz"
    }

    codegen {
        fallible #false
        runtime "blocking" {
            type "tmp123_driver::Tmp123<espforge_runtime::ConfiguredSpiDevice>"
            constructor "tmp123_driver::Tmp123::new({bus_device})"
            endpoint "espforge_runtime::ConfiguredSpiDevice"
        }
    }

    attr "init" kind="choice" default="boot" {
        choices "boot" "deferred"
        help "`boot` builds the sensor during `Devices::new`; `deferred` stores the bus and constructs it in `init()`."
    }

    example "minimal" #"<tmp123 id="temp" bus="$spi2" cs="7" />"#
    see_also "peripherals/spi"
}
```

Why each line is what it is:

- **No `feature`.** That is the external-driver signal. With `feature` the Rust
  would have to live in `espforge-drivers` and the tag would have to be
  registered in `espforge-drivers/src/lib.rs`; a project-local file cannot do
  either.
- **`path "../tmp123-driver"`** resolves from this manifest's directory, i.e.
  `drivers/tmp123/tmp123-driver`, which is inside the project dir
  `drivers/tmp123`, so the path check accepts it and Cargo receives an absolute
  path in the generated `Cargo.toml`.
- **`fallible #false`** because `Tmp123::new` returns `Self`. Without it the
  generated `.map_err(..)` would not compile.
- **`{bus_device}` first** is the only slot the endpoint fills; there is no
  `{address}` and no `{delay}` because the part has neither.
- **`endpoint`** is the type the bus helper returns (not the driver type), and
  is what `init="deferred"` would store. Boot is the tested path here.
- **`mode 0` / `1000kHz`** are applied by `ConfiguredSpiDevice` before each
  transfer. The virtual part ignores timing; real hardware must be checked
  against the datasheet's serial-interface section (see §9).
- **`cs` is not declared here.** `bus "spi"` synthesises it, and the element
  passes the pad.

**File `drivers/tmp123/tmp123-test.espf.txt`**

```text
====================
TMP123 test firmware

Fixture for plantmp123.md: the project-local `<tmp123>` driver over SPI, wired
to the virtual TMP123 part (CS = GPIO 7, SCK = GPIO 6, SO = GPIO 2). MOSI is
declared because `<spi>` requires it; the sensor has no data input.

Prints `TEST:` lines the recipes assert on.
====================

# espforge
  <project name="tmp123_test" />
  <chip type="esp32c3" />
  <runtime type="blocking" />

# peripherals
  <spi id="spi2" sclk="6" mosi="3" miso="2" mode="0" frequency="1000kHz" />

# devices
  <tmp123 id="temp" bus="$spi2" cs="7" />

# services
```

**File `drivers/tmp123/src/app.rs`**

```rust
//! Test fixture for the external `<tmp123>` driver (plantmp123.md).
//!
//! The driver is `drivers/tmp123/drivers/tmp123.kdl` plus the `tmp123-driver`
//! crate; this app only adds machine-checkable output:
//!
//! - `TEST: tmp123 boot`
//! - `TEST: reading t=25.00 c` once a second
//!
//! Pins match `drivers/tmp123/board.json` (CS 7, SCK 6, SO 2). The sensor has
//! no data input, so MOSI (GPIO 3) is declared and unused.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: tmp123 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, temp).read_celsius() {
        Ok(celsius) => ctx.logger.info(format_args!("TEST: reading t={:.2} c", celsius)),
        Err(_) => ctx.logger.error("TEST: reading failed"),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
```

Note the diagnostic difference from `sht31`: this constructor is infallible, so
a bus that does not work surfaces as `TEST: reading failed` every second, not as
`device `temp`: initialization failed`. That is expected, not a bug.

---

## 5. Milestone 3: `just` recipes

`AGENTS.md` says espforge is driven only through `just`, so every command below
is a recipe. Add to `justfile`, beside the other driver sections:

```make
# ---- TMP123 external driver fixture (drivers/tmp123) ---------------------

tmp123_rel := "drivers/tmp123"
tmp123_dir := justfile_directory() / tmp123_rel
tmp123_out := build_dir / "tmp123"
tmp123_port := "127.0.0.1:5561"
tmp123_part := justfile_directory() / "esp-emulator" / "examples" / "parts" / "tmp123"

# The thin crate's own tests: the protocol math, on the host, no emulator.
tmp123-driver-test:
    cargo test --manifest-path {{ tmp123_dir }}/tmp123-driver/Cargo.toml

# Build the emulator's virtual TMP123 part -> drivers/tmp123/tmp123.wasm.
tmp123-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ tmp123_part }}/Cargo.toml"
    cp "{{ tmp123_part }}/target/wasm32-unknown-unknown/release/tmp123.wasm" \
        "{{ tmp123_dir }}/tmp123.wasm"
    ls -l "{{ tmp123_dir }}/tmp123.wasm"

# Schema, resource graph, manifest discovery and the driver's path dependency.
tmp123-check:
    just check {{ tmp123_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
tmp123-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ tmp123_out }}"
    just check "{{ tmp123_rel }}"
    just build "{{ tmp123_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ tmp123_out }}/tmp123.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/tmp123_test" \
        "{{ tmp123_out }}/tmp123.elf"
    ls -l "{{ tmp123_out }}/tmp123.bin" "{{ tmp123_out }}/tmp123.elf"

# Run the firmware with the virtual sensor on the control channel.
tmp123-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ tmp123_out }}/tmp123.bin --cosim-board {{ tmp123_dir }}/board.json --control-tcp {{ tmp123_port }} --elf {{ tmp123_out }}/tmp123.elf --timeout {{ timeout }} --log-file {{ tmp123_out }}/emu.log"

# One control line, e.g. "part thermometer input warmer".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
tmp123-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ tmp123_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/tmp123/drivers/tmp123.kdl.
# This is `espforge driver <project dir> add --local <manifest>`: the project
# dir is the positional before the subcommand, and `--local` records the file
# where it is instead of copying it.
tmp123-driver-record:
    just espforge "driver {{ tmp123_rel }} add --local {{ tmp123_rel }}/drivers/tmp123.kdl"

# Happy path: boot, read 25 °C, warmer, then an exact set value.
tmp123-sensor: tmp123-image tmp123-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ tmp123_out }}/sensor.stdout"
    : > "$out"
    setsid just tmp123-run 90s > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    wait_for() {
        local i
        for i in $(seq 1 "$3"); do
            grep -q "$1" "$out" && { echo "PASS: $2"; return 0; }
            sleep 1
        done
        echo "FAIL: $2 (no '$1'); see $out" >&2
        ls -l "$out" >&2 || true
        return 1
    }
    # The attach line goes to --log-file, not to the emulator's stdout: the
    # firmware console and the co-simulation log are different streams.
    log="{{ tmp123_out }}/emu.log"
    : > "$log"
    wait_for_log() {
        local i
        for i in $(seq 1 "$3"); do
            grep -q "$1" "$log" && { echo "PASS: $2"; return 0; }
            sleep 1
        done
        echo "FAIL: $2 (no '$1'); see $log" >&2
        return 1
    }
    wait_for "TEST: tmp123 boot" "firmware boots" 40
    wait_for_log "is the SPI device on chip select GPIO 7" "the part attached on the firmware's CS" 10
    wait_for "TEST: reading t=25.00 c" "fresh part reads 25.00 C" 20
    just tmp123-part-input "part thermometer input warmer" >/dev/null
    wait_for "TEST: reading t=26.00 c" "warmer -> 26.00 C" 20
    just tmp123-part-input "part thermometer input set:temperature=31.5" >/dev/null
    wait_for "TEST: reading t=31.50 c" "set:temperature=31.5 -> 31.50 C" 20
    echo "tmp123-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
tmp123-clean:
    rm -rf {{ tmp123_out }}
```

Two recipe details that are easy to get wrong:

- `just check`/`just build` take the **spec directory**; the project tier is
  found from it, so no `--driver-dir` is needed.
- `tmp123-driver-record` uses the **relative** path so the lock's `source`
  field stays relative and the lock does not carry a machine-specific path.

**Prerequisites:** `wasm32-unknown-unknown` must be installed
(`rustup target add wasm32-unknown-unknown`; it is already installed here), and
the first `tmp123-part` fetches the part SDK's crates (`esp-part` → `prost`,
`protox`). Generation stays offline; only the part build reaches the network,
once.

---

## 6. Milestone 4: the compile gate

```sh
just tmp123-driver-test   # protocol math, on the host
just tmp123-check         # manifest parses, lowers, dependency path resolves
just tmp123-image         # the generated project compiles
```

`tmp123-check` proves the manifest is data espforge accepts; it does **not**
compile `type`/`constructor`. `tmp123-image` is the first place a wrong type
parameter, a wrong crate alias (`tmp123_driver`, not `tmp123-driver`), or a
constructor-shape mistake appears — as a cargo error in `build/build`, not as an
espforge diagnostic.

Expected: the `tmp123-image` line `driver: .../drivers/tmp123/drivers/tmp123.kdl`
in `gen`'s output, then a successful `cargo build`, then the two files listed.

At this point the external-driver mechanism is fully proven: discovery, path
dependency, `fallible #false`, the generated `Tmp123<ConfiguredSpiDevice>`, and
the `device!(ctx, temp)` accessor all compiled.

---

## 7. Milestone 5: the virtual part and the happy path

**File `drivers/tmp123/board.json`**

```json
{ "parts": {
    "thermometer": { "module": "tmp123.wasm", "pins": { "CS": 7, "SCK": 6, "SO": 2 } }
} }
```

Pin names are case-sensitive and belong to the part, not the chip. This is
exactly the wiring the part's README documents for ESP32-C3.

**Why this should work even though espforge drives CS as a GPIO:** the generated
`ConfiguredSpiDevice` does not use the SPI peripheral's chip-select signal; it
drives CS as a plain `Output` pin around each transaction
(`espforge-runtime/src/hw/spi.rs:411-425`). The emulator handles that case — a
part that names its clock or data pads lets it find the bus — and this part's
claim names `SCK` and `SO` (`esp-emulator/examples/parts/tmp123/src/lib.rs`,
`claims()`). So the GPIO-CS path is the one exercised, and it is supported by
design.

**Run it**

```sh
just tmp123-part      # build the wasm into the fixture
just tmp123-sensor    # boot, 25.00, warmer -> 26.00, set -> 31.50
```

**What you should see**

| Where | Line |
|---|---|
| emulator log (`build/tmp123/emu.log`) | `co-simulation: part thermometer (tmp123) attached: wired CS=GPIO7 SCK=GPIO6 SO=GPIO2` |
| emulator log | `is the SPI device on chip select GPIO 7` |
| firmware console (`build/tmp123/sensor.stdout`) | `INFO - TEST: tmp123 boot` |
| firmware console | `INFO - TEST: reading t=25.00 c` |
| after `warmer` | `INFO - TEST: reading t=26.00 c` |
| after `set:temperature=31.5` | `INFO - TEST: reading t=31.50 c` |

Measured, not assumed: the attach line is written only to `--log-file`; the
firmware console and the co-simulation log are separate streams. The recipe
asserts the attach line in `build/tmp123/emu.log` (§5) and everything else in
`sensor.stdout`.

**Byte-level evidence, if a value is wrong:** re-run with
`--cosim-log events.jsonl`, or add the flag to a one-off `just esp-emu` line.
Each SPI read is one `BusRequest` with `rx_len: 2` and one `BusReply` whose
`data` is the word; 25 °C is `0x0C80`.

---

## 8. The driver lock

This is the one external-driver behaviour that surprises people:

1. The **first** `just tmp123-image` (its `gen` step) writes
   `drivers/tmp123/.espforge/drivers.lock` with the manifest's sha256 and its
   dependency list. `just tmp123-check` alone does not write it.
2. From then on every `check`, `gen` and `build` **verifies** it. Editing
   `drivers/tmp123/drivers/tmp123.kdl` — even a comment — fails with:

   ```text
   DRIVER_LOCK_STALE: `<tmp123>` in 'drivers/tmp123/drivers/tmp123.kdl' changed since it was recorded
      fix: espforge driver update tmp123
   ```

   Two fixes work: `espforge driver update tmp123` re-hashes a `local` entry in
   place, and `driver add --local` rewrites the entry. `just tmp123-driver-record`
   uses the second because it also validates the manifest and works before a
   lock exists.
3. Editing `tmp123-driver/`, `src/app.rs`, the `.espf.txt` or `board.json` does
   **not** need a re-record. Only the manifest text is pinned.
4. `verify` compares hashes, not paths, so a lock whose `source` is relative
   works wherever the repository is checked out.

**Commit or ignore?** Both work. Committing the lock records the manifest hash
and is espforge's intent for fetched drivers; ignoring it keeps the tree free of
a one-line generated file. Recommended: add

```gitignore
drivers/tmp123/.espforge/
```

and let the first build recreate it, because the fixture's whole value is a
manifest you will edit repeatedly while the mechanism is new.

---

## 9. Reading a failure

| Symptom | Cause | Next step |
|---|---|---|
| `TEST: reading failed` every second, but boot printed | The transfer itself failed: esp-hal's SPI did not complete under the emulator | The same class of problem as `repro.md`, but on SPI and unproven here. Check `build/tmp123/emu.log` for an SPI/bus error; a bare HAL failure is a toolchain gate, not a driver bug |
| Readings are `-0.06` or hover near zero | No part selected, or the CS pad is wrong: 0xFFFF sign-extends to −1 count | Compare the attach line's `CS=` with `board.json` and the element's `cs` |
| Readings are `511.94` | Not the TMP123: a 0xFF-filled 16-bit word interpreted as positive can only happen if the shift changed | Re-run `just tmp123-driver-test` |
| Emulator refuses the board at load | A pin name the part does not have, or a pad another part drives | Pin names are `CS`, `SCK`, `SO` in upper case |
| End-of-run warning: `is the SPI device on GPIO N, but the firmware never put a chip select on it` | `cs="7"` does not match the board's `CS: 7`, or the firmware never set the device up | Make the three agree: element `cs`, `board.json`, and the log line |
| End-of-run warning naming a bus line | `sclk`/`miso` in the spec differ from the board's `SCK`/`SO` | Make them agree |
| `DRIVER_LOCK_STALE` | The manifest was edited after the first build | `just tmp123-driver-record` |
| `cannot read crate path 'tmp123-driver'` | The dependency path does not resolve from the manifest, or escapes the project dir | The crate must be at `drivers/tmp123/tmp123-driver`; `path "../tmp123-driver"` |
| `expected feature, fallible, runtime or map` | A typo inside `codegen` | `codegen` accepts `feature`, `fallible`, `runtime`, `map` only |

---

## 10. What this does not prove, and what comes next

1. **Real hardware.** The virtual part has no conversion timing, no accuracy and
   no TMP121/TMP123 package distinction. Before wiring a sensor, confirm the
   datasheet's serial-interface timing (clock idle level, which edge clocks
   data out) against `mode 0`; the emulator cannot catch a mode error.
2. **Async.** The crate is blocking (`embedded_hal::spi::SpiDevice`). An embassy
   variant needs `embedded-hal-async` behind a feature, a second manifest with a
   separate `dependency` feature set, and `espforge_runtime::ConfiguredAsyncSpiDevice`
   as its endpoint. Out of scope here.
3. **Deferred init.** The manifest offers `init="deferred"` and names the
   endpoint, but only `boot` is exercised. Enabling it is one element attribute
   (`init="deferred"`) and one compile check; the `adxl345` manifest is the
   bundled precedent for the same shape.
4. **Other sensors on the bus.** Do not add a second device to `$spi2` in this
   fixture. `ConfiguredSpiDevice` owns one CS pad and one configuration; a
   multi-device board is a separate test.
5. **Promotion to pure manifest-only.** When `tmp123-driver` is published:

   ```diff
   -    dependency "tmp123-driver" {
   -        path "../tmp123-driver"
   -    }
   +    dependency "tmp123-driver" { version "0.1.0" }
   ```

   Then delete the local crate and the path resolves from crates.io. Nothing
   else in the manifest changes — which is the point of keeping the crate
   generic over `SpiDevice` instead of importing esp-hal.

---

## 11. Ordered checklist

- [x] 1. Create `drivers/tmp123/tmp123-driver/` (`Cargo.toml`, `src/lib.rs`)
      — §3
- [x] 2. `just tmp123-driver-test` — the datasheet table passes on the host
- [x] 3. Create `drivers/tmp123/drivers/tmp123.kdl` — §4
- [x] 4. Create `drivers/tmp123/tmp123-test.espf.txt` and `src/app.rs` — §4
- [x] 5. Add the `justfile` recipes — §5
- [x] 6. `just tmp123-check` — the manifest is discovered, parses and lowers
- [x] 7. `just tmp123-image` — the generated project compiles (the real
      manifest gate)
- [x] 8. `just tmp123-part` — `drivers/tmp123/tmp123.wasm` exists
- [x] 9. Create `drivers/tmp123/board.json` — §7
- [x] 10. `just tmp123-sensor` — 25.00 → 26.00 → 31.50, all `PASS` lines
- [x] 11. Add `drivers/tmp123/.espforge/` to `.gitignore` — §8
- [x] 12. `drivers/tmp123/README.md` records the outcome, and this document
      is corrected where reality differed (§5, §7)

One correction to §6: `just build` prints `build: wrote 'tmp123_test'` and
compiles; the `driver: <path>` line is `gen`'s own summary, and the recipes
do not call `gen` separately. The compile is what proves the manifest.

## 12. Verification matrix

| Check | What it proves |
|---|---|
| `just tmp123-driver-test` | The 13-bit two's-complement decode and the 0.0625 °C scale, on the host |
| `just tmp123-check` | The project-tier scan found the manifest; the tag is new; the dependency `path` resolves inside the project; the spec lowers with the driver |
| `just tmp123-image` | The manifest's `type`/`constructor`/`endpoint` type-check against `ConfiguredSpiDevice`; the generated `Cargo.toml` carries the path dependency; `fallible #false` emits no `.map_err`; `device!(ctx, temp)` yields the right handle |
| `just tmp123-part` | The part builds to a loadable `tmp123.wasm` for this emulator |
| `just tmp123-sensor` | End to end: the manifest's wiring reaches the emulator's SPI bus, CS selects the part, the value is exact, and both interaction forms change it |

**Non-goals:** publishing a crate, changing `espforge/`, async, deferred init,
a second device on the bus, real hardware validation.
