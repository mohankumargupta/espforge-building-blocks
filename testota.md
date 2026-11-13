# Testing `002-wifi-ota` in `esp-emu`

How to exercise block 002 end to end with the Espressif emulator (`esp-emu`,
v1.0.0 in this workspace), using a `python3 -m http.server` on the host as the
update source and the espforge `blink` example as the OTA payload.

The block itself only joins Wi-Fi and logs its address: `<ota />` owns the A/B
partition table and `ctx.ota`, but the transport is the app (see
`blocks/002-wifi-ota/README.md`). So this plan tests the *block as shipped*
(Wi-Fi + DHCP + partition layout) and a **test-only app variant** that drives
`ctx.ota` from an HTTP download. The block's files are not modified.

All espforge invocations go through the root `justfile` (`just build`,
`just gen`, `just espforge`, `just esp-emu`), per `AGENTS.md`. `espflash` is
called directly for image extraction because the justfile has no image recipe.
Every block is `esp32c3`.

---

## 1. What the emulator actually runs

`esp-emu` is a RISC-V ESP32-C3 emulator that boots the **real mask ROM → the
`esp-bootloader-esp-idf` second stage → the app** from a merged flash image. That
matters here: partition table, `otadata` and rollback are executed, not
simulated at the Rust level.

| Piece | Real / emulated | Consequence for the test |
|---|---|---|
| Wi-Fi soft AP (WPA2/WPA3 transition), 802.11 assoc + 4-way handshake | emulated | firmware joins `--wifi-ssid` / `--wifi-password` |
| DHCP server | emulated, guest gets `192.168.4.2`, gateway `192.168.4.1` | the block's `config_v4()` log shows that address |
| User-mode NAT (`--net user`) | emulated (slirp-style) | traffic to the **gateway IP** is redirected to `127.0.0.1` on the host, so the guest fetches `http://192.168.4.1:8000/...` |
| Flash, partitions, `otadata` | emulated, sized from the image header | A/B slots and boot selection are real |
| Rollback | real IDF bootloader logic | a payload that never confirms is reverted on the next reset |
| Flash persistence across processes | only with `--save-state` (rewrites the firmware file on exit) | cold-boot tests need a saved copy |

What it cannot prove: a real radio/AP, mDNS (the block has none anyway), the
`ota:` upload endpoint ESPHome has and espforge does not, and timing of a power
loss mid-write.

### Main unknown to settle first

The repo's own `esp-emu` recipes (`espforge/testing/justfile`) exercise
`helloworld`, `blink`, `i2c_scanner`, `spi_example`, `uart_example`, `button` —
**not** a Wi-Fi station. espforge uses esp-hal's `esp-radio`, not ESP-IDF's
Wi-Fi stack, so case T0 below is the checkpoint that decides whether the rest of
this plan applies unchanged. If association fails, collect `RUST_LOG=debug` and
treat it as an esp-radio ↔ emulator integration finding, not a block regression.

---

## 2. Topology

```
host                            esp-emu (esp32c3)
┌────────────────────────┐      ┌──────────────────────────────┐
│ python3 -m http.server │      │ app v1 (ota_0 @ 0x10000)     │
│ 127.0.0.1:8000         │◄─────┤   GET /blink.bin             │
│ build/ota/blink.bin    │ NAT  │   ctx.ota.begin/write/finish │
└────────────────────────┘      │                              │
                                │ soft AP  192.168.4.1         │
 192.168.4.1 = host loopback    │ station  192.168.4.2         │
                                │                              │
                                │ ota_1 @ 0x200000 ← image     │
                                │ otadata @ 0xd000 ← selection │
                                └──────────────────────────────┘
```

The device asks for `http://192.168.4.1:8000/blink.bin`; no `hostfwd` is needed
in that direction (guest → host). `hostfwd` would only be needed for host →
guest.

Flash layout the merged image must carry (from `<ota />`, 4 MB):

| Partition | Offset | Size |
|---|---|---|
| `nvs` | `0x9000` | `0x4000` |
| `otadata` | `0xd000` | `0x2000` |
| `phy_init` | `0xf000` | `0x1000` |
| `ota_0` | `0x10000` | `0x1F0000` |
| `ota_1` | `0x200000` | `0x1F0000` |

Slot size is `0x1F0000` = **2,031,616 bytes**. A debug `blink` build fits
comfortably; check anyway (T6).

---

## 3. Fixtures

Everything disposable lives under `build/ota/` (already gitignored).

| Artifact | Source | Purpose |
|---|---|---|
| `build/ota/base.bin` | `blocks/002-wifi-ota`, merged image | T0/T1: block as shipped |
| `build/ota/blink.bin` | espforge `01.Basics/blink`, app image only | OTA payload served over HTTP |
| `build/ota/blink.fw.bin` | `blink` + `<ota />` + `mark_app_valid()` (optional) | T4: confirmed image persistence |
| `build/ota/v1.bin` | test variant of 002 with `<http_client>` and the updater | OTA client, merged image |
| `build/ota-src/v1/` | copy of the block + edits | the variant project (kept out of `blocks/`) |

`just build` always writes the generated crate to `build/build`, so copy each
image out before building the next fixture.

---

## 4. Building the fixtures

### 4.1 Baseline: the block as shipped (T0/T1)

```sh
just build example=blocks/002-wifi-ota
mkdir -p build/ota
(cd build/build && cargo espflash save-image --merge \
  --chip esp32c3 --flash-size 4mb \
  --partition-table partitions.csv --target-app-partition ota_0 \
  ../../build/ota/base.bin)
```

`--flash-size 4mb` must agree with `<ota flash_size="4MB">`: esp-emu takes the
emulated flash size from the image header, not the file length, and a smaller
die would truncate `ota_1`.

Expected boot log: `wifi-ota: connecting...` then
`Connected, IP address: 192.168.4.2/24` (the `/24` suffix comes from
`embassy_net::IpCidr`; assert on the address, not the exact line).

### 4.2 The payload: blink, as an OTA application image

Generate it through the justfile's espforge wrapper, then build it:

```sh
just espforge "examples get 01.Basics/blink --out build/ota-src/blink"
just build example=build/ota-src/blink
(cd build/build && cargo espflash save-image \
  --chip esp32c3 --flash-size 4mb ../../build/ota/blink.bin)
```

Two things to verify before serving it:

```sh
xxd -l 1 build/ota/blink.bin        # 0xe9 — an app image, not a merged one
stat -c%s build/ota/blink.bin       # < 2031616
```

**No `--merge` for the payload.** A merged image would carry a second-stage
bootloader and partition table into `ota_1`; `esp-hal-ota` wants the app image
only. If this `espflash` version writes the app segment under an address-prefixed
name instead, take the app segment (the file starting `0xe9`).

Record the exact bytes the device will verify:

```sh
python3 - <<'PY'
import pathlib, zlib
data = pathlib.Path("build/ota/blink.bin").read_bytes()
print("size:", len(data), "crc32:", hex(zlib.crc32(data) & 0xffffffff))
PY
```

Any later rebuild of `blink` changes both numbers — recopy them into the app.

### 4.3 The OTA client: a test-only variant of 002

```sh
mkdir -p build/ota-src/v1/src
cp blocks/002-wifi-ota/wifi-ota.espf.txt build/ota-src/v1/
cp blocks/002-wifi-ota/src/app.rs     build/ota-src/v1/src/
```

Add the HTTP transport to `build/ota-src/v1/wifi-ota.espf.txt`:

```xml
# services
  <wifi_station id="home" wifi="$radio" ssid="MyNetwork" password="MyPassword" />
  <http_client id="http" network="$home" rx_buffer_size="4096" timeout_ms="30000" />
```

`rx_buffer_size` is the streaming chunk size (`get_stream` reads into it), so
4 KiB keeps flash-write chunks small; `timeout_ms` is generous because the
emulator is instruction-stepped.

Replace `build/ota-src/v1/src/app.rs` with the updater. The two constants are
the ones printed by the script above:

```rust
use crate::{service, Context};
use embassy_executor::Spawner;

const IMAGE_SIZE: u32 = 0; // <- len(blink.bin)
const IMAGE_CRC32: u32 = 0; // <- zlib.crc32(blink.bin)
const URL: &str = "http://192.168.4.1:8000/blink.bin";

pub async fn setup(ctx: &'static Context, _spawner: Spawner) {
    let station = service!(ctx, home);
    let logger = ctx.logger;

    logger.info("TEST: wifi-ota: connecting...");
    station.wait_connected().await;
    match station.stack().config_v4() {
        Some(config) => logger.info(format_args!("TEST: ip {}", config.address)),
        None => {
            logger.error("TEST: no IPv4 address");
            return;
        }
    }

    logger.info(format_args!("TEST: ota begin size={IMAGE_SIZE} crc={IMAGE_CRC32:#010x}"));
    if let Err(e) = ctx.ota.begin(IMAGE_SIZE, IMAGE_CRC32) {
        logger.error(format_args!("TEST: ota begin failed: {e:?}"));
        return;
    }

    let http = service!(ctx, http);
    let mut complete = false;
    let mut last_pct = -1i16;
    let streamed = http
        .get_stream(URL, |chunk| {
            complete = ctx.ota.write(chunk).map_err(|_| ())?;
            if let Some(pct) = ctx.ota.progress() {
                if pct as i16 / 10 != last_pct {
                    last_pct = pct as i16 / 10;
                    logger.info(format_args!("TEST: ota {pct}%"));
                }
            }
            Ok::<(), ()>(())
        })
        .await;

    if let Err(e) = streamed {
        logger.error(format_args!("TEST: download failed: {e:?}"));
        return;
    }
    if !complete {
        logger.error("TEST: image incomplete");
        return;
    }

    match ctx.ota.finish() {
        Ok(()) => {
            logger.info("TEST: ota done, rebooting");
            ctx.ota.reboot();
        }
        Err(e) => logger.error(format_args!("TEST: ota finish failed: {e:?}")),
    }
}

pub async fn forever(ctx: &'static Context) {
    ctx.delay.delay_ms(1000).await;
}
```

`get_stream` takes `FnMut`, so `complete` and `last_pct` are captured mutably;
`ctx.ota` is `Copy` and safe to use from inside the callback. No `mark_app_valid`
here on purpose — v1 is factory firmware in `ota_0` and T3 depends on the *new*
image being the one that fails to confirm.

Build and image it:

```sh
just check example=build/ota-src/v1      # schema + resource graph
just build  example=build/ota-src/v1
(cd build/build && cargo espflash save-image --merge \
  --chip esp32c3 --flash-size 4mb \
  --partition-table partitions.csv --target-app-partition ota_0 \
  ../../build/ota/v1.bin)
```

`just check` first is worth it: a missing `FLASH` claim, a bad `network=`
reference or an out-of-range `rx_buffer_size` is reported there rather than as
a Rust compile error.

### 4.4 Optional payload that confirms itself (T4)

`blink` has no `<ota />` and never calls `mark_app_valid()`, so by design it is
reverted on the next reset. To test the confirmed path, generate `blink` again
into `build/ota-src/blink-confirm/`, add `<ota />` to `# espforge` and this line
after the Wi-Fi check in `setup`:

```rust
ctx.ota.mark_app_valid().ok();
```

Build it the same way as 4.2 and save as `build/ota/blink.fw.bin`; recompute
size/CRC and rebuild `v1` with those constants.

### 4.5 Serve the payload

```sh
python3 -m http.server 8000 --bind 127.0.0.1 --directory build/ota
curl -sI http://127.0.0.1:8000/blink.bin | head -1   # sanity
```

Leave it running for every case below. The guest reaches it at
`192.168.4.1:8000` through the emulator's gateway redirect.

---

## 5. Test cases

Every run goes through the justfile's `esp-emu` passthrough. The recipe is a
plain `esp-emu {{ args }}` shell, so quote the whole argument list.

Keep a pristine copy of each firmware: `--save-state` overwrites the file it was
given, which is the point in T3b but destructive if reused blindly.

### T0 — Firmware boots and the block's station associates

```sh
just esp-emu "--chip esp32c3 --firmware build/ota/base.bin --net user \
  --wifi-ssid MyNetwork --wifi-password MyPassword \
  --timeout 45s --log-file build/ota/t0.log"
```

Expected, in order:
1. ROM/bootloader output, then `wifi-ota: connecting...`
2. `Connected, IP address: 192.168.4.2`
3. no `wifi: disconnected` retry loop for the rest of the run

Pass: the address line appears within ~20 s of `connecting...`, and the device
stays associated. **This is the gate for everything else.**

Wrong SSID/password is the cheap control: `--wifi-ssid nope` must leave
`connecting...` unfulfilled — proves the log line is a real association, not a
constant.

### T1 — DHCP lease carries the emulator's addressing

Evidence for T0's address: `192.168.4.2` with gateway `192.168.4.1` is exactly
what `esp-emu`'s built-in AP hands out. If the firmware logged a `10.0.2.x`
address instead, the run is on a different network path (e.g. a stray `tap0`
was picked up — see [§7](#7-emulator-knobs-used-and-why)), and T2's URL would
be wrong.

Pass: `TEST`/block address is `192.168.4.2`, and `--net user` is on the command
line.

### T2 — OTA happy path: download, verify, reboot into the new slot

With `build/ota/v1.bin` built from §4.3 and the server up:

```sh
just esp-emu "--chip esp32c3 --firmware build/ota/v1.bin --net user \
  --wifi-ssid MyNetwork --wifi-password MyPassword \
  --control-tcp 127.0.0.1:5558 --elf build/build/target/riscv32imc-unknown-none-elf/debug/wifi_ota \
  --timeout 120s --log-file build/ota/t2.log"
```

Expected:
1. `TEST: ip 192.168.4.2/24`
2. `TEST: ota begin size=<n> crc=0x...`
3. monotonic `TEST: ota 0%` … `TEST: ota 100%` (decile throttled)
4. `TEST: ota done, rebooting`
5. bootloader messages, then `blink example` from the payload, and **no**
   further `TEST: ota ...` lines

Pass: all five, in that order, in one process. Step 5 is what proves
`finish()` switched `otadata` to `ota_1` and the IDF bootloader executed the
payload rather than re-running v1.

Evidence that it is really a slot switch, not just a reset: dump `otadata`
before and after through the control channel:

```sh
printf 'read-region 0xd000 32 build/ota/otadata-t2.bin\n' | nc -q1 127.0.0.1 5558
od -An -tx1 -N32 build/ota/otadata-t2.bin
```

Erased flash reads `ff`; after `finish()` the active entry has an `ota_seq`
(and its complement) written. Treat the exact bytes as evidence to record, not
as a golden value.

### T3 — Rollback: an unconfirmed image is reverted

`blink` never calls `mark_app_valid()`, and `<ota rollback="true">` is the
default, so a reset after T2 must boot v1 from `ota_0` again. In the *same*
session that just reached `blink example`, send a hard reset:

```sh
printf 'reset\n' | nc -q1 127.0.0.1 5558
```

Expected: bootloader runs, then `TEST: wifi-ota: connecting...` and
`TEST: ip ...` reappear — the `ota_0` image — and `blink example` does not.
The reappearing v1 then starts another OTA; stop the run as soon as the v1 boot
line is seen.

Pass: the next boot after T2 is v1, not blink. That is the bootloader's
rollback, and it is the behaviour block 099 depends on.

### T3b — Cold boot from persisted flash

To prove it is flash state and not process state, repeat with a saved image:

```sh
cp build/ota/v1.bin build/ota/v1-state.bin
just esp-emu "... --firmware build/ota/v1-state.bin --save-state --timeout 120s ..."
```

After the T2 sequence, `build/ota/v1-state.bin` now contains `ota_1`'s payload
and a pending `otadata`. A second run on that file must boot blink, and a
`reset` in that second run must revert to v1. Use `--timeout`, not `--exit-on
"blink example"`, if you want the process to reach the save point consistently
(`--exit-on` exits as soon as it matches, which is also fine — flash is saved on
exit either way).

### T4 — Confirmed image persists (optional)

Only with the `blink.fw.bin` payload from §4.4 (same size/CRC rebuilt into v1):

```sh
# v1 fetches blink.fw.bin; after "rebooting" and the payload's own
# mark_app_valid(), send two resets.
printf 'reset\nreset\n' | nc -q1 127.0.0.1 5558
```

Expected: the payload keeps booting; v1 never comes back. Pass: confirmed
`ota_1` survives repeated resets. Comparing T3 and T4 is the A/B contract.

### T5 — Corrupt payload is rejected, device stays usable

Serve a mutated copy of the same length so only the CRC differs:

```sh
cp build/ota/blink.bin build/ota/blink-bad.bin
printf '\x00' | dd of=build/ota/blink-bad.bin bs=1 seek=4096 count=1 conv=notrunc
# point URL at blink-bad.bin (or rebuild v1 with blink-bad's size + blink's CRC)
```

Expected: progress reaches 100%, then `TEST: ota finish failed: ...` (CRC or
read-back verification), **no** `rebooting`, and the device keeps running v1 —
`TEST: ip` stays reachable, and a reset still lands in `ota_0` with no pending
slot. Pass: no activation on a bad image, no boot loop, `otadata` unchanged from
the pre-update dump.

A second variant worth running: wrong `IMAGE_SIZE` (e.g. size of the old build)
must fail in `begin`/`write` rather than activating a truncated image.

### T6 — Payload cannot exceed the slot

Not a device run: `stat -c%s build/ota/blink.bin` must be < `2,031,616`
(`0x1F0000`), and the same check applies to any replacement payload. If a debug
build ever crosses it, rebuild with a release profile before blaming
`<ota />`; `begin()` rejects an oversized image, which looks like a transport
failure in the log.

Optional negative: pass the real size but a deliberately huge value in
`begin(size, crc)` and confirm the error is reported *before* any HTTP traffic.

### T7 — Server unreachable

Stop `python3 -m http.server`, then run v1. Expected: `TEST: download failed`
(timeout / connection refused) after `timeout_ms`, no reboot, device still
associated and logging. Pass: a missing update server never bricks the node.

---

## 6. Result table

| ID | Firmware | Expected | Block claim exercised |
|---|---|---|---|
| T0 | `base.bin` | associates, logs `192.168.4.2` | `wifi:` + `<wifi_station>` association + DHCP |
| T1 | `base.bin` | lease is the emulator's `192.168.4.x` | address really from DHCP, not a constant |
| T2 | `v1.bin` | download → verify → reboot → blink | `<ota />` slots, `ctx.ota.begin/write/finish`, boot selection |
| T3 | T2 session | reset boots v1 again | A/B rollback without `mark_app_valid()` |
| T4 | `v1.bin` + `blink.fw.bin` | confirmed image survives resets | `mark_app_valid()` persistence |
| T5 | `v1.bin` + bad payload | rejected, v1 keeps running | CRC/read-back verification |
| T6 | payload | size < slot | partition sizing |
| T7 | `v1.bin`, server down | error, no reboot | transport failure is survivable |

A green run is T0–T3 plus T5 and T7. T4 is the extra confirmation the shipped
blink payload cannot provide.

---

## 7. Emulator knobs used and why

| Flag | Why it is in the command |
|---|---|
| `--net user` | zero-setup NAT; gateway `192.168.4.1` is redirected to host loopback, which is how the python server is reached |
| `--wifi-ssid MyNetwork --wifi-password MyPassword` | must match the block's `<wifi_station>` literals; the defaults are `myssid`/`mypassword` |
| `--control-tcp 127.0.0.1:5558` | `reset` for T3/T3b/T4 and `read-region 0xd000 ...` for `otadata` evidence, all without restarting the process |
| `--save-state` | persists flash/`otadata` to the firmware file on exit (T3b) — on a copy, never the pristine image |
| `--elf` | symbolised backtraces; pass both builds for an OTA test so the payload's ELF takes over after reboot |
| `--timeout` / `--exit-on` | bounds each case; `--exit-on "blink example"` is the shortest T2 assertion |
| `--log-file` | keeps emulator log lines out of the firmware console, so captured output greps cleanly |

If a Linux `tap0` exists, `--net` defaults to it rather than `user`; always pass
`--net user` explicitly, and check that `192.168.4.2` is the address that shows
up (T1).

`RUST_LOG=debug` on a failing run prints the flash size the emulator picked
(`Flash resized to N KB`) — the first thing to check if `ota_1` is missing.

---

## 8. What this plan does not test

- **`ota:` as a server**: espforge has no upload endpoint, no `password` and no
  mDNS advertisement. Nothing here exercises an ESPHome-style push.
- **Real RF / roaming / weak signal**, and WPA3-SAE specifics: the AP is
  emulated in-process. `--wifi-auth wpa3-sae` or `--wifi-auth wpa2-psk` can pin
  the security mode if a future change needs it.
- **Power loss mid-write** and flash wear: `--save-state` only persists on a
  clean exit; a torn write is not modelled.
- **Release-size and timing characteristics**: the emulator is instruction
  stepped, so wall-clock download throughput means nothing; use the payload
  size check (T6) for space.
- **block 099's health-check deadline**: rollback here is triggered by hand
  (`reset`), not by the bootloader's own timeout, which is not configurable
  through `<ota />`.

---

## 9. Cleanup

```sh
# stop the server (Ctrl-C) and the emulator (timeout/Ctrl-C)
rm -rf build/ota-src build/ota
```

`build/` is disposable and gitignored; `blocks/002-wifi-ota/` stays untouched
by the whole procedure.
