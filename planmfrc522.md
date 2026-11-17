# `planmfrc522.md`: an MFRC522 virtual part and a project-local driver

**Goal:** `drivers/mfrc522/` holds a virtual NXP MFRC522 reader part, a
project-local espforge `<mfrc522>` driver, and a blocking ESP32-C3 fixture. The
fixture boots, the driver accepts `VersionReg` = 0x92, a virtual MIFARE Classic
1K card is selected, and the UID reads back as `DEADBEEF` with `SAK` 0x08.
Interactions remove the card and give it another UID. No hardware, no changes
to `espforge/`.

**Scope:** the main happy path only. 4-wire SPI, one reader on one bus, one
card at a time, blocking runtime, one poll a second. No crypto1 authentication,
no block reads or writes, no anticollision loop, no UID clashes, no I2C/UART
host interface, no reset pin, no real-hardware validation. `planbme280_spi.md`
is the precedent for a project-local driver plus a part under `drivers/`; this
document carries only what the MFRC522 needs.

**Current state:** `drivers/mfrc522/` does not exist (checked). espforge has no
bundled `<mfrc522>` element, so this plan creates the driver, the part and the
fixture. The built `parts/mfrc522.wasm` will come from `just mfrc522-part`.

## 0. Baseline measured against the code, not assumed

| Fact | Where it comes from |
|---|---|
| espforge has **no bundled MFRC522 driver**: the manifests are `adxl345`, `ili9341`, `scd4x`, `ssd1306`, `ads111x`, `bme680`, `sht31`, `sht4x`, `mhz19b` | `espforge/espforge-drivers/manifests/` |
| A project-local driver is a manifest under `<project>/drivers/*.kdl` plus a crate beside it, named by `dependency "…" { path "../…" }`, with no `codegen.feature` | `drivers/bme280_spi/drivers/bme280_spi.kdl`, `drivers/bme280_spi/bme280-spi-driver/` |
| A part's source can live in the fixture folder, with `esp-part` at `../../../../esp-emulator/esp-part` | `drivers/bme280_spi/parts/bme280_spi/Cargo.toml` |
| Parts are `cdylib` crates over `esp-part`, built for `wasm32-unknown-unknown`, exported with `export_part!` | `esp-emulator/esp-part/README.md` |
| A SPI part gets one `part_on_bus` per transfer: `tx` is the data phase clocked out, `rx_len` what the master reads | `esp-emulator/proto/PART_ABI.md`, "On SPI" |
| A part that claims a GPIO-driven CS must also name its clock/data pads so the emulator finds the bus | `PART_ABI.md`, "On SPI"; `drivers/bme280_spi/parts/bme280_spi/src/lib.rs` |
| The MFRC522 wire address is one byte, `(register << 1)`, with bit 7 set for a read and bit 0 always 0; **no dummy byte**, reads auto-increment | MFRC522 datasheet, serial interface and register overview |
| `VersionReg` is `0x37`: `0x92` on a v2.0 part, `0x91` on some clones | datasheet, version register |
| Soft reset is `CommandReg` (`0x01`) = `0x0F`; it clears the register file and leaves the version | datasheet, command register / soft reset |
| Commands used: `Transceive` = `0x0C`, `Idle` = `0x00` | datasheet, command register table |
| `TxControlReg` (`0x14`) powers up `0x80`; bits `0x03` switch the antenna drivers on | datasheet, `TxControlReg` reset value |
| `FIFODataReg` (`0x09`) is a push/pop port; `FIFOLevelReg` (`0x0A`) is the byte count | datasheet, FIFO |
| `ComIrqReg` (`0x04`): `RxIRq` `0x20`, `IdleIRq` `0x10`, `TimerIRq` `0x01` | datasheet, `ComIrqReg` |
| A MIFARE Classic answers `REQA` `0x26` (7 bits, `BitFramingReg` `0x0D` = `0x07`) with a 2-byte ATQA; anticollision CL1 `0x93 0x20` returns UID0..3 + BCC; select CL1 `0x93 0x70` + UID + BCC + CRC_A returns the SAK | ISO/IEC 14443-3, REQA / anticollision / select; MIFARE Classic 1K datasheet |
| CRC_A: init `0x6363`, polynomial `0x8408`, LSB-first, result complemented | ISO/IEC 14443-3, CRC_A |
| espforge drives CS as a plain GPIO around each transaction, not the controller's CS signal | `espforge/espforge-runtime/src/hw/spi.rs`, `ConfiguredSpiDevice` |
| ESP32-C3 pads used by the existing SPI fixtures: SCK 6, MOSI 3, MISO 2, CS 7 | `drivers/bme280_spi/bme280_spi-test.espf.txt`, `drivers/adxl345/board.json` |
| Every espforge call goes through a `justfile` recipe | root `AGENTS.md` |
| All blocks are ESP32-C3 only | root `AGENTS.md` |

## 1. Why the driver is project-local

There is no bundled `<mfrc522>` element, and the published MFRC522 crates
(`mfrc522`, `mfrc522-hal`, …) are written against `embedded-hal` 0.2 and a
`DelayNs`-plus-reset-pin constructor, which does not fit espforge's manifest
slots. Copying `bme280_spi`'s shape keeps the whole protocol in this repository:

- `drivers/mfrc522/drivers/mfrc522.kdl` names the crate by `path`;
- `drivers/mfrc522/mfrc522-driver/` is generic over
  `embedded_hal::spi::SpiDevice`, so espforge's `ConfiguredSpiDevice`, the
  driver's host tests and a future real board take the same code;
- the manifest's `codegen.runtime` instantiates
  `mfrc522_driver::Mfrc522<espforge_runtime::ConfiguredSpiDevice>`.

When a published crate fits, only the `dependency` block and the `type` /
`constructor` lines change; the part and the firmware stay as they are.

## 2. Layout

```text
drivers/mfrc522/
  mfrc522-test.espf.txt             # the espforge project
  src/app.rs                        # firmware application half
  board.json                        # CS 7, SCK 6, MOSI 3, MISO 2
  drivers/mfrc522.kdl               # the project-local driver manifest
  mfrc522-driver/
    Cargo.toml                      # generic embedded-hal driver crate
    src/lib.rs
  parts/mfrc522/
    Cargo.toml                      # the virtual part
    src/lib.rs
  parts/mfrc522.wasm                # built by `just mfrc522-part`
  (git-ignored) build output lives in build/mfrc522, not here
```

The part's `esp-part` dependency points into the vendored `esp-emulator/`
checkout, which the root `.gitignore` excludes, exactly like
`drivers/bme280_spi/parts/bme280_spi`.

## 3. The virtual part

`parts/mfrc522/src/lib.rs` models the reader's register file, its FIFO and the
RF exchange with one card. The card is the part's state, not the register file:

| Field | Meaning |
|---|---|
| `present` | whether a card is in the field; starts `true` |
| `uid: [u8; 4]` | the card's UID; starts `DE AD BE EF` |
| `atqa: [u8; 2]` | `04 00` |
| `sak: u8` | `08` (MIFARE Classic 1K) |

Registers are a 64-byte array, with these exceptions:

| Register | Behaviour |
|---|---|
| `0x01` `command` | writes drive the exchange: `0x0F` soft-resets, `0x0C` transceives the FIFO contents, `0x00` idles |
| `0x04` `com_irq` | set by the exchange: `0x30` (`RxIRq \| IdleIRq`) on an answer, `0x11` (`TimerIRq \| IdleIRq`) when no card answers; cleared on read |
| `0x06` `error` | `0x00` on the happy path |
| `0x09` `fifo_data` | writes push, reads pop and advance the level |
| `0x0A` `fifo_level` | the current FIFO byte count |
| `0x0D` `bit_framing` | TxLastBits in the low three bits; `0x07` makes `REQA` a 7-bit frame |
| `0x14` `tx_control` | powers up `0x80`; the driver sets `0x03` to enable the antenna, stored only |
| `0x21`/`0x22` `crc_result` | `CRC_A` of the last transceived frame, high byte first |
| `0x37` `version` | read-only `0x92` |
| `0x2A`..`0x2F`, `0x11`, `0x15` | the driver's timer/mode/ASK writes, plain storage |
| everything else | stored register file |

The emulator delivers a read as one transaction whose `tx` is the address byte
and whose `rx_len` is the answer length, and a write as `[address, value]` with
`rx_len == 0`; reads auto-increment from the address. The part therefore:

- serves `VersionReg` before any command, so the driver's identity check runs
  against a part that has not been configured yet;
- builds the answer when `Transceive` is written, from the FIFO bytes and
  `BitFramingReg`, and leaves it in the FIFO for the driver to drain;
- keeps `CRCResultReg` in step with the answer, so the driver's `CRC_A` check
  has something to compare against.

The exchange itself is the three frames of a card selection:

| FIFO frame | `TxLastBits` | Answer |
|---|---|---|
| `26` (`REQA`) | 7 | `04 00` (ATQA) |
| `93 20` (anticollision CL1) | 0 | `uid[0..4]` + BCC, BCC = XOR of the four UID bytes |
| `93 70` + UID + BCC + CRC_A (select CL1) | 0 | `08` (SAK) + CRC_A |

With no card in the field, every frame leaves the FIFO empty and sets
`TimerIRq`, which is what the driver reads as "no tag".

Inputs:

| Name | What it does |
|---|---|
| `present` | a card appears with the current UID |
| `absent` | the card is removed; the field is empty |
| `set:uid=<hex>` | e.g. `set:uid=01:02:03:04`, exactly four bytes |
| `rest` | back to the defaults: card present, UID `DE AD BE EF` |

The part's own tests (`just mfrc522-part-test`) run on the host and check the
version, the soft reset, FIFO push/pop and level, the exact three frames, the
BCC and SAK, the no-card `TimerIRq`, the `set:uid` and `absent` inputs, the
`CRCResultReg` bytes, the SPI claim and the state view.

## 4. The driver

`mfrc522-driver/src/lib.rs`:

- `Mfrc522::new(spi)` soft-resets (`CommandReg = 0x0F`), reads `VersionReg`,
  refuses anything but `0x92` or `0x91` with `WrongVersion(v)`, applies the
  standard init (`TModeReg = 0x8D`, `TPrescalerReg = 0x3E`,
  `TReloadReg = 0x001E`, `TxASKReg = 0x40`, `ModeReg = 0x3D`), switches the
  antenna on with a read-modify-write of `TxControlReg |= 0x03`, and clears
  `Status2Reg` (`0x08`);
- `read_tag()` runs `REQA` then anticollision CL1 then select CL1, and returns
  `Ok(Some(Tag { atqa, uid, sak }))`, or `Ok(None)` when the FIFO stays empty
  and `TimerIRq` is set;
- one frame is: clear the FIFO (`FIFOLevelReg = 0x80`), push the bytes
  (`FIFODataReg`), set `BitFramingReg`, write `CommandReg = 0x0C`, poll
  `ComIrqReg` for `RxIRq | TimerIRq` with a bounded retry count, then read
  `ErrorReg` and `FIFOLevelReg` and drain that many bytes from `FIFODataReg`;
- every transfer is `SpiDevice::transaction`; a register read is
  `[reg << 1 | 0x80]` plus `Operation::Read`, a register write is
  `[reg << 1, value]`, so the address byte carries the read bit and no dummy
  byte is on the wire;
- `CRC_A(frame)` is the ISO/IEC 14443-3 implementation (init `0x6363`, poly
  `0x8408`, complemented); the driver appends it to the select frame and checks
  the card's reply;
- the error type is `Mfrc522Error<E>`: `Bus(E)`, `WrongVersion(u8)`, `Short`,
  `Bcc(u8)`, `Crc`. espforge maps an external constructor's error to
  `DeviceErrorKind::Init`, so a wrong part fails at boot with the device named.

The driver's tests (`just mfrc522-driver-test`) use a register-file
`SpiDevice` double: the version check accepts `0x92` and rejects `0x00`, the
soft reset and init writes land on the right addresses, `TxControlReg` is
`0x83` after the antenna read-modify-write, the three frames and their
`CRC_A`/BCC checks decode to `DEADBEEF` / SAK `0x08` / ATQA `0x0004`, and an
empty FIFO with `TimerIRq` reads as no tag.

## 5. The espforge project

`mfrc522-test.espf.txt`:

```text
# espforge
  <project name="mfrc522_test" />
  <chip type="esp32c3" />
  <runtime type="blocking" />

# peripherals
  <spi id="spi2" sclk="6" mosi="3" miso="2" mode="0" frequency="1000kHz" />

# devices
  <mfrc522 id="reader" bus="$spi2" cs="7" />
```

`src/app.rs` logs `TEST: mfrc522 boot` once, then once a second either

```text
TEST: tag uid=deadbeef sak=0x08 atqa=0x0004
```

or `TEST: tag none` when the card is away. Absence is not an error: the reader
initializes from `VersionReg` alone, so a missing card keeps the firmware
running and the recipe can watch the line change.

`board.json` wires the part's own pin names:

```json
{ "parts": {
    "reader": { "module": "parts/mfrc522.wasm", "pins": { "CS": 7, "SCK": 6, "MOSI": 3, "MISO": 2 } }
} }
```

The four names must agree: `cs="7"` and the `<spi>` pads in the spec,
`board.json`, and the part's `CS`/`SCK`/`MOSI`/`MISO` constants.

## 6. `just` recipes

Everything goes through the root `justfile`, in a section beside
`bme280-spi-*`, with `mfrc522_out := build_dir / "mfrc522"` and the control
channel on `127.0.0.1:5564`:

| Recipe | What it proves |
|---|---|
| `just mfrc522-part-test` | the part's version, FIFO, frames, BCC/SAK, no-card IRQ and inputs, on the host |
| `just mfrc522-part` | builds `parts/mfrc522.wasm` |
| `just mfrc522-driver-test` | the driver's version check, init writes, frames and CRC, on the host |
| `just mfrc522-check` | the manifest is discovered, lowers, and its `path` dependency resolves |
| `just mfrc522-image` | the generated project compiles → `build/mfrc522/mfrc522.bin` |
| `just mfrc522-run` | boots with the control channel on `127.0.0.1:5564` |
| `just mfrc522-part-input "part reader input absent"` | one control line |
| `just mfrc522-sensor` | the whole happy path |
| `just mfrc522-driver-record` | re-records the manifest lock after editing the KDL |
| `just mfrc522-clean` | removes `build/mfrc522` and the part's `target/` |

Editing the manifest, even a comment, makes the next `check` report
`DRIVER_LOCK_STALE`; `mfrc522-driver-record` fixes it. Editing the crate,
`src/app.rs`, the spec or `board.json` needs no re-record.

## 7. Milestones and expected output

1. `just mfrc522-part-test` — the part passes on the host.
2. `just mfrc522-part` — `parts/mfrc522.wasm` exists.
3. `just mfrc522-driver-test` — the driver passes on the host.
4. `just mfrc522-check` — the element and bus reference lower.
5. `just mfrc522-image` — the generated project compiles.
6. `just mfrc522-sensor` — boot, attach, select, remove, re-UID:

```text
PASS: firmware boots
PASS: the part attached on the firmware's CS
PASS: tag DEADBEEF selected with SAK 0x08
PASS: absent -> no tag
PASS: set:uid=01020304 -> 01020304
mfrc522-sensor PASS
```

The attach line is in `build/mfrc522/emu.log`; the firmware console is
`build/mfrc522/sensor.stdout`:

```text
co-simulation: part reader (mfrc522) attached: wired CS=GPIO7 MISO=GPIO2 MOSI=GPIO3 SCK=GPIO6; drives no pin; is the SPI device on chip select GPIO 7
```

## 8. Reading a failure

| Symptom | Cause | Next step |
|---|---|---|
| `initialization failed`, no `TEST` lines | the driver's `VersionReg` check failed; the part answered something other than 0x92/0x91, or nothing | confirm `parts/mfrc522.wasm` was copied, then check `mfrc522-part-test` |
| Boot prints, then `TEST: tag read failed` | the transfer itself failed in esp-hal | compare with `just bme280-spi-sensor`, which uses the same SPI path and passes |
| Always `TEST: tag none` with a card present | the exchange never produced an answer: `BitFramingReg` is not 7 for `REQA`, or the command write is not `0x0C` | `just mfrc522-part-test` checks the three frames and the IRQ bits |
| `WrongVersion` in the boot line | the register file answers the version from storage, not the constant | `VersionReg` must be answered before the `regs` fallback |
| UID comes back as four zeros or a wrong BCC | the FIFO still holds the previous frame, or BCC is not the XOR of all four bytes | clear the FIFO first (`FIFOLevelReg = 0x80`) and rerun `mfrc522-driver-test` |
| End-of-run warning: the part never saw a chip select | the spec's `cs="7"` differs from the board's `CS: 7`, or the bus pads are wrong | make all four agree |
| `unexpected element` or `unknown attribute` from `check` | the `<mfrc522>` element syntax differs, or the manifest was not discovered | the tag is the manifest's `driver "mfrc522"`; the nested `drivers/` folder is the tier |
| `DRIVER_LOCK_STALE` | the KDL text changed | `just mfrc522-driver-record` |
| Emulator refuses the board | a pin name the part does not have, or a pad two parts drive | names are `CS`, `SCK`, `MOSI`, `MISO` in upper case |

## 9. What this does not prove

1. **RF timing and the field.** The part answers whatever the FIFO holds; it
   models no carrier, no 13.56 MHz field, no modulation depth and no `RFCfgReg`
   gain. `TxControlReg` and `TxASKReg` are storage.
2. **SPI timing.** Mode 0 at 1 MHz is declared, but the emulator does not model
   polarity, phase or speed; the MFRC522 accepts up to 10 MHz, and that is not
   exercised.
3. **Crypto1.** No `MFAuthent`, no key, no sector access. `Mifare` block reads
   and writes, the MIFARE Classic 1K memory and its access bits are out of
   scope; selection stops at the SAK.
4. **Anticollision and UID clashes.** One card, no cascaded levels (`0x95`,
   `0x97`), no 7-byte or 10-byte UID, no `CollReg` bits, no `HALT`/wake cycle
   beyond what the driver sends.
5. **The timer and IRQ model.** `TModeReg`, `TPrescalerReg`, `TReloadReg` and
   `ComIrqReg`'s mask registers are storage plus a fixed answer; no real
   timeout counting, no `DivIrq`, no `TimerIRq` timing.
6. **The reset pin.** The driver uses the SPI soft reset only; a board with
   `RST` on a GPIO is not modeled, and no extra pin is claimed.
7. **I2C and UART host interfaces, and real hardware.** Only 4-wire SPI is
   modeled; the virtual part validates the protocol and the frame arithmetic,
   not the reader or a particular card.

## 10. Ordered checklist

- [ ] 1. Create `drivers/mfrc522/parts/mfrc522/` (`Cargo.toml`, `src/lib.rs`) — §3
- [ ] 2. Create `drivers/mfrc522/mfrc522-driver/` (`Cargo.toml`, `src/lib.rs`) — §4
- [ ] 3. Create `drivers/mfrc522/drivers/mfrc522.kdl` — §1, §4
- [ ] 4. Create `drivers/mfrc522/mfrc522-test.espf.txt`, `src/app.rs`, `board.json` — §5
- [ ] 5. Add the `mfrc522-*` section to `justfile` — §6
- [ ] 6. `just mfrc522-part-test` — the part passes on the host
- [ ] 7. `just mfrc522-part` — `drivers/mfrc522/parts/mfrc522.wasm` exists
- [ ] 8. `just mfrc522-driver-test` — the driver passes on the host
- [ ] 9. `just mfrc522-driver-record` then `just mfrc522-check` — the element and bus reference lower
- [ ] 10. `just mfrc522-image` — the generated project compiles
- [ ] 11. `just mfrc522-sensor` — all `PASS` lines above
- [ ] 12. `drivers/mfrc522/README.md` records how to run it

## 11. Verification matrix

| Check | What it proves |
|---|---|
| `just mfrc522-part-test` | version 0x92, soft reset, FIFO push/pop and level, `REQA`/anticollision/select answers, BCC, SAK, `CRCResultReg`, the no-card `TimerIRq`, `set:uid`/`absent`, the SPI claim and the state view |
| `just mfrc522-driver-test` | the version check, the soft reset and init writes, the antenna read-modify-write, the three frames with `CRC_A`, and the no-tag path |
| `just mfrc522-check` | the project-local `<mfrc522>` element, its `cs` and bus reference, and the resource graph |
| `just mfrc522-image` | the generated project names the local crate by path and compiles `Mfrc522<ConfiguredSpiDevice>` with `device!(ctx, reader)` |
| `just mfrc522-sensor` | the whole path: boot, attach on GPIO 7, `DEADBEEF` / SAK 0x08 / ATQA 0x0004, the card removed, and a new UID |
