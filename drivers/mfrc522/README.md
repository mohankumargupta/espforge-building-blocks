# MFRC522 with an external (project-local) driver

An espforge project whose `<mfrc522>` driver is **external** to
`espforge-drivers`: the wiring lives in `drivers/mfrc522.kdl`, a project-local
manifest, and the Rust half is the `mfrc522-driver` crate beside it. It runs
against the virtual MFRC522 part in `parts/mfrc522/`, so the same generated
firmware talks to `mfrc522.wasm` with no reader code glued in by hand.

Everything goes through the root `justfile`, per `AGENTS.md`. The firmware is
ESP32-C3 only. `planmfrc522.md` is the plan and the reasoning.

## What is here

| File | What it is |
|---|---|
| `mfrc522-test.espf.txt` | the espforge project: SPI2 on SCK 6 / MOSI 3 / MISO 2, `<mfrc522 id="reader" bus="$spi2" cs="7" />` |
| `src/app.rs` | selects a card once a second and logs `TEST: tag uid=… sak=… atqa=…`, or `TEST: tag none` |
| `drivers/mfrc522.kdl` | **the external driver**: `dependency "mfrc522-driver" { path "../mfrc522-driver" }`, no `codegen.feature` |
| `mfrc522-driver/` | the driver crate, generic over `embedded_hal::spi::SpiDevice` |
| `board.json` | wires the part's `CS` to GPIO 7, `SCK` to 6, `MOSI` to 3, `MISO` to 2 |
| `parts/mfrc522/` | the virtual reader's source (`Cargo.toml`, `src/lib.rs`) |
| `parts/mfrc522.wasm` | the built virtual reader, from `just mfrc522-part` |
| `.espforge/drivers.lock` | written by the first `check`/`build`; re-record with `just mfrc522-driver-record` |

The spec sits directly in this folder, so the project tier is
`drivers/mfrc522/drivers/mfrc522.kdl` — the nested `drivers/` is the tier, not a
typo. Nothing in `espforge/` was changed.

The part's `esp-part` dependency points into the vendored `esp-emulator/` tree,
which the root `.gitignore` excludes, exactly like
`drivers/bme280_spi/parts/bme280_spi`.

## Try it in one command

```sh
just mfrc522-sensor
```

Builds the firmware and the virtual part, boots the emulator, and prints:

```text
PASS: firmware boots
PASS: the part attached on the firmware's CS
PASS: tag DEADBEEF selected with SAK 0x08
PASS: absent -> no tag
PASS: set:uid=01020304 -> 01020304
mfrc522-sensor PASS
```

## The steps one at a time

```sh
just mfrc522-part-test     # version, soft reset, FIFO, the three frames, BCC/SAK, the inputs
just mfrc522-part          # -> parts/mfrc522.wasm
just mfrc522-driver-test   # the version check, the init writes, the frames and CRC_A
just mfrc522-check         # the element and its bus reference lower
just mfrc522-image         # the generated project compiles -> build/mfrc522/mfrc522.bin
just mfrc522-run           # run with the control channel on 127.0.0.1:5564
```

In a second terminal, while `mfrc522-run` is going:

```sh
just mfrc522-part-input "part list"
just mfrc522-part-input "part reader state"
just mfrc522-part-input "part reader input absent"
just mfrc522-part-input "part reader input set:uid=01:02:03:04"
just mfrc522-part-input "part reader input present"
just mfrc522-part-input "part reader input rest"
```

After editing `drivers/mfrc522.kdl` — even a comment — the lock is stale and the
next `check` says `DRIVER_LOCK_STALE`. Re-record it:

```sh
just mfrc522-driver-record
```

Editing the crate, `src/app.rs`, the `.espf.txt` or `board.json` needs no
re-record: only the manifest text is pinned.

## Reading the wire

The reader's SPI protocol is one address byte per transfer: `register << 1`,
bit 7 set for a read, bit 0 always 0, and **no dummy byte**; a read
auto-increments across the register file. The generated `ConfiguredSpiDevice`
drives CS as a plain GPIO output around each transaction, which is the path
this board exercises.

The driver soft-resets through `CommandReg` (0x0F), checks `VersionReg` (0x37)
for 0x92 or 0x91, applies the standard init (`TModeReg`, `TPrescalerReg`,
`TReloadReg`, `TxASKReg`, `ModeReg`) and switches the antenna on with
`TxControlReg |= 0x03`.

A poll is the three frames of an ISO/IEC 14443-3 selection, with the CRC_A the
driver appends and checks (`crc_a_matches_the_published_frame_vectors` pins the
arithmetic to the published `50 00 57 CD` HALT):

| Frame | `BitFramingReg` | Part answers |
|---|---|---|
| `26` (`REQA`) | 7 | `04 00` (ATQA) |
| `93 20` (anticollision CL1) | 0 | `DE AD BE EF 22` (UID + BCC) |
| `93 70` + UID + BCC + CRC_A | 0 | `08 B6 DD` (SAK + CRC_A) |

The attach line is in `build/mfrc522/emu.log` (the firmware console is
`build/mfrc522/sensor.stdout`):

```text
co-simulation: part reader (mfrc522) attached: wired CS=GPIO7 MISO=GPIO2 MOSI=GPIO3 SCK=GPIO6; drives no pin; is the SPI device on chip select GPIO 7
```

## Limits

- Happy path only: 4-wire SPI, one reader on one bus, one card at a time,
  blocking runtime, one poll a second.
- The part models the register file, the FIFO and the RF exchange, not the
  field: no carrier, no 13.56 MHz, no modulation depth, no `RFCfgReg` gain.
  `TxControlReg` and `TxASKReg` are storage.
- SPI mode 0 at 1 MHz is declared; the emulator does not model polarity, phase
  or speed.
- No crypto1: no `MFAuthent`, no key, no block reads or writes.
- No anticollision loop or UID clashes: one card, a four-byte UID, no cascaded
  levels (0x95, 0x97), no `CollReg`.
- The timer and mask registers are storage plus a fixed `ComIrqReg` answer; no
  timeout counting and no `DivIrq`.
- No reset pin: the SPI soft reset only. I2C and UART host interfaces and real
  hardware are out of scope.

## Cleanup

`build/` is disposable; the fixture and the checked-in wasm stay:

```sh
just mfrc522-clean
```
