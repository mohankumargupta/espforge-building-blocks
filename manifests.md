# `manifests.md`: which manifests could be manifest-only

**Question:** of the 29 driver manifests in `manifests/`, which ones that are
*not* manifest-only could be written as manifest-only drivers — and for the
rest, what blocks it?

**Answered:** 2026-11-21, against **`espforge` 0.9.3** (workspace version in
`espforge/Cargo.toml`, HEAD `f4a20ae`) and the crates.io/docs.rs state of that
date. Analysis only: no manifest, crate, fixture or recipe was changed.

> This supersedes the 0.9.2 answer. Three things changed in 0.9.3 and they move
> eight manifests: `then` steps can rebind a typestate, `map "address"` can
> give a crate's address enum, and `{bus_device}` no longer has to be the first
> constructor argument.

**Status 2026-11-21:** the seven §2a manifests (`aip31068`, `bh1750`, `sgp30`,
`mcp9808`, `ds3231`, `bme280_i2c`, `bme280_spi`) have been converted. Each now
names a published crate, and no `*-driver` path dependency remains; the local
crates are unreferenced but still on disk.

## 1. What "manifest-only" means here

Manifest-only does not mean "no Rust". It means the Rust the element needs is a
published crate the manifest names, so the `.kdl` file is the whole driver
definition. Concretely the manifest fills three slots:

- `type "..."` — the concrete field type, over the endpoint `{bus_device}`
  produces (or the post-init type, when a step rebinds);
- `constructor "..."` — one plain call; on a bus device `{bus_device}` appears
  exactly once, at whatever position the crate wants;
- `endpoint "..."` — the bus type, only when a bus device offers
  `init="deferred"`.

The dependency is a `version` (or `git`+`rev`), not a `path` to a crate beside
the fixture. `codegen.feature` stays absent.

**The test used below.** A manifest can become manifest-only when all four hold:

| # | Requirement | Fails when |
|---|---|---|
| 1 | A published crate implements the part's protocol | no crate, or only a Linux/`i2cdev` or other-part crate |
| 2 | Its API is the runtime's `embedded-hal` 1.0 (`I2c`, `SpiDevice`, `DelayNs`) | the crate is `embedded-hal` 0.2/`nb`-based |
| 3 | The field type is nameable and built by one call, with the bus somewhere in it | builder chain, struct literal, or a bus that must be `&mut`ed in a way the template cannot spell |
| 4 | Nothing after construction has to change the field's type | `new()` returns `Uninit` and `init(self)` returns another type |

Requirement 3's address clause is gone: `map "address"` now supplies a crate's
own address type (`espforge-schema/src/driver.rs`, `build_recipe`, complete
finite range enforced). Requirement 4 is gone for `then { call ".." rebind
#true }`, which emits `let mut dev = dev.init()?;` so the field can be declared
as the post-init type (`espforge-elements/src/model.rs::step_statements`).
`then` steps may use only values the constructor already takes, plus fixed Rust
expressions, so `init(&mut espforge_runtime::Delay::new())` is legal while
`init(some_computed_value)` is not.

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
drivers/*/*-driver/src/lib.rs`). The 26 split 15 / 11: fifteen can be made
manifest-only today, eleven cannot.

### 2a. Convertible today, no `espforge` change (7)

The published crate already has the right shape; only the `dependency` line,
`type`, `constructor` and the description prose change.

| Manifest | Crate | Constructor that fits | Note |
|---|---|---|---|
| `aip31068.kdl` | `i2c-character-display` 0.6.0 | `CharacterDisplayAIP31068::new({bus_device}, LcdDisplayType::Lcd16x2, espforge_runtime::Delay::new())` | the adapter takes **no address** (the controller is fixed at 0x3E), so the constructor drops `{address}`; `address allowed="0x3e..=0x3e"` can stay. Same crate and endpoint shape `hd44780.kdl` already proves |
| `bh1750.kdl` | `bh1750` 0.1.0 | `BH1750::new({bus_device}, {delay}, false)` | add `attr "delay"` + `map "delay"` → `espforge_runtime::Delay::new()`; third argument a fixed literal |
| `sgp30.kdl` | `sgp30` 1.0.0 | `Sgp30::new({bus_device}, {address}, {delay})` | verified `new(i2c: I2C, address: u8, delay: D)`; `init()` stays an `app.rs` call |
| `mcp9808.kdl` | `mcp9808` 0.4.0 | `MCP9808::new({bus_device})` | the crate hardcodes 0x18 and takes no address, so narrow to `address fixed="0x18"` (the example already writes `address="0x18"`) |
| `ds3231.kdl` | `ds323x` 0.7.0 | `ds323x::Ds323x::new_ds3231({bus_device})` | infallible, hardcoded 0x68: `fallible #false` + `address fixed="0x68"`; type is `Ds323x<I2cInterface<..>, ic::DS3231>`. `new_ds3231` verified on docs.rs 0.7.0 |
| `bme280_i2c.kdl` | `bme280` 0.5.1 | `bme280::i2c::BME280::new({bus_device}, {address})` + `then { call "init(&mut espforge_runtime::Delay::new())" }` | `new` is infallible (`fallible #false`); `init` applies 2x T / 16x P / IIR 16, so the fixture's expected readings may move |
| `bme280_spi.kdl` | `bme280` 0.5.1 | `bme280::spi::BME280::new({bus_device})` + the same `then` step | verified `pub fn new(spi: SPI) where SPI: SpiDevice`; espforge's `ConfiguredSpiDevice` is exactly that |

The `then` step is a hole-free literal (`init(&mut
espforge_runtime::Delay::new())`), which `parse_step` accepts: it validates as
one method call on the generated receiver and takes no `{placeholder}` the
constructor does not already take.

`bme280_i2c` and `bme280_spi` could instead become **one** `transport`
manifest (`<bme280 bus="$i2c0" address=... />` / `<bme280 bus="$spi2" cs=... />`,
doc §7), but that renames both tags and edits both `.espf` files, so it is not a
plain per-manifest conversion.

### 2b. Converted by the 0.9.3 additions (8)

These were blocked in 0.9.2 and are not blocked now. Each row names the one
mechanism that moves it.

| Manifest | Crate | Shape that now fits | Mechanism |
|---|---|---|---|
| `pcf857x.kdl` | `pcf857x` 0.5.0 | `Pcf8574::new({bus_device}, {address})` with `map "address"` over `0x20..=0x27` → `SlaveAddr::default()` / `SlaveAddr::Alternative(a2,a1,a0)` | `map "address"` (complete 8-entry set) |
| `pca9685.kdl` | `pwm-pca9685` 1.0.0 | `Pca9685::new({bus_device}, {address})` with `map "address"` → a six-bool tuple (docs.rs 1.0.0: `Pca9685::new(dev, (a5,a4,a3,a2,a1,a0))`), or `Address::default()` for 0x40 | `map "address"` (56 entries for `0x40..=0x77`; narrowing `allowed` first is cheaper) |
| `hdc302x.kdl` | `hdc302x` 0.5.0 | `Hdc302x::new({bus_device}, {delay}, {address})` with `map "address"` over `0x44..=0x47` → `I2cAddr::Addr00..=Addr11`, `features "blocking"` | `map "address"` + delay map; address is the third arg, which is now legal |
| `aht20.kdl` | `aht20-driver` 2.0.0 | `AHT20::new({bus_device}, aht20_driver::SENSOR_ADDRESS)` + `then { call "init(&mut espforge_runtime::Delay::new())" rebind #true }`; `type` names `AHT20Initialized<I2C, Delay>` | `rebind #true` on a `then` step |
| `mfrc522.kdl` | `mfrc522` 0.8.0 | `Mfrc522::new(mfrc522::comm::blocking::spi::SpiInterface::new({bus_device}))` + `then { call "init()" rebind #true }`; `type` names `Mfrc522<.., Initialized>` | `rebind #true`; `Initialized` is a public enum in the crate root (docs.rs 0.8.0) |
| `ccs811.kdl` | `embedded-ccs811` 1.0.0 | `Ccs811::new({bus_device}, {address}, {nwake}, {delay})` + `then { call "start_application()" rebind #true }`; `map "address"` over `0x5A..=0x5B` → `SlaveAddr::default()` / alternate | `map "address"` + `rebind` + a `pin "nwake"` |
| `sen6x.kdl` | `sen6x` 0.1.4 | `sen6x::blocking::Sen6x::new({delay}, {bus_device})` + `then { call "start_continuous_measurement()" }`; `fallible #false` | bus at argument 1 (docs.rs 0.1.4: `new(delay: D, i2c: I2C)` — by value, not `&mut`) |
| `ina219.kdl` | `ina219` 0.2.1 | `ina219::SyncIna219::new({bus_device}, {address})` with `map "address"` over `0x40..=0x4F` → `ina219::address::Address` (the module accepts a byte or two pins) | `map "address"`; calibration stays an `app.rs` call |

Three caveats, all smaller than the walls removed:

- `map "address"` must cover **every** address in the bus's finite range
  (`driver.rs` emits "`map "address"` has no entry for `0x..`"). `pcf857x` and
  `hdc302x` are 8 and 4 entries; `pca9685` is 56 and `ina219` is 16. Narrowing
  `address allowed=` to the addresses the board actually uses is the sane move.
- `mfrc522`'s SPI interface is a wrapper object, so the constructor nests the
  bus one level deep (`SpiInterface::new({bus_device})`), which the schema
  allows for any single bus hole. Its default features are `embedded-hal` 1.0;
  the 0.2 dependency is optional and must stay off.
- `ccs811`'s measurement reads return `nb::Result`, and `nb` is a direct
  dependency of the crate but not of the generated project. Construction is
  expressible; `src/app.rs` would have to poll the `nb` result by hand (or the
  project would need `nb` in its manifest-declared dependencies). That is an
  app-ergonomics cost, not a manifest blocker.

### 2c. Not convertible (11)

| Manifest | Why not |
|---|---|
| `tmp123.kdl` | no TMP121/TMP123 crate exists (checked in `plantmp123.md` §1 and re-checked: nothing on crates.io) |
| `as7262.kdl` | no crate — crates.io search for `as7262` returns 0 results |
| `bmxx80.kdl` | no crate named `bmxx80`. The manifest's part is a BME280, so it *could* be retargeted at `bme280` 0.5.1 — but then it is a duplicate of `bme280_i2c`, and the BMP280/BME680 coverage the tag name implies is in no published crate |
| `tca95xx.kdl` | no crate for the *family*; the per-variant crates (`tca9539`, `tca9555`, …) have different register maps, so one manifest cannot name one crate for a `0x20..=0x27` tag |
| `ads1x15.kdl` | `ads1x15` 0.5.0 is a 2018 Linux `i2cdev`/tokio crate; `ads1x1x` 0.2.0 is `embedded-hal` 0.2 + `nb`. The one usable eh1 crate, `embedded-ads111x` 0.1.3 (already behind the bundled `ads111x` manifest), is 16-bit ADS111x only, so it cannot carry the tag's ADS1015 half |
| `am2320.kdl` | `am2320` 0.2.0 declares `embedded-hal ^0.2` — the type cannot take espforge's 1.0 endpoint |
| `ht16k33.kdl` | `ht16k33` 0.4.0 declares `embedded-hal ^0.2` |
| `mcp23xxx.kdl` | `mcp23017` 1.1.0 declares `embedded-hal ^0.2.3`, and covers only the 16-bit I²C member of the tag's family |
| `mpu9250.kdl` | `mpu9250` 0.25.0 declares `embedded-hal ^0.2.3`; its constructor also wants a separate `ncs` pin and yields an `Imu`/`Marg` typestate |
| `tlv493d.kdl` | no `tlv493d` crate; `sensor-tlv493d` 0.2.0 is the `embedded-hal` 0.2-era driver, so requirement 2 fails |
| `tmp102.kdl` | `tmp102` 1.0.0 is a Linux `i2cdev` crate; `tmp1x2` 1.1.0 is `nb`-based over `embedded-hal` 0.2 with a `SlaveAddr` argument (`plantmp123.md` §1) |

## 3. What the answers hinge on

Three different walls, in increasing cost to remove:

1. **`embedded-hal` 0.2 only** (7 manifests: `ads1x15`, `am2320`, `ht16k33`,
   `mcp23xxx`, `mpu9250`, `tlv493d`, `tmp102`). The runtime hands out
   `embedded-hal` 1.0 endpoints, so no manifest can use these crates, however
   good they are. Only an upstream 1.0 release changes this — a 0.2 shim would
   be a runtime type, not a manifest feature, and the `espforge` docs reject
   the `embedded-hal-compat` shim for exactly that reason
   (`research/11-manifest-only-drivers.md`, Strategy B note).
2. **A missing crate** (4: `as7262`, `tmp123`, `tca95xx`, and `bmxx80` unless
   it is collapsed into `bme280`). This is the case the
   project-local crate exists for, and the answer is "publish the crate", which
   `plantmp123.md` §1 already notes would make the conversion a one-line
   `path` → `version` (or `git` + pinned `rev`).
3. **Nothing left in the schema.** The three walls the 0.9.2 answer recorded —
   bus-first constructors, address enums, typestate rebinding — are all closed
   in 0.9.3. A future manifest needing a builder chain (rung 4 in
   `driverglue.md` Appendix A.5) or a struct-literal constructor (A.2) is
   still out, but none of these 26 needs either.

## 4. What a conversion costs

Not just the `.kdl`:

- **`src/app.rs` changes.** The local crates' APIs were chosen to fit the
  fixtures (`bh1750` powers on in `new`, `sgp30` accepts `set_humidity`, …);
  the published crates differ in measurement mode and error types. `ccs811`
  additionally exposes `nb` results.
- **Host tests disappear.** Each local crate has a `just <tag>-driver-test` that
  asserts the protocol on the host. Manifest-only moves that Rust upstream, so
  the only remaining evidence is the emulator run (`just <tag>-sensor`), which
  asserts the part's state, not the crate's internals.
- **`.espforge/drivers.lock` must be re-recorded** (`just <tag>-driver-record`)
  because the manifest text is pinned; `src/app.rs`, the `.espf.txt` and
  `board.json` need no re-record.
- **Description prose.** Every one of the 26 currently ends with "The Rust half
  is the `<tag>-driver` crate beside this manifest", which becomes false.
- **Crate-swap risk.** Each new dependency has to compile on `esp32c3`
  blocking; the schema checks the template's syntax, not the crate's trait
  bounds. The per-manifest gate (`just check` → `just <tag>-driver-record` →
  `just build` → `just <tag>-sensor`) is what proves the shape.

## 5. How each verdict was reached

- **Manifest shape:** read all 29 `.kdl` files (uniform: one `bus`, one
  `codegen`, `init` attribute, no `transport`/`compose`; the three busless ones
  are the only `pin` users).
- **Local crate size and role:** `wc -l drivers/*/*-driver/src/lib.rs` — 102 to
  577 lines each, i.e. protocol implementations, not 15-line glue.
- **`espforge` 0.9.3 capabilities, read in code, not just docs:**
  - `espforge-schema/src/driver.rs::build_recipe` builds `address_map` and
    requires a complete finite range; tests `a_mapped_address_loads`,
    `a_mapped_address_needs_a_complete_range`, `a_mapped_address_needs_a_finite_range`.
  - `parse_constructor` records `CtorArg::Bus` where the manifest wrote it;
    test `a_bus_may_come_after_another_argument` asserts `args[1] == Bus`.
  - `espforge-elements/src/model.rs::step_statements` emits
    `let mut #receiver = #call?;` for `rebind #true` and skips `.map_err` for
    `fallible #false`; tests `a_then_step_says_whether_it_rebinds_and_is_fallible`,
    `a_then_step_names_only_constructor_values`.
  - `docs/002-external-driver-manifest.md` §5.2/§5.3/§5.4 documents all three
    (its "bus first" sentence in §5.2 is stale; §5.4 and the test are right).
- **Crate facts:** crates.io dependency API and docs.rs pages on 2026-11-21.
  Verified constructors: `ds323x::Ds323x::new_ds3231(dev)` (no address),
  `Hdc302x::new(i2c, delay, I2cAddr)` with `I2cAddr::Addr00 = 68 … Addr11 = 71`,
  `Sen6x::new(delay: D, i2c: I2C)` (by value, delay first),
  `pwm_pca9685::Pca9685::new(dev, Address)` (six-bool tuple),
  `pcf857x::Pcf8574::new(dev, SlaveAddr)` (`default()` / `Alternative(a2,a1,a0)`),
  `ina219::SyncIna219::new(i2c, Address)` (`Address` accepts a byte or two pins),
  `mfrc522::Mfrc522::new(comm) -> Uninitialized` then `init(self)` with public
  `Initialized`, `aht20_driver::AHT20::new(i2c, SENSOR_ADDRESS)` then
  `init(&mut delay) -> AHT20Initialized`, `embedded_ccs811::Ccs811::new(i2c,
  SlaveAddr, nwake, delay)` then `start_application()`. Dependency requirements
  read from the crates.io API: `aht20-driver` 2.0.0, `pcf857x` 0.5.0,
  `mfrc522` 0.8.0, `embedded-ccs811` 1.0.0 are all `embedded-hal ^1.0`
  (`mfrc522`'s 0.2 is optional).
- **Existing docs that agree:** `espforge/docs/002-external-driver-manifest.md`
  (slots, maps, steps), `espforge/research/11-manifest-only-drivers.md`
  (same three-gap grouping and the same 18/29 after Phase 2), `plantmp123.md`
  §1 (why `tmp123` is a local crate).

One correction to `research/11-manifest-only-drivers.md` §Gap 2A: it describes
`Sen6x::new(&mut timer, i2c)`. docs.rs for `sen6x` 0.1.4 shows
`pub fn new(delay: D, i2c: I2C) -> Self`, both by value. The bus-second point
stands; the borrow does not exist.

## 6. Answer in one line

Of the 26 manifests that are not manifest-only, **15 can be converted now** —
7 with only the `dependency`/`type`/`constructor` edit (`aip31068`, `bh1750`,
`sgp30`, `mcp9808`, `ds3231`, `bme280_i2c`, `bme280_spi`) and 8 that `espforge`
0.9.3 unblocked (`pcf857x`, `pca9685`, `hdc302x`, `aht20`, `mfrc522`, `ccs811`,
`sen6x`, `ina219`) — and **11 cannot**: 7 because the only crate for the part is
`embedded-hal` 0.2, and 4 because no crate exists (with `bmxx80` convertible
only by collapsing it into `bme280_i2c`). The three already-manifest-only
manifests are `hd44780`, `max7219` and `nxp74hc595`.
