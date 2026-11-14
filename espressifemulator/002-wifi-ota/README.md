# 002-wifi-ota — emulator fixtures

Hand-written scaffolding for [`testota.md`](../../testota.md). The disposable
images and server root live in `build/`; nothing here modifies
`blocks/002-wifi-ota/`.

| Path | testota.md | Role |
|---|---|---|
| `v1/` | §4.3 | test-only 002 variant: `TEST:` logs, streaming updater, no `mark_app_valid()` |
| `blink-confirm/` | §4.4 | the `blink` payload plus `<ota />` and `mark_app_valid()` (T4) |
| `scripts/image-info.py` | §4.2, T6 | size/CRC32, slot fit, patch the v1 constants |
| `../../justfile` | §4, §5 | `ota-*` recipes: fixtures, image info, HTTP server, control channel |

`v1` keeps the block's `<ota flash_size="4MB" rollback="true" />` layout, so the
slot is `0x1F0000` bytes. It never confirms its own image; that is intentional
for T3.

## Prerequisites

- `esp-emu` v1.0.0 on `PATH` (all runs are `esp32c3`)
- `cargo espflash` (`cargo install espflash`) for image extraction
- `python3`, `nc`
- the espforge binary at `espforge/target/debug/espforge`
  (`just build_espforge` if it is missing)

## Build the fixtures

```sh
just ota-fixtures
```

This runs, in order:

1. `just ota-base` → `build/ota/base.bin` + `base.elf` (merged, from the block)
2. `just ota-blink` → `build/ota/blink.bin` + `blink.elf` (app image only)
3. `just ota-patch-v1` → writes blink's size/CRC32 into `v1/src/app.rs`
4. `just ota-build-v1` → `build/ota/v1.bin` + `v1.elf` (merged)
5. `just ota-blink-confirm` → `build/ota/blink.fw.bin` (app image only)

Step 3 rewrites the two constants at the top of `v1/src/app.rs`; expect that
file to change when the payload is rebuilt. After `just ota-blink` alone, run
`just ota-patch-v1` and `just ota-build-v1` before re-running T2. For T4:

```sh
just ota-patch-v1 build/ota/blink.fw.bin
just ota-build-v1
```

`just ota-image-info <image>` prints size, CRC32, slot fit and the matching Rust
constants without touching any source.

## Cases

Serve the fixtures in their own terminal and leave it running:

```sh
just ota-serve
```

### T0 / T1 — boot and association (the gate)

```sh
just ota-t0
```

Expect `wifi-ota: connecting...` then `Connected, IP address: 192.168.4.2/24`.
The control run with credentials that match nothing must never print an IP:

```sh
just esp-emu "--chip esp32c3 --firmware build/ota/base.bin --net user --wifi-ssid nope --wifi-password MyPassword --timeout 45s --log-file build/ota/t0-bad.log"
```

### T2 — download, verify, reboot into the new slot

```sh
just ota-t2
```

Expect `TEST: ip 192.168.4.2/24`, `TEST: ota begin ...`, decile progress, then
`TEST: ota done, rebooting`, followed by `blink example` and no more `TEST:`
lines. While it runs (120 s window), inspect `otadata` and reset from another
terminal:

```sh
just ota-otadata
just ota-reset
```

Erased flash reads `ff`; after `finish()` the active entry carries an `ota_seq`.
Record the bytes rather than treating them as a golden value.

### T3 — unconfirmed image rolls back

After `blink example` appears, `just ota-reset` in the same session. The next
boot must be v1 again (`TEST: wifi-ota: connecting...`, `TEST: ip ...`) and not
blink. Stop that run as soon as the v1 boot line appears — it starts another
update.

### T3b — cold boot from persisted flash

```sh
just ota-state-copy
just esp-emu "--chip esp32c3 --firmware build/ota/v1-state.bin --net user --wifi-ssid MyNetwork --wifi-password MyPassword --control-tcp 127.0.0.1:5558 --save-state --timeout 120s --log-file build/ota/t3b.log"
```

Run T2's sequence on the copy, then exit. A second run on the same file boots
the payload, and a `reset` in that second run reverts to v1.

### T4 — confirmed image persists

Build `v1` with `blink.fw.bin` as above, then run `just ota-t2`. After the
payload boots, reset twice:

```sh
printf 'reset\nreset\n' | nc -q1 127.0.0.1 5558
```

The payload keeps booting and v1 never returns. (Give the payload a moment to
run `mark_app_valid()` after each boot.)

### T5 — corrupt payload is rejected

Mutating `blink.bin` in place keeps the URL and the old CRC32 constants:

```sh
cp build/ota/blink.bin build/ota/blink-good.bin
cp build/ota/blink.bin build/ota/blink-bad.bin
printf '\x00' | dd of=build/ota/blink-bad.bin bs=1 seek=4096 count=1 conv=notrunc
cp build/ota/blink-bad.bin build/ota/blink.bin
```

Run `just ota-t2`: progress reaches 100%, then `TEST: ota finish failed`, no
reboot, and `just ota-otadata` is unchanged. Restore with
`cp build/ota/blink-good.bin build/ota/blink.bin`. A wrong `IMAGE_SIZE` (patch
with `just ota-patch-v1 build/ota/blink-good.bin` but serve the bad file)
must fail in `begin`/`write` instead of activating a truncated image.

### T6 — payload fits the slot

```sh
just ota-image-info build/ota/blink.bin
```

Exits non-zero if the image is `>= 0x1F0000`. Rebuild in release before
blaming `<ota />`.

### T7 — update server unreachable

Stop `just ota-serve`, then run `just ota-t2`. Expect
`TEST: download failed` after `timeout_ms=30000`, no reboot, and the device
still logging.

## Cleanup

```sh
just ota-clean
```

`build/` is disposable and gitignored. The only tracked file that changes during
a fixture build is `v1/src/app.rs` (its two constants).
