# `manifests.md`: which manifests could be manifest-only

**Question:** of the 29 driver manifests in `manifests/`, which ones that are
*not* manifest-only could be written as manifest-only drivers — and for the
rest, what blocks it?

**Answered:** 2026-11-21, against `espforge` 0.9.2 and the crates.io/docs.rs
state of that date. This is an analysis only: no manifest, crate or recipe was
changed.

## 1. What "manifest-only" means here

Manifest-only does not mean "no Rust". It means the Rust the element needs is a
published crate the manifest names, so the `.kdl` file is the whole driver
definition (`espforge/docs/002-external-driver-manifest.md` §5–6,
`plantmp123.md` §1). Concretely the manifest fills three slots:

- `type "..."` — the concrete field type, over the endpoint `{bus_device}`
  produces;
- `constructor "..."` — one plain call, and on a bus device its **first
  argument is `{bus_device}`**;
- `endpoint "..."` — the bus type, only when `init="deferred"` is offered.

The dependency is a `version` (or `git`+`rev`), not a `path` to a crate beside
the fixture. `codegen.feature` stays absent.

**The test used below.** A manifest can become manifest-only when all four hold:

| # | Requirement | Fails when |
|---|---|---|
| 1 | A published crate implements the part's protocol | no crate, or only a Linux/`i2cdev` or other-part crate |
| 2 | Its API is the runtime's `embedded-hal` 1.0 (`I2c`, `SpiDevice`, `DelayNs`) | the crate is `embedded-hal` 0.2/`nb`-based |
| 3 | The field type is nameable and built by one plain call, bus first | builder chain, typestate, address/`Option` argument the manifest cannot spell |
| 4 | Nothing after construction has to rebind the value | `new()` returns `Uninit` and `init(self)` returns another type |

`then { call "..." }` covers a post-construction call that only needs values the
constructor already takes and does not change the type (§5.2); it does not cover
rebinding, which is the glue case in `espforge/driverglue.md`.

## 2. Inventory

All 29 manifests are one transport, one `codegen` block and (except the three
busless ones) the implicit `address`/`cs` plus the `init` attribute — the shape
is uniform, so the verdicts below are about the crate, not the KDL.

**Already manifest-only (3)** — `dependency { version ... }`, no crate beside
the fixture:

| Manifest | Crate | Bus |
|---|---|---|
| `hd44780.kdl` | `i2c-character-display` 0.6.0 | i2c, `new_with_address` |
| `max7219.kdl` | `max7219` 0.5.0 | none, `from_pins` |
| `nxp74hc595.kdl` | `drive_74hc595` 0.2.0 | none, `new` |

**Not manifest-only (26)** — `dependency { path "../<tag>-driver" }`, with a
100–577 line local crate implementing the protocol (`wc -l
drivers/*/*-driver/src/lib.rs`). Those 26 split 6 / 7 / 13:

### 2a. Convertible today (6)

The published crate already has the right shape; only the `dependency` line,
`type`, `constructor` and the description prose change.

| Manifest | Crate | Constructor that fits | Note |
|---|---|---|---|
| `aip31068.kdl` | `i2c-character-display` 0.6.0 | `CharacterDisplayAIP31068::new({bus_device}, LcdDisplayType::Lcd16x2, espforge_runtime::Delay::new())` | the adapter takes **no address** (the controller is fixed at 0x3E), so the constructor drops `{address}`; the current `address allowed="0x3e..=0x3e"` can stay as it is. Same crate, and the same endpoint/delay shape, that `hd44780.kdl` already proves |
| `bh1750.kdl` | `bh1750` 0.1.0 | `BH1750::new({bus_device}, {delay}, false)` | bus first, `{delay}` via the standard `map "delay"` trick, third argument a fixed literal |
| `sgp30.kdl` | `sgp30` 1.0.0 | `Sgp30::new({bus_device}, {address}, {delay})` | verified `new(i2c: I2C, address: u8, delay: D)`; `init()` stays an `app.rs` call |
| `mcp9808.kdl` | `mcp9808` 0.4.0 | `MCP9808::new({bus_device})` | the crate hardcodes 0x18 and takes no address, so the element must narrow from `address allowed="0x18..=0x1F"` to `fixed="0x18"` (the example already writes `address="0x18"`); the dependency's `embedded-hal ^1.0.0-rc.1` requirement resolves to 1.0 stable |
| `bme280_i2c.kdl` | `bme280` 0.5.1 | `bme280::i2c::BME280::new({bus_device}, {address})` + `then { call "init(&mut espforge_runtime::Delay::new())" }` | `new` is infallible (`fallible #false`); `init` applies 2x T / 16x P / IIR 16, so the fixture's expected readings may move |
| `bme280_spi.kdl` | `bme280` 0.5.1 | `bme280::spi::BME280::new({bus_device})` + the same `then` step | verified `pub fn new(spi: SPI) where SPI: SpiDevice` — espforge's `ConfiguredSpiDevice` is exactly that |

Two caveats that apply to the pair above:

- The `then` step has to be a **hole-free literal** (`init(&mut
  espforge_runtime::Delay::new())`), because the constructor takes no delay and
  a step may only use holes the constructor already takes. Confirm the emitter
  accepts a literal expression in `then` before writing it.
- `bme280_i2c` and `bme280_spi` could instead become **one** `transport`
  manifest (`<bme280 bus="$i2c0" address=... />` / `<bme280 bus="$spi2" cs=... />`,
  doc §7), but that renames both tags and edits both `.espf` files, so it is not
  a plain per-manifest conversion.

### 2b. Blocked by one knowable thing (4)

The crate is otherwise right (embedded-hal 1.0, real driver), but the
constructor cannot be written into today's `codegen`:

| Manifest | Crate | Blocker |
|---|---|---|
| `aht20.kdl` | `aht20-driver` 2.0.0 | `AHT20::new(i2c, SENSOR_ADDRESS)` returns an *uninitialised* value; `init(&mut delay)` **consumes it and returns the ready type**. `type` cannot name the post-init type while the constructor returns the pre-init one, and `then` cannot rebind the field. This is the `icm20948` glue shape in `driverglue.md` |
| `mfrc522.kdl` | `mfrc522` 0.8.0 | `Mfrc522::new(comm)` → `Mfrc522<COMM, Uninitialized>`, then `init(self)` → `Mfrc522<COMM, Initialized>`. Same typestate rebinding; also `embedded-hal` 0.2 is still an optional dependency of the crate |
| `ccs811.kdl` | `embedded-ccs811` 1.0.0 | `Ccs811::new(i2c, address, nwake, delay)`, then `start_application()` returns a different mode type. Also `nwake` is a pin the manifest cannot hand as present/absent |
| `pcf857x.kdl` | `pcf857x` 0.5.0 | `Pcf8574::new(i2c, SlaveAddr)` takes an address **enum**; the manifest's `{address}` is a number and there is no `map` for a bus attribute, so `0x20..=0x27` cannot become `SlaveAddr::Default`/`Alternate(..)` |

### 2c. One docs check away (3)

The candidate crate is `embedded-hal` 1.0 and covers the part, but its
constructor was not verifiable in this pass. Each needs one docs.rs read before
it can be called convertible — they are **not** counted as "yes":

| Manifest | Candidate | What decides it |
|---|---|---|
| `pca9685.kdl` | `pwm-pca9685` 1.0.0 | whether `PCA9685::new` takes `address: u8` or a `SlaveAddr`-style enum (the family is inconsistent: `sgp30` uses `u8`, `pcf857x` uses an enum) |
| `ds3231.kdl` | `ds323x` 0.7.0 | same address question, plus which of `Ds3231::new` / `new_ds3231` is the plain call and whether the `rtcc` traits need `alloc`-free `then` steps |
| `hdc302x.kdl` | `hdc302x` 0.5.0 | constructor order and which feature (`sync`/`async`) must be enabled in `dependency { features ... }` |

### 2d. Not convertible (13)

| Manifest | Why not |
|---|---|
| `tmp123.kdl` | no TMP121/TMP123 crate exists (checked in `plantmp123.md` §1 and re-checked: nothing on crates.io) |
| `as7262.kdl` | no crate — `crates.io` search for `as7262` returns 0 results |
| `bmxx80.kdl` | no crate named `bmxx80` (crates.io search empty, docs.rs 404). The local crate is what adds BMP280/BME280 handling on one tag |
| `tca95xx.kdl` | no crate for the *family*; the per-variant crates (`tca9539`, `tca9555`, …) have different register maps, so one manifest cannot name one crate for an `0x20..=0x77` tag |
| `ads1x15.kdl` | `ads1x15` 0.5.0 is a 2018 Linux `i2cdev`/tokio crate, not embedded-hal; `ads1x1x` 0.2.0 is `embedded-hal` 0.2 + `nb` |
| `am2320.kdl` | `am2320` 0.2.0 declares `embedded-hal ^0.2` — the type cannot take espforge's 1.0 endpoint |
| `ht16k33.kdl` | `ht16k33` 0.4.0 declares `embedded-hal ^0.2` |
| `mcp23xxx.kdl` | `mcp23017` 1.1.0 declares `embedded-hal ^0.2.3`, and covers only the 16-bit I²C member of the tag's family |
| `mpu9250.kdl` | `mpu9250` 0.25.0 declares `embedded-hal ^0.2.3`; its constructor also wants a separate `ncs` pin (`marg_default(spi, ncs, &mut delay)`) and yields an `Imu`/`Marg` typestate |
| `tlv493d.kdl` | no `tlv493d` crate; `sensor-tlv493d` 0.2.0 is the `embedded-hal` 0.2-era driver, so requirement 2 fails |
| `tmp102.kdl` | `tmp102` 1.0.0 is a Linux `i2cdev` crate; `tmp1x2` 1.1.0 is `nb`-based over `embedded-hal` 0.2 with a `SlaveAddr` argument (`plantmp123.md` §1) |
| `sen6x.kdl` | `sen6x` 0.1.4 is close (embedded-hal 1.0, async+blocking), but `Sen6x::new(&mut timer, i2c)` puts the **delay first and the bus second, by reference** — a bus device's constructor must take `{bus_device}` first |
| `ina219.kdl` | `ina219` 0.2.1 exposes `SyncIna219`/`AsyncIna219` with an `address::Address` type and calibration typestates, not a `new(i2c, address)` call the manifest can spell |

## 3. What the answers hinge on

Three different walls, in increasing cost to remove:

1. **`embedded-hal` 0.2 only** (7 manifests: `ads1x15`, `am2320`, `ht16k33`,
   `mcp23xxx`, `mpu9250`, `tlv493d`, `tmp102`). The runtime hands out
   `embedded-hal` 1.0 endpoints, so no manifest can use these crates, however
   good they are. Only an upstream 1.0 release changes this — a 0.2 shim would
   be a runtime type, not a manifest feature.
2. **A missing or too-narrow crate** (5: `as7262`, `bmxx80`, `tca95xx`,
   `tmp123`, plus `sen6x`'s argument order). This is the case the project-local
   crate exists for, and the answer is "publish the crate", which `plantmp123.md`
   §1 already notes would make the conversion a one-line `path` → `version`.
3. **Schema expressiveness** (5: `aht20`, `mfrc522`, `ccs811`, `pcf857x`, and
   the `then`-literal question for `bme280_*`). Two additive changes would move
   all of them:

   - *address mapping for bus attributes* — the `pcf857x` class (`SlaveAddr`),
     and probably `pca9685`/`ds3231`/`ina219` if their address is an enum. A
     `map` over `address` (or a `dependency`-level conversion) is the smallest
     version.
   - *`then` rebinding* — `aht20`, `mfrc522`, `ccs811`: a step whose result
     replaces the field, so `type` can name the post-init type. `driverglue.md`
     §A.3 stopped short of this deliberately; doing it is what removes the
     typestate glue class.

## 4. What a conversion costs

Not just the `.kdl`:

- **`src/app.rs` changes.** The local crates' APIs were chosen to fit the
  fixtures (`bh1750` powers on in `new`, `sgp30` accepts `set_humidity`, …
  ); the published crates differ in measurement mode and error types.
- **Host tests disappear.** Each local crate has a `just <tag>-driver-test` that
  asserts the protocol on the host. Manifest-only moves that Rust upstream, so
  the only remaining evidence is the emulator run (`just <tag>-sensor`), which
  asserts the part's state, not the crate's internals.
- **`.espforge/drivers.lock` must be re-recorded** (`just <tag>-driver-record`)
  because the manifest text is pinned; `src/app.rs`, the `.espf.txt` and
  `board.json` need no re-record.
- **Description prose.** Every one of the 26 currently ends with "The Rust half
  is the `<tag>-driver` crate beside this manifest", which becomes false.

## 5. How each verdict was reached

- Manifest shape: read all 29 `.kdl` files (uniform: one `bus`, one `codegen`,
  `init` attribute, no `transport`/`compose`; the three busless ones are the
  only `pin` users).
- Local crate size and role: `wc -l drivers/*/*-driver/src/lib.rs` — 102 to 577
  lines each, i.e. protocol implementations, not 15-line glue.
- Crate facts: crates.io search API and docs.rs crate/struct pages on
  2026-11-21. The `embedded-hal` requirement was read from each crate's
  dependency list; constructors from the README example or the struct page.
  Verified constructor pages: `aip31068` (via `i2c-character-display` README),
  `bh1750`, `aht20-driver`, `sgp30` (`new(i2c, address: u8, delay: D)`),
  `bme280` (both `i2c` and `spi::BME280::new`), `mcp9808` (`new(i2c)`, no
  address), `sen6x`, `mfrc522`, `embedded-ccs811`, `ina219` (item list only),
  `mcp23017`, `ht16k33`, `mpu9250`, `am2320`, `ads1x15`, `pcf857x`, `tmp102`,
  `tlv493d`.
- Not verified, and therefore not claimed as convertible: `pwm-pca9685`,
  `ds323x`, `hdc302x` constructors (§2c).
- Existing docs that agree: `espforge/docs/002-external-driver-manifest.md`
  (slots and constructor rules), `espforge/driverglue.md` §1 and §A (typestate
  and step limits), `plantmp123.md` §1 (why `tmp123` is a local crate).

## 6. Answer in one line

Of the 26 manifests that are not manifest-only, **6 can be converted now**
(`aip31068`, `bh1750`, `bme280_i2c`, `bme280_spi`, `mcp9808`, `sgp30`), **3
need one docs check** (`pca9685`, `ds3231`, `hdc302x`), **4 need a schema change
or glue** (`aht20`, `ccs811`, `mfrc522`, `pcf857x`), and **13 cannot** — mostly
because the only crate for the part is `embedded-hal` 0.2 or simply does not
exist. The three already-manifest-only manifests are `hd44780`, `max7219` and
`nxp74hc595`.
