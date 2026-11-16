# `planadxl345.md`: an ADXL345 virtual part and the happy path

**Goal:** put an ADXL345 virtual part beside the emulator's other parts, wire
it to the bundled espforge `<adxl345>` driver over SPI, and prove the happy
path end to end: boot, `DEVID` accepted, three axes read, one interaction
changes them. No hardware, no changes to `espforge/`.

**Scope:** the main happy path only. No negative cases, no I2C, no async, no
`init="deferred"`, no real-hardware validation. `plantmp123.md` is the
long-form precedent for the mechanics; this document only carries what the
ADXL345 needs.

**Current state:** `drivers/adxl345/` exists but is empty (checked). The part
does not exist in `esp-emulator/examples/parts/`. Both are created here.

**Baseline measured against the code, not assumed:**

| Fact | Where it comes from |
|---|---|
| `<adxl345>` is a bundled driver: manifest, `feature "adxl345"`, module `adxl345` | `espforge/espforge-drivers/manifests/adxl345.kdl`, `espforge-drivers/src/lib.rs`, `espforge-drivers/src/adxl345.rs` |
| It is **SPI only**: `bus "spi"`, mode 3, 5 MHz | same manifest |
| Its constructor is fallible: it reads `DEVID` (`0x00`) and wants `0xE5`, else `DeviceErrorKind::Init` | `adxl345.rs`, `Adxl345::new` |
| Axes are six bytes from `DATAX0` (`0x32`), little-endian pairs X, Y, Z | `adxl345.rs`, `read()` and `decode()` |
| Read bit is `0x80`, multi-byte bit is `0x40`, so the two commands are `0x80` and `0xF2` | `adxl345.rs`, `READ` / `MULTI` |
| `Reading::milli_g()` is `counts * 4`; the datasheet scale is 3.9 mg/LSB | `adxl345.rs`, `milli_g()` |
| The endpoint drives CS as a plain GPIO around each transaction, not the controller's CS signal | `espforge/espforge-runtime/src/hw/spi.rs:411-425` |
| A part that claims a GPIO-driven CS must also name its clock/data pads so the emulator can find the bus | `esp-emulator/proto/PART_ABI.md`, "On SPI"; `examples/parts/eeprom93c46` |
| A part exports one `part_on_bus` call per transfer: `tx` is what the master clocked out, `rx_len` what it reads | `PART_ABI.md`, "On SPI"; `proto/esp_cosim.proto:910-932` |
| Parts are `cdylib` crates over `esp-part`, built for `wasm32-unknown-unknown` | `esp-emulator/esp-part/README.md`, `examples/parts/tmp123/Cargo.toml` |
| ESP32-C3 pads used by the existing SPI fixture: SCK 6, MOSI 3, MISO 2, CS 7 | `drivers/tmp123/tmp123-test.espf.txt`, `drivers/tmp123/board.json` |
| esp-hal 1.2.2 I2C does not complete under `esp-emu` 1.0.0 | `repro.md` |
| Emulator version to target: `esp-emu` 1.0.0, part ABI 1 | `drivers/tmp123/README.md`, `PART_ABI.md` |

## 1. Why SPI, and why no driver code

Two independent reasons:

1. **The bundled driver is SPI only.** Its manifest declares `bus "spi"`; there
   is no I2C wiring, no `{address}` slot and no second manifest. Testing the
   part over I2C would test a driver espforge does not have.
2. **I2C under this emulator is red anyway.** `repro.md` records that every
   esp-hal 1.2.2 I2C transaction fails `AcknowledgeCheckFailed` against
   `esp-emu` 1.0.0, with and without a virtual part. A happy path on I2C
   would fail before the part's protocol was ever exercised.

The corollary is that this fixture needs **no Rust driver and no KDL**: the
element, the crate dependency and the feature all come from
`espforge-drivers`. `drivers/adxl345/` holds only the spec, the app, the board
and the built `.wasm`.

## 2. Target layout

```text
esp-emulator/examples/parts/adxl345/   # the part, beside tmp123 and sht31
  Cargo.toml
  src/lib.rs
  README.md

drivers/adxl345/
  adxl345-test.espf.txt                # the espforge project
  src/app.rs                           # firmware application half
  board.json                           # CS 7, SCK 6, MOSI 3, MISO 2
  adxl345.wasm                         # built from the part above
  (git-ignored) build output lives in build/adxl345, not here
```

`esp-emulator/` is a vendored checkout and is git-ignored by the root
`.gitignore`, exactly like `espforge/`. The part's source therefore lives
where the emulator's own parts live and the fixture keeps the built `.wasm`,
which is what `drivers/tmp123/` does. If the part should be versioned from
this repository too, copy the crate to `drivers/adxl345/part/` and point its
`esp-part` dependency at `../../../esp-emulator/esp-part`; nothing else
changes.

**No changes to `espforge/`.**

## 3. The virtual part

**File `esp-emulator/examples/parts/adxl345/Cargo.toml`** — the same shape as
`tmp123`'s: its own workspace, a `cdylib`, one dependency.

```toml
# A virtual part: an Analog Devices ADXL345 accelerometer on SPI, compiled to
# a WebAssembly module the emulator loads. README.md says what it models.
#
# Its own workspace, so it builds on its own and nothing here enters esp-emu's
# dependency graph.
[workspace]

[package]
name = "adxl345"
version = "0.1.0"
edition = "2021"
publish = false

[lib]
crate-type = ["cdylib"]

[dependencies]
esp-part = { path = "../../../esp-part" }

[profile.release]
opt-level = "s"
lto = true
strip = true
```

**File `esp-emulator/examples/parts/adxl345/src/lib.rs`**

The happy path needs exactly three things from the chip: `DEVID` at `0x00`
answering `0xE5`, six axis bytes from `0x32` with auto-increment, and a way
for a person to move those axes. The register file below is a plain 64-byte
array so a read of any register answers something sensible; only `0x00` and
`0x32..=0x37` are special. Writes are stored (the bundled driver never
writes), which keeps `POWER_CTL`/`DATA_FORMAT` honest if a driver ever does.

```rust
//! An Analog Devices ADXL345 three-axis accelerometer, written as a virtual
//! part.
//!
//! The firmware reads it over SPI the way it would the real chip: a command
//! byte with bit 7 set for a read and bit 6 for auto-increment, then the
//! answer. `DEVID` (0x00) is 0xE5 and the three axes are six little-endian
//! bytes from `DATAX0` (0x32), which is all espforge's driver uses
//! (`espforge-drivers/src/adxl345.rs`). README.md lists what is left out.
//!
//! `Adxl345` is the device and knows nothing about WebAssembly. The
//! `export_part!` line at the end makes it a part: the `esp-part` crate
//! exports every function of `proto/PART_ABI.md`, `part_on_bus` included.

use esp_part::prelude::*;

/// The chip's own pin names (its datasheet's pin table). The board says which
/// chip pins they are wired to, as
/// `{ pins: { CS: 7, SCK: 6, MOSI: 3, MISO: 2 } }`.
const CS: &str = "CS";
const SCK: &str = "SCK";
const MOSI: &str = "MOSI";
const MISO: &str = "MISO";

/// `DEVID`: the fixed identity register, 0xE5 on every ADXL345.
const DEVID: u8 = 0x00;
const DEVID_VALUE: u8 = 0xE5;
/// `DATAX0`: X low byte, then Y and Z, two bytes each little-endian.
const DATAX0: u8 = 0x32;

/// Read bit: set for a read, clear for a write.
const READ: u8 = 0x80;
/// Multi-byte bit: auto-increment the register address between bytes.
const MULTI: u8 = 0x40;

/// Counts per g: the datasheet's 3.9 mg/LSB at the default ±2 g range.
const COUNTS_PER_G: f32 = 256.0;
/// ±2 g, the range `DATA_FORMAT` (0x31) powers up in.
const MAX_G: f32 = 2.0;

struct Adxl345 {
    /// The chip pins the part's own pins are wired to. `None` until the board
    /// says, or when it left the pin unconnected.
    cs: Option<u32>,
    sck: Option<u32>,
    mosi: Option<u32>,
    miso: Option<u32>,
    /// Gravity along each axis, in g.
    g: [f32; 3],
    /// Registers 0x00..=0x3F. `DEVID` and the axes are answered from the
    /// fields above; every other register is plain storage.
    regs: [u8; 64],
    /// The last command byte, for a read the emulator delivers as its own
    /// transfer with an empty `tx`.
    command: u8,
}

impl Default for Adxl345 {
    fn default() -> Self {
        let mut regs = [0u8; 64];
        // Bandwidth rate: the power-on 0x0A. Everything else powers up 0x00,
        // which is ±2 g, right-justified, 4-wire SPI in DATA_FORMAT (0x31).
        regs[0x2C] = 0x0A;
        Self {
            cs: None,
            sck: None,
            mosi: None,
            miso: None,
            // Flat on the table, gravity on +Z, so a fresh part reads 1 g.
            g: [0.0, 0.0, 1.0],
            regs,
            command: 0,
        }
    }
}

impl Device for Adxl345 {
    const NAME: &'static str = "adxl345";
    const ON_BUS: bool = true;

    /// Where the board wired the part.
    fn configure(&mut self, config: &PartConfig) {
        self.cs = config.pins.get(CS).copied();
        self.sck = config.pins.get(SCK).copied();
        self.mosi = config.pins.get(MOSI).copied();
        self.miso = config.pins.get(MISO).copied();
    }

    /// The chip select this part is selected by, plus the clock and data pads.
    /// espforge drives CS as a plain GPIO, so the pad carries no controller
    /// chip select; the emulator takes the bus from whichever of `SCK`,
    /// `MOSI` and `MISO` the firmware put a bus line on.
    fn claims(&self) -> Vec<Claim> {
        let spi = SpiClaim {
            cs_pin: self.cs,
            clk_pin: self.sck,
            mosi_pin: self.mosi,
            miso_pin: self.miso,
            ..Default::default()
        };
        vec![Claim { what: Some(claim::What::Spi(spi)) }]
    }

    /// Move gravity. `set:x=<g>`, `set:y=<g>`, `set:z=<g>` set one axis, and
    /// `rest` puts the chip flat on the table again.
    fn on_input(&mut self, name: &str) {
        if let Some((axis, value)) = set_value(name) {
            let value = (value as f32).clamp(-MAX_G, MAX_G);
            match axis {
                "x" => self.g[0] = value,
                "y" => self.g[1] = value,
                "z" => self.g[2] = value,
                _ => {} // Not an axis this part has.
            }
        } else if name == "rest" {
            self.g = [0.0, 0.0, 1.0];
        }
    }

    /// The guest's SPI transfer. A write stores a register; a read answers
    /// from the command byte, or from the last command if the transfer
    /// carries no new one.
    fn on_bus(&mut self, request: &BusRequest) -> Option<BusReply> {
        let Some(bus_request::Op::Spi(spi)) = &request.op else { return None };
        // A pure write: the first byte is the command, the rest the value.
        if spi.rx_len == 0 {
            if let Some(&command) = spi.tx.first() {
                self.command = command;
                if command & READ == 0 {
                    let reg = (command & 0x3F) as usize;
                    if let Some(&value) = spi.tx.get(1) {
                        self.regs[reg] = value;
                    }
                }
            }
            return None;
        }
        // A read with the command byte in the same transfer is the common
        // shape: tx is exactly one byte and it has the read bit set. Anything
        // longer is the controller's buffer, not a command.
        if spi.tx.len() == 1 && spi.tx[0] & READ != 0 {
            self.command = spi.tx[0];
        }
        let data = (0..spi.rx_len as usize)
            .map(|i| {
                let step = if self.command & MULTI != 0 { i } else { 0 };
                self.register((self.command & 0x3F) as usize + step)
            })
            .collect();
        Some(BusReply {
            correlation_id: request.correlation_id,
            ack: true,
            data,
            op: BusOp::Spi as i32,
        })
    }

    fn state(&self) -> Option<State> {
        Some(
            State::new()
                .set("x", self.g[0])
                .set("y", self.g[1])
                .set("z", self.g[2]),
        )
    }
}

impl Adxl345 {
    /// Register `reg`, wrapping inside the file. `DEVID` is read-only and the
    /// axes are the live values.
    fn register(&self, reg: usize) -> u8 {
        let reg = reg % self.regs.len();
        if reg == DEVID as usize {
            DEVID_VALUE
        } else if (DATAX0 as usize..=0x37).contains(&reg) {
            self.axis_bytes()[reg - DATAX0 as usize]
        } else {
            self.regs[reg]
        }
    }

    /// The six axis bytes, little-endian pairs X, Y, Z, clamped to the part's
    /// ±2 g range.
    fn axis_bytes(&self) -> [u8; 6] {
        let mut bytes = [0u8; 6];
        for (axis, g) in self.g.iter().enumerate() {
            let counts = (g * COUNTS_PER_G).round().clamp(-512.0, 512.0) as i16;
            bytes[axis * 2..axis * 2 + 2].copy_from_slice(&counts.to_le_bytes());
        }
        bytes
    }
}

// Big enough for any read the driver makes, 6 bytes, and for the write a host
// hands over with it.
esp_part::export_part!(Adxl345, buffer = 512);

// The device runs on the host too, so `cargo test` checks it without an
// emulator.
#[cfg(test)]
mod tests {
    use super::*;

    /// Wired as README.md's board wires it: CS on GPIO 7, SCK on 6, MOSI on
    /// 3, MISO on 2.
    fn chip() -> Adxl345 {
        let mut chip = Adxl345::default();
        let pins = [
            (CS.to_string(), 7),
            (SCK.to_string(), 6),
            (MOSI.to_string(), 3),
            (MISO.to_string(), 2),
        ]
        .into();
        chip.configure(&PartConfig { pins, ..Default::default() });
        chip
    }

    fn request(tx: &[u8], rx_len: u32) -> BusRequest {
        let spi = SpiTransfer { bus: 2, tx: tx.to_vec(), rx_len, ..Default::default() };
        BusRequest { correlation_id: 7, op: Some(bus_request::Op::Spi(spi)) }
    }

    /// What the driver's `write_read` gets.
    fn read(chip: &mut Adxl345, command: u8, rx_len: u32) -> Vec<u8> {
        chip.on_bus(&request(&[command], rx_len)).expect("a read is answered").data
    }

    #[test]
    fn devid_is_e5() {
        assert_eq!(read(&mut chip(), 0x80, 1), [0xE5]);
    }

    #[test]
    fn the_axes_are_little_endian_pairs_from_datax0() {
        // Their command: DATAX0 | READ | MULTI.
        let mut chip = chip();
        assert_eq!(read(&mut chip, 0x32 | 0x80 | 0x40, 6), [0x00, 0x00, 0x00, 0x00, 0x00, 0x01]);
        chip.on_input("set:x=0.5");
        chip.on_input("set:y=-1");
        chip.on_input("set:z=0");
        assert_eq!(read(&mut chip, 0xF2, 6), vec![0x80, 0x00, 0x00, 0xFF, 0x00, 0x00]);
    }

    #[test]
    fn a_read_without_the_multi_bit_answers_one_register() {
        let mut chip = chip();
        assert_eq!(read(&mut chip, 0x32 | 0x80, 2), [0x00, 0x00], "the same register twice");
    }

    #[test]
    fn a_write_then_a_read_is_answered_from_the_last_command() {
        // A host may deliver the write and the read as two transfers.
        let mut chip = chip();
        assert_eq!(chip.on_bus(&request(&[0x80], 0)), None, "a write needs no answer");
        let reply = chip.on_bus(&request(&[], 1)).unwrap();
        assert_eq!((reply.ack, reply.data), (true, vec![0xE5]));
    }

    #[test]
    fn the_inputs_move_gravity_and_stop_at_two_g() {
        let mut chip = chip();
        chip.on_input("set:x=1");
        chip.on_input("set:y=9");
        chip.on_input("wiggle");
        let reading = read(&mut chip, 0xF2, 6);
        assert_eq!(reading[..4].to_vec(), [0x00, 0x01, 0x00, 0x02], "1 g, clamped to 2 g");
        chip.on_input("rest");
        assert_eq!(
            read(&mut chip, 0xF2, 6)[4..].to_vec(),
            [0x00, 0x01],
            "back to 1 g on Z"
        );
    }

    #[test]
    fn it_claims_the_chip_select_and_the_bus_pads() {
        let spi = SpiClaim {
            cs_pin: Some(7),
            clk_pin: Some(6),
            mosi_pin: Some(3),
            miso_pin: Some(2),
            ..Default::default()
        };
        assert_eq!(chip().claims(), [Claim { what: Some(claim::What::Spi(spi)) }]);
        // Unwired: claimed without a chip select, which the handshake refuses.
        assert_eq!(
            Adxl345::default().claims(),
            [Claim { what: Some(claim::What::Spi(SpiClaim::default())) }]
        );
    }

    #[test]
    fn it_shows_the_three_axes() {
        let mut chip = chip();
        chip.on_input("set:x=-0.5");
        let shown = chip.state().unwrap();
        assert_eq!(
            (shown.get("x"), shown.get("y"), shown.get("z")),
            (Some("-0.5"), Some("0"), Some("1"))
        );
    }
}
```

**File `esp-emulator/examples/parts/adxl345/README.md`** — copy the shape of
`tmp123/README.md`: what it models (`DEVID`, six axis bytes, read/multi bits),
what it does not (mode 3, 5 MHz, FIFO, interrupts, offset/gain calibration),
its interactions, the build command, and the board snippet below.

**Build it** (through the recipe added in §5):

```sh
just adxl345-part        # -> drivers/adxl345/adxl345.wasm
```

The part's own tests run on the host:

```sh
just adxl345-part-test   # cargo test in esp-emulator/examples/parts/adxl345
```

## 4. The fixture: spec, app, board

**File `drivers/adxl345/adxl345-test.espf.txt`**

```text
====================
ADXL345 test firmware

Fixture for planadxl345.md: the bundled `<adxl345>` driver over SPI, wired to
the virtual ADXL345 part (CS = GPIO 7, SCK = GPIO 6, MOSI = GPIO 3,
MISO = GPIO 2). Prints `TEST:` lines the recipes assert on.
====================

# espforge
  <project name="adxl345_test" />
  <chip type="esp32c3" />
  <runtime type="blocking" />

# peripherals
  <spi id="spi2" sclk="6" mosi="3" miso="2" mode="0" frequency="1000kHz" />

# devices
  <adxl345 id="accel" bus="$spi2" cs="7" />

# services
```

Why nothing else: the manifest supplies mode 3 at 5 MHz and the driver applies
them before each transaction, so the `<spi>` element's own mode and clock are
irrelevant to this one device. `cs` stays a plain pad number; the element
synthesises the chip select from the bus.

**File `drivers/adxl345/src/app.rs`**

```rust
//! Happy-path fixture for the bundled `<adxl345>` driver (planadxl345.md).
//!
//! The driver itself is the generated `accel` device: `Devices::new` builds
//! `espforge_drivers::Adxl345`, which first reads `DEVID` and fails
//! initialization unless it is 0xE5. This app only adds machine-checkable
//! output:
//!
//! - `TEST: adxl345 boot`
//! - `TEST: accel x=0 y=0 z=1024 mg` once a second
//!
//! Pins match `drivers/adxl345/board.json` (CS 7, SCK 6, MOSI 3, MISO 2).
//!
//! The driver reports `counts * 4` mg, while the part converts 1 g to 256
//! counts (the datasheet's 3.9 mg/LSB), so 1 g reads back as 1024 mg. That is
//! the driver's rounding, not a wrong part.

use crate::Context;

/// One reading a second, so an interaction is visible in the next line.
const UPDATE_INTERVAL_MS: u32 = 1_000;

pub fn setup(ctx: &mut Context) {
    ctx.logger.info("TEST: adxl345 boot");
}

pub fn forever(ctx: &mut Context) {
    match device!(ctx, accel).read() {
        Ok(reading) => {
            let (x, y, z) = reading.milli_g();
            ctx.logger
                .info(format_args!("TEST: accel x={x} y={y} z={z} mg"));
        }
        Err(error) => ctx.logger.error(format_args!("TEST: accel read failed: {error:?}")),
    }
    ctx.delay.delay_ms(UPDATE_INTERVAL_MS);
}
```

**File `drivers/adxl345/board.json`**

```json
{ "parts": {
    "accel": { "module": "adxl345.wasm", "pins": { "CS": 7, "SCK": 6, "MOSI": 3, "MISO": 2 } }
} }
```

The pin names are the part's own, upper case. `module` is relative to the
board file, so `adxl345.wasm` sits beside `board.json` in `drivers/adxl345/`.

## 5. `just` recipes

Per `AGENTS.md`, espforge is driven only through `just`. Add one section to
`justfile`, beside the other driver sections:

```make
# ---- ADXL345 virtual part fixture (drivers/adxl345) ----------------------

adxl345_rel := "drivers/adxl345"
adxl345_dir := justfile_directory() / adxl345_rel
adxl345_out := build_dir / "adxl345"
adxl345_port := "127.0.0.1:5562"
adxl345_part_dir := justfile_directory() / "esp-emulator" / "examples" / "parts" / "adxl345"

# The part's own tests: DEVID, the axis bytes and the inputs, on the host.
adxl345-part-test:
    cargo test --manifest-path {{ adxl345_part_dir }}/Cargo.toml

# Build the virtual ADXL345 -> drivers/adxl345/adxl345.wasm.
adxl345-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ adxl345_part_dir }}/Cargo.toml"
    cp "{{ adxl345_part_dir }}/target/wasm32-unknown-unknown/release/adxl345.wasm" \
        "{{ adxl345_dir }}/adxl345.wasm"
    ls -l "{{ adxl345_dir }}/adxl345.wasm"

# Schema and resource graph for the fixture.
adxl345-check:
    just check {{ adxl345_rel }}

# Build the merged image -> build/adxl345/adxl345.bin (+ .elf).
adxl345-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ adxl345_out }}"
    just check "{{ adxl345_rel }}"
    just build "{{ adxl345_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ adxl345_out }}/adxl345.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/adxl345_test" \
        "{{ adxl345_out }}/adxl345.elf"
    ls -l "{{ adxl345_out }}/adxl345.bin" "{{ adxl345_out }}/adxl345.elf"

# Run the firmware with the virtual part on the control channel.
adxl345-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ adxl345_out }}/adxl345.bin --cosim-board {{ adxl345_dir }}/board.json --control-tcp {{ adxl345_port }} --elf {{ adxl345_out }}/adxl345.elf --timeout {{ timeout }} --log-file {{ adxl345_out }}/emu.log"

# One control line, e.g. "part accel input set:x=0.5".
adxl345-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ adxl345_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Happy path: boot, DEVID accepted, flat reads 1 g on Z, then two moves.
adxl345-sensor: adxl345-image adxl345-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ adxl345_out }}/sensor.stdout"
    log="{{ adxl345_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just adxl345-run 90s > "$out" 2>&1 &
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
    wait_for_log() {
        local i
        for i in $(seq 1 "$3"); do
            grep -q "$1" "$log" && { echo "PASS: $2"; return 0; }
            sleep 1
        done
        echo "FAIL: $2 (no '$1'); see $log" >&2
        ls -l "$log" >&2 || true
        return 1
    }
    wait_for "TEST: adxl345 boot" "firmware boots" 40
    wait_for_log "is the SPI device on chip select GPIO 7" "the part attached on the firmware's CS" 10
    wait_for "TEST: accel x=0 y=0 z=1024 mg" "flat part reads 1 g on Z" 20
    just adxl345-part-input "part accel input set:x=0.5" >/dev/null
    wait_for "TEST: accel x=512 y=0 z=1024 mg" "set:x=0.5 reads back as 512 mg" 20
    just adxl345-part-input "part accel input set:z=-1" >/dev/null
    wait_for "TEST: accel x=512 y=0 z=-1024 mg" "set:z=-1 reads back as -1024 mg" 20
    echo "adxl345-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
adxl345-clean:
    rm -rf {{ adxl345_out }}
```

Two details worth keeping:

- The attach line goes to `--log-file`, the firmware console to stdout. The
  recipe asserts each in its own stream, as `tmp123-sensor` does.
- `--cosim-log events.jsonl` (add it to a one-off `just esp-emu` line) records
  every bus request and reply, which is the first place to look when a reading
  is wrong.

## 6. Milestones

1. **Part tests.** `just adxl345-part-test` proves `DEVID`, the little-endian
   axis bytes, auto-increment, and the input clamp on the host. No toolchain,
   no emulator.
2. **Part build.** `just adxl345-part` produces a loadable module and copies it
   beside the board.
3. **Compile gate.** `just adxl345-check` proves the element, bus reference and
   resource graph; `just adxl345-image` proves the generated project compiles
   with the bundled `adxl345` feature and `device!(ctx, accel)`.
4. **Happy path.** `just adxl345-sensor` boots, attaches, reads and moves the
   part.

Expected console:

| Where | Line |
|---|---|
| `build/adxl345/emu.log` | `co-simulation: part accel (adxl345) attached: wired CS=GPIO7 SCK=GPIO6 MOSI=GPIO3 MISO=GPIO2` |
| `build/adxl345/emu.log` | `is the SPI device on chip select GPIO 7` |
| `build/adxl345/sensor.stdout` | `INFO - TEST: adxl345 boot` |
| `build/adxl345/sensor.stdout` | `INFO - TEST: accel x=0 y=0 z=1024 mg` |
| after `set:x=0.5` | `INFO - TEST: accel x=512 y=0 z=1024 mg` |
| after `set:z=-1` | `INFO - TEST: accel x=512 y=0 z=-1024 mg` |

The recipe's exact wording of the attach line comes from `tmp123`'s log; if
the emulator phrases it differently for this part, read `emu.log` once and
match the recipe to it rather than the other way round.

## 7. Reading a failure

| Symptom | Cause | Next step |
|---|---|---|
| `ERROR - device \`accel\`: initialization failed`, no `TEST` lines | The driver's `DEVID` check failed: the part answered something other than `0xE5`, or nothing | Check the module copied, the board's `CS`, and the part's `0x80` read; the driver fails before `setup` |
| Boot prints, then `TEST: accel read failed: Bus` | The transfer itself failed in esp-hal | Compare with `just tmp123-sensor`, which uses the same SPI path and passes; look in `emu.log` |
| Readings are all zero | The part answers `DATAX0` from `regs` instead of the axes | The `DATAX0..=0x37` branch of `register()` must come before the `regs` fallback; rerun `adxl345-part-test` |
| A fresh part reads `z=1000` | Not this part: 1 g is 256 counts and the driver multiplies by 4 | Expect `1024`; `milli_g()` rounds 3.9 mg/LSB to 4 |
| A value moves the wrong axis | The board, the spec pads and the part's pins disagree | The three must agree: `cs`/pad numbers in the spec, `board.json`, and the part's `CS`/`SCK`/`MOSI`/`MISO` |
| Emulator refuses the board at load | A pin name the part does not have, or a pad two parts drive | Names are `CS`, `SCK`, `MOSI`, `MISO` in upper case |
| End-of-run warning: the part never saw a chip select | `cs="7"` in the spec differs from the board's `CS: 7`, or the bus pads are wrong | Make them agree; espforge drives CS as a GPIO, so the pads are how the emulator finds the bus |
| `unexpected element` or `unknown attribute` from `check` | The `<adxl345>` element syntax differs | It is `bus`, `cs`, optionally `mode`, `frequency`, `init`; see the manifest's examples |

## 8. What this does not prove

1. **Mode 3 and 5 MHz.** The part never sees clock polarity, phase or speed.
   A real ADXL345 needs mode 3; the driver applies it, but the emulator cannot
   catch a wiring or timing error here.
2. **The rest of the register map.** `BW_RATE`, `POWER_CTL`, `OFSX/Y/Z`,
   `FIFO_CTL`, interrupts and the self-test bits are storage only. The driver
   reads just `DEVID` and `DATAX0`.
3. **Measurement behaviour.** No noise, no offset, no FIFO, no data-ready
   interrupt, no range change through `DATA_FORMAT`. `±2 g` is assumed.
4. **Async, I2C, `init="deferred"`.** The bundled async variant
   (`adxl345-async`) and the I2C pinout are out of scope; see `repro.md` for
   why I2C is blocked under this emulator anyway.
5. **Real hardware.** The virtual part validates the driver's protocol, not
   the sensor.

## 9. Ordered checklist

- [ ] 1. Create `esp-emulator/examples/parts/adxl345/` (`Cargo.toml`,
      `src/lib.rs`, `README.md`) — §3
- [ ] 2. Add `adxl345-part-test` and `adxl345-part` to `justfile` — §5
- [ ] 3. `just adxl345-part-test` — `DEVID`, axes and inputs pass on the host
- [ ] 4. `just adxl345-part` — `drivers/adxl345/adxl345.wasm` exists
- [ ] 5. Create `drivers/adxl345/adxl345-test.espf.txt` and `src/app.rs` — §4
- [ ] 6. Create `drivers/adxl345/board.json` — §4
- [ ] 7. Add the remaining `justfile` recipes — §5
- [ ] 8. `just adxl345-check` — the element and bus reference lower
- [ ] 9. `just adxl345-image` — the generated project compiles (the real gate)
- [ ] 10. `just adxl345-sensor` — boot, `1024 mg`, `512 mg`, `-1024 mg`, all
      `PASS` lines
- [ ] 11. `drivers/adxl345/README.md` records how to run it

## 10. Verification matrix

| Check | What it proves |
|---|---|
| `just adxl345-part-test` | The part answers `0x80` with `0xE5`, six little-endian axis bytes from `0xF2`, keeps a command across split transfers, and clamps inputs |
| `just adxl345-check` | The bundled `<adxl345>` element, its `cs` and bus reference, and the resource graph are valid |
| `just adxl345-image` | The generated project enables the `adxl345` feature, builds `espforge_drivers::Adxl345` on `ConfiguredSpiDevice`, and exposes `device!(ctx, accel)` |
| `just adxl345-part` | The crate builds to a loadable `adxl345.wasm` for this emulator |
| `just adxl345-sensor` | End to end: the GPIO-driven CS reaches the part, `DEVID` lets initialization pass, the axes are exact, and both `set:` interactions change them |
