# `manfifestfindings.md`: can the 8 pending conversions be manifest-only?

**Question:** `manifests.md` §2b lists eight manifests that `espforge` 0.9.3
supposedly unblocked (`pcf857x`, `pca9685`, `hdc302x`, `aht20`, `mfrc522`,
`ccs811`, `sen6x`, `ina219`). Does the *current* `espforge` actually allow
those conversions, and where it does not, what is the blocker and is there an
implementation plan?

**Answered:** 2026-11-22, against the checked-out `espforge` 0.9.3
(`espforge/Cargo.toml`) and the crates.io/docs.rs state of that date. Analysis
only: no manifest, crate, fixture or recipe was changed.

## 1. Answer in one line

**7 of the 8 are expressible today and need no `espforge` change; `aht20` is
not, and the blocker is the published crate's borrowing typestate, not the
manifest schema.** The schema work that `research/11-manifest-only-drivers.md`
Phase 2 schedules has already landed in 0.9.3, so the remaining work is
per-manifest conversion, not compiler work.

## 2. What the schema supports now

Read in code, not just docs; each row names the test that pins it.

| Mechanism | Where it is implemented | Pinning test |
|---|---|---|
| `{bus_device}` at any constructor position | `espforge-schema/src/driver.rs` `build_recipe` pushes `CtorArg::Bus` where it appeared (~2136); `espforge-elements/src/devices/recipe.rs:633` records `bus_index` | `a_bus_may_come_after_another_argument` (driver.rs:3421) asserts `args[1] == Bus` |
| `map "address"` for a crate's own address type | driver.rs:1941–2120: needs a finite `address allowed=` (or `fixed`) and **an entry for every address**; resolved in recipe.rs:944 | `a_mapped_address_loads` (3471), `…_needs_a_complete_range` (3500), `…_needs_a_finite_range` (3522), `…_must_reach_the_constructor` (3543) |
| `then { call ".." }` with `rebind #true` and `fallible #false` | driver.rs `parse_step` 1648–1771; `espforge-elements/src/model.rs::step_statements` 2039 emits `let mut dev = dev.init()?;` | `a_then_step_says_whether_it_rebinds_and_is_fallible` (3599) |
| a `pin` on a bus driver | `pin` becomes `CtorArg::Slot` (driver.rs:2153), claimed in recipe.rs:508–566; `esp_hal::gpio::Output` implements `embedded-hal` 1.0 `OutputPin` (`impl OutputPin for Output<'_>`) | existing bus devices with pins (`tft` dc/reset on SPI) |
| per-dependency Cargo `features` | driver.rs:911; emitted in `espforge-emit/src/codegen/cargo.rs:428` | the `mode-crate` schema test |

`then` steps may read only the values the constructor already takes plus fixed
Rust expressions (`parse_steps`, driver.rs:~1780), which is enough for every
row below.

## 3. Per-manifest verdicts

| Manifest | Allowed? | Shape that fits | Residual work / caveat |
|---|---|---|---|
| `pcf857x` | **yes** | `Pcf8574::new({bus_device}, {address})` with `map "address"` over the 8 addresses `0x20..=0x27` → `SlaveAddr::default()` / `SlaveAddr::Alternative(a2,a1,a0)` | The map is required: `SlaveAddr` is an enum with no `From<u8>`. `app.rs`, re-record, sensor. |
| `pca9685` | **yes — simpler than `manifests.md` says** | `Pca9685::new({bus_device}, {address})` | docs.rs 1.0.0: `new<A: Into<Address>>(i2c, address)` and `impl From<u8> for Address`, so a plain 7-bit address works and the **56-entry six-bool map is unnecessary**. The constructor returns `Result`, so leave `fallible` at its default. |
| `hdc302x` | **yes** | `Hdc302x::new({bus_device}, {delay}, {address})` with a `delay` attr mapped to the runtime delay, plus `map "address"` over `0x44..=0x47` → `I2cAddr::Addr00..=Addr11` | Constructor is infallible. Check whether the crate gates its blocking API behind a Cargo feature and declare it with `features ".."` if so. |
| `sen6x` | **yes** | `sen6x::blocking::Sen6x::new({delay}, {bus_device})` with `fallible #false`, plus `then { call "start_continuous_measurement()" }` | Bus is argument 1 — the 0.9.3 change. The step takes `&mut self` and returns `Result<(), Sen6xError>`, so no rebind. docs.rs confirms `new(delay: D, i2c: I2C) -> Self`. |
| `mfrc522` | **yes** | `Mfrc522::new(mfrc522::comm::blocking::spi::SpiInterface::new({bus_device}))` with `fallible #false`, plus `then { call "init()" rebind #true }`; `type` names the post-init `Mfrc522<…, Initialized>` | docs.rs confirms `new(comm) -> Self` and `init(self) -> Result<Mfrc522<COMM, Initialized>, _>`; confirm the state type is exported. Keep the crate's optional `embedded-hal` 0.2 feature off. |
| `ccs811` | **yes, with an interface decision** | `Ccs811::new({bus_device}, {address}, {nwake}, {delay})` with `map "address"` over `0x5A..=0x5B`, a `pin "nwake"` and `then { call "start_application()" rebind #true }` | `n_wake_pin: NWAKE` is a **required** `OutputPin` and `wake_delay` is taken by value, so the element gains a `pin "nwake"` attribute — boards must wire nWAKE. That is an element-interface change, not a schema gap. The pin-less alternative is `Ccs811Awake` with nWAKE tied low. |
| `ina219` | **yes** | `SyncIna219::new({bus_device}, {address})` with `map "address"` over `0x40..=0x4F` → `Address::from_byte(0xNN).unwrap()` | `Address` is `TryFrom<u8>`, **not** `From<u8>`, and `new` takes `Address` directly, so the map is mandatory (16 entries, or narrow `address allowed=` first). Constructor is fallible, so the default `fallible #true` is right. |
| `aht20` | **no** | — | See §4. |

## 4. `aht20`: the one real wall

`aht20-driver` 2.0.0, from docs.rs:

```rust
pub fn init(&mut self, delay: &mut impl DelayNs)
    -> Result<AHT20Initialized<'_, I>, Error<I::Error>>
```

`init` **borrows** the device and returns a value that **borrows it**. That
breaks the two ways a manifest could store it:

- `rebind #true` emits `let mut dev = dev.init(..).map_err(..)?;`, which
  borrows `dev` and then assigns to it — `E0506`, the local is borrowed while
  it is being reassigned.
- Even without the assignment, `AHT20Initialized<'_, I>` has a lifetime tied to
  the local, so it cannot be the `Devices` field type without a
  self-referential struct.

`manifests.md` §2b and `research/11` Gap 2C both assume a consuming
`init(self) -> AHT20Initialized<I>`; the real crate's API is not that.

Options, cheapest first:

1. **Stay local** (status quo). The `aht20-driver` crate beside the fixture
   implements the protocol; nothing is wrong with it.
2. **Manifest-only with the borrow dance in `app.rs`.** Field type
   `aht20_driver::AHT20<I>`; a non-rebind
   `then { call "init(&mut espforge_runtime::Delay::new())" }` calibrates at
   boot (the returned guard is dropped), and each read does
   `let mut ready = dev.init(&mut delay)?;` locally to reach `measure`. The
   manifest is still the whole driver, but every measurement re-runs init's
   status path.
3. **Upstream change**: a consuming `init(self)` or an owning initialized
   type. Then it converts with `rebind #true` and no `espforge` change.
4. **A schema feature that stores a guard plus its owner**: self-referential
   struct, wrong seam; not recommended.

## 5. Corrections to existing documents

- `manifests.md` §2b: move `aht20` out of "converted by the 0.9.3 additions"
  (rebind cannot express it); §6's "8 that 0.9.3 unblocked" becomes **7**.
- `manifests.md` §2b, `pca9685` row: the 56-entry map is not needed —
  `Address: From<u8>` means the plain `{address}` works.
- `manifests.md` §2b, `ccs811` caveats: the "`nb::Result`" ergonomics note is
  stale for 1.0.0, whose methods return `Result<_, Error<CommE, PinE>>`; the
  real constraint is the **required** nWAKE pin.
- `manifests.md` §2b, `ina219` row: `Address` is `TryFrom<u8>`, so the map is
  required rather than optional; `from_byte(..).unwrap()` is the expression.
- `espforge/research/11-manifest-only-drivers.md`: Phase 2 (bus position,
  address map, rebind/`fallible`) is **landed in 0.9.3** and should be marked
  done; Phases 1, 3 and 4 remain accurate.
- Header comments in `manifests/*.kdl` (and the copies under
  `drivers/<tag>/drivers/<tag>.kdl`) encode these verdicts, so a conversion
  must update the header as well as `dependency`/`type`/`constructor`.

## 6. Implementation plan

There is **no `espforge` compiler work left** for these eight. The plan is
per-manifest, and each line is proven only by its own gate:

1. Edit `manifests/<tag>.kdl` **and** `drivers/<tag>/drivers/<tag>.kdl`
   (dependency, `type`, `constructor`, `then`, `map`, description, header).
2. Update the fixture's `src/app.rs` to the published crate's measurement and
   control methods; the local crates' APIs were shaped around the fixtures and
   the published ones differ.
3. Run the repo recipes from the project root:
   `just <tag>-check` → `just <tag>-driver-record` → `just <tag>-image` →
   `just <tag>-sensor`.
4. Delete the superseded `../<tag>-driver` crate once the emulator test passes.

Suggested order (cheapest first): `pca9685` (no map), `sen6x`, `mfrc522`,
`hdc302x`, `pcf857x`, `ina219`, then `ccs811` after the nWAKE decision, then
`aht20` after choosing among §4's options.

Two side effects to expect on every line: the host-side `just <tag>-driver-test`
evidence disappears with the local crate (only the emulator's part model
asserts behaviour afterwards), and `.espforge/drivers.lock` must be re-recorded
because the manifest text is pinned.

## 7. How these verdicts were reached, and what they are not

- **Schema capability:** read in `espforge-schema/src/driver.rs`,
  `espforge-elements/src/model.rs` and `espforge-elements/src/devices/recipe.rs`,
  with the unit tests named in §2. Not taken from `manifests.md` or the docs.
- **Crate shapes:** docs.rs pages for `aht20-driver` 2.0.0, `mfrc522` 0.8.0,
  `embedded-ccs811` 1.0.0, `sen6x` 0.1.4, `hdc302x` 0.5.0, `pwm-pca9685` 1.0.0,
  `pcf857x` 0.5.0, `ina219` 0.2.1 and `esp-hal` 1.0.0 (`gpio::Output`), on
  2026-11-22.
- **Not done:** no manifest was written and no generated project was compiled.
  The schema checks the template's syntax, not the crate's trait bounds, so
  each conversion still has to pass the §6 gate on `esp32c3` before it counts.
