# ADXL345: the virtual part

A virtual Analog Devices ADXL345 three-axis accelerometer, written in Rust and
compiled to the WebAssembly module the emulator loads. It lives beside the
fixture that uses it (`drivers/adxl345`) rather than in the vendored
`esp-emulator/` tree, so the source is versioned with this repository. The
built module is `../adxl345.wasm`; `board.json` one level up points at it.

It follows the ADXL345 register protocol as the bundled espforge driver uses
it: a command byte with bit 7 set for a read and bit 6 for auto-increment,
then the answer. See `PART_ABI.md` in `esp-emulator/proto/` for the part
contract.

## What it models

- `DEVID` (`0x00`) answers `0xE5`, so the driver's constructor accepts it.
- The three axes are six bytes from `DATAX0` (`0x32`): X, Y and Z as
  little-endian `i16` counts, auto-incremented when the multi-byte bit is set.
- 1 g is 256 counts (the datasheet's 3.9 mg/LSB at the default ±2 g range),
  and the part starts flat on the table with gravity on +Z, so a fresh part
  reads 1 g.
- Reads are answered from the last command, so a transfer split into a write
  of the command and a separate read works too.
- Every register from `0x00` to `0x3F` answers; only `0x00` and `0x32..=0x37`
  are special. Writes are stored, so `POWER_CTL` and `DATA_FORMAT` keep what
  was written even though the driver never writes.

The part claims the pad on its `CS` pin as its chip select, with `SCK`, `MOSI`
and `MISO` beside it: espforge drives CS as a plain GPIO around each
transaction, so the bus pads are how the emulator finds the bus.

## What it does not model

- Clock polarity, phase or speed (mode 3 at 5 MHz). The emulator cannot see
  them.
- The measurement: no noise, no offset, no gain error, no FIFO, no
  data-ready interrupt, no self-test, no range change through `DATA_FORMAT`.
  ±2 g is assumed.
- The rest of the register map, which is plain storage.

## Interactions

| Name | What it does |
|---|---|
| `set:x=<g>`, `set:y=<g>`, `set:z=<g>` | Gravity along that axis, in g, clamped to ±2 g. |
| `rest` | Flat on the table again: `x=0`, `y=0`, `z=1`. |

`state` reports `x`, `y` and `z` in g, and `set:<axis>=<value>` values read
back under the same names.

## Build it

From the repository root, per `AGENTS.md`:

```sh
just adxl345-part-test   # the part's own tests, on the host
just adxl345-part        # -> drivers/adxl345/parts/adxl345.wasm
```

`adxl345-part` needs Rust's `wasm32-unknown-unknown` target
(`rustup target add wasm32-unknown-unknown`). The `esp-part` dependency is a
path into the vendored `esp-emulator` checkout, which the root `.gitignore`
excludes; the part builds only where that checkout is present.

## Put it on a board

A board file for `esp-emu --cosim-board`, with `CS` on GPIO 7, `SCK` on GPIO
6, `MOSI` on GPIO 3 and `MISO` on GPIO 2. The module path is relative to the
board file:

```json
{ "parts": { "accel": { "module": "parts/adxl345.wasm", "pins": { "CS": 7, "SCK": 6, "MOSI": 3, "MISO": 2 } } } }
```

The pin names are the part's own, upper case. A part with no `CS` is refused
when the board loads.

## What is in it

| File | What it does |
|---|---|
| `Cargo.toml` | A `cdylib` crate with its own workspace: it builds to a `.wasm` module. |
| `src/lib.rs` | The device (`Adxl345`) as an `esp_part::Device`, its `on_bus` answers, and the `export_part!` line that makes it a part. |
