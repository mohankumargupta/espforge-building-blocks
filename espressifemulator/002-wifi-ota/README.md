# 002-wifi-ota — the happy path, by hand

Block `002-wifi-ota` upgrades itself from an HTTP server: it downloads an
application image, verifies it, writes it to the inactive `ota_1` slot, and
reboots into it. This guide walks that path once, manually, in three terminals,
on the ESP32-C3 emulator (`esp-emu`) — no test harness deciding anything for you.

Budget five minutes and two rebuilds. When it works you will have seen the
emulated ROM, the real IDF second-stage bootloader, the partition table,
`otadata`, and your app all cooperating.

Every espforge invocation below goes through the root `justfile` (`just build`,
`just espforge`, `just esp-emu`), per `AGENTS.md`. Everything is `esp32c3`.
The full test matrix (T0–T7, including the failure paths) lives in
[`../../testota.md`](../../testota.md).

---

## What is on the wire

```
host                                       esp-emu (esp32c3)
python3 -m http.server 127.0.0.1:8000      app v1 in ota_0 @ 0x10000
  build/ota/                              ─────────────────────────
    blink.bin  ◄── GET /blink.bin ─────── ctx.ota.begin / write / finish
    v1.bin (this is flashed)              writes image ─► ota_1 @ 0x200000
                                          writes        ─► otadata @ 0xd000
                                          then reboots into ota_1
       192.168.4.1 = gateway ──► host loopback
       guest station = 192.168.4.2
```

The device asks for `http://192.168.4.1:8000/blink.bin`. Under `--net user`
the emulator routes the *gateway* address to the host's loopback, so the python
server never has to bind anything but `127.0.0.1`. No port forwarding is
involved.

Two different kinds of image are in play, and mixing them up is the most common
mistake:

| Image | Kind | Why |
|---|---|---|
| `build/ota/v1.bin` | **merged** (bootloader + partition table + app) | you flash this; it must be a complete bootable flash |
| `build/ota/blink.bin` | **app image only** | the OTA payload; a merged image would copy a bootloader and partition table into `ota_1` |

## Prerequisites

- `esp-emu` v1.0.0 on `PATH` (all runs are `esp32c3`)
- `cargo espflash` for the image extraction steps
- `python3`, `just`, `nc` (netcat)
- `espforge/target/debug/espforge` — run `just build_espforge` if it is missing

## 1. Build what you are going to run

```sh
just ota-fixtures
```

That produces, in `build/ota/`:

| File | What it is |
|---|---|
| `v1.bin` + `v1.elf` | the updater: block 002's WiFi + `<http_client>` + `<ota />`, merged, constants matching `blink.bin` |
| `blink.bin` + `blink.elf` | the payload: the espforge `blink` example as a bare app image |
| `base.bin` + `base.elf` | block 002 exactly as shipped (only needed for the Wi-Fi baseline, T0) |
| `blink.fw.bin` | `blink` plus `<ota />` and `mark_app_valid()` (used by T4, not by this walkthrough) |

Confirm the payload's numbers now, because they are the contract the device
enforces:

```sh
just ota-image-info build/ota/blink.bin
```

```text
image: build/ota/blink.bin
size:  106816 (0x1a140) bytes
crc32: 0x6923ba79
slot:  2031616 (0x1f0000) bytes  [T6]
fits:  yes
rust:
const IMAGE_SIZE: u32 = 106816;
const IMAGE_CRC32: u32 = 0x6923ba79;
```

The `size` and `crc32` printed here are the exact values the updater was
compiled with, and the ones you must see in the `ota begin` line below. (The
constants live at the top of `v1/src/app.rs`; `just ota-patch-v1` writes them,
which is why that file shows up as modified in git.)

## 2. Terminal 1 — the update server

```sh
just ota-serve
```

This is `python3 -m http.server 8000 --bind 127.0.0.1 --directory build/ota`.
Leave it running for the whole walkthrough. You will see exactly one request
when it works:

```text
127.0.0.1 - - [14/Nov/2026 …] "GET /blink.bin HTTP/1.1" 200 -
```

## 3. Terminal 2 — boot the updater

```sh
just esp-emu "--chip esp32c3 --firmware build/ota/v1.bin \
  --net user --wifi-ssid MyNetwork --wifi-password MyPassword \
  --control-tcp 127.0.0.1:5558 \
  --elf build/ota/v1.elf --elf build/ota/blink.elf \
  --timeout 180s --log-file build/ota/manual.log"
```

Why each flag is there:

| Flag | Meaning |
|---|---|
| `--net user` | NAT; `192.168.4.1` becomes the host loopback |
| `--wifi-ssid MyNetwork --wifi-password MyPassword` | must match the block's `<wifi_station>` literals — this is the emulated soft AP |
| `--control-tcp 127.0.0.1:5558` | a side channel for `reset` and `read-region`, so you can inspect and reboot without restarting the emulator |
| `--elf … --elf …` | gives the emulator symbols for **both** firmwares, because after the reboot the payload is what is running |
| `--timeout 180s` | bounds the run; the emulator exits on its own |
| `--log-file build/ota/manual.log` | keeps the emulator's own log in a file, separate from the guest console |

**The firmware console is this terminal's stdout.** Do not redirect it if you
want to watch — `--log-file` receives only the emulator's own log lines
(ROM/bootloader), not the app's `INFO - …` output.

## 4. What you should see

```text
I (58) boot: Loaded app from partition at offset 0x10000        ← v1, from ota_0
INFO - TEST: wifi-ota: connecting...
INFO - TEST: ip 192.168.4.2/24                                  ← DHCP lease
INFO - TEST: ota begin size=106816 crc=0x6923ba79               ← matches step 1
INFO - TEST: ota 0%
INFO - TEST: ota 10%
…
INFO - TEST: ota 100%
INFO - TEST: ota done, rebooting
I (15) boot: Loaded app from partition at offset 0x200000       ← payload, from ota_1
INFO - blink example                                            ← it is alive
```

Read that second boot line carefully: `0x200000` is `ota_1`, not the `0x10000`
the device started from. The updater streamed the image straight into the
inactive slot — it never buffers the whole 106 KB in RAM — then flipped
`otadata` and reset itself.

After `blink example` there are **no more `TEST:` lines**, and that is correct:
the thing now running is the plain `blink` example, not the updater. It prints
one line at boot and then toggles its LED forever.

## 5. Terminal 3 — prove it really moved

While the emulator is still up, decode `otadata` through the control channel:

```sh
just ota-otadata 127.0.0.1:5558 build/ota/manual-otadata.bin
```

```text
sector0: seq=1 state=0x2 crc=0x4743989a
sector1: seq=2 state=0x0 crc=0x55f63774
```

Both 4 KiB sectors are decoded, because only one of them is the live entry:

- `sector0` — `seq=1`, `state=0x2` (`VALID`): that is v1 in `ota_0`, untouched
  and still bootable.
- `sector1` — `seq=2`, `state=0x0` (`NEW`): that is `blink` in `ota_1`, written by
  `finish()`. The bootloader picks the higher `seq`, which is why the device
  came back on `0x200000`.

`NEW` rather than `VALID` is deliberate: this test firmware never calls
`ctx.ota.mark_app_valid()`. On real hardware with a rollback-enabled bootloader,
that is exactly the state that gets reverted on the next reset — the subject of
T3 in `testota.md`, and of the limitation below.

You can also reboot the running emulator without losing it:

```sh
just ota-reset
```

## 6. Run the happy path a second time

Stop the emulator (Ctrl-C, or wait for `--timeout`). Emulated flash is
discarded on exit unless you ask for it to be kept, so running step 3 again
starts from `ota_0` with an erased `otadata` — a clean repeat, not a continuation.

To instead demonstrate *persistence* across processes, work on a copy and let
the emulator write flash state back to it on exit:

```sh
just ota-state-copy    # build/ota/v1.bin -> build/ota/v1-state.bin
just esp-emu "--chip esp32c3 --firmware build/ota/v1-state.bin \
  --net user --wifi-ssid MyNetwork --wifi-password MyPassword \
  --control-tcp 127.0.0.1:5558 --save-state \
  --timeout 180s --log-file build/ota/manual-state.log"
```

Run the update once, let it reboot, then exit. Launching the same file again
now boots `ota_1` directly (`Loaded app from partition at offset 0x200000`,
`blink example`) with no update and no server needed. Never point `--save-state`
at `v1.bin` itself; the recipe copies first for exactly that reason.

## 7. Where this happy path stops

- **Rollback does not fire.** After the update, a reset keeps booting `ota_1`
  instead of reverting to v1. The second-stage bootloader inside a merged image
  comes from `espflash save-image --merge`, which ships without
  `CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE`; it never promotes the `NEW` entry to
  `PENDING_VERIFY`, so there is nothing to abort. `just ota-t3` reproduces and
  explains this. It is a firmware-tooling gap, not a `<ota />` defect: the slot
  selection, the write, and the reboot above are all real.
- **Failure paths are separate cases**, each with a recipe: corrupt payload
  (`just ota-blink-bad`, then `just ota-serve 8000 build/ota-bad`, then
  `just ota-t5`) and unreachable server (stop the server, then `just ota-t7`).

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `TEST: no payload configured` | `IMAGE_SIZE` in `v1/src/app.rs` is `0`; run `just ota-patch-v1` |
| `TEST: ota begin failed: … size` | `blink.bin` was rebuilt after v1; rerun `just ota-v1` |
| `TEST: ota finish failed: image crc mismatch` | the file served is not the one v1 was patched with — often `build/ota-bad` still being served; serve `build/ota` |
| `TEST: download failed: Http(Request)` | server not running, wrong port, or wrong `--directory` |
| stuck at `TEST: wifi-ota: connecting...` | credentials must be exactly `MyNetwork` / `MyPassword` |
| update ran but the next boot is `0x10000` again | `TEST: ota done, rebooting` never appeared; the write or verification failed, so nothing selected `ota_1` |
| boot log missing from the terminal | the console goes to stdout; check you did not redirect it, and that `--log-file`'s path is a different file |

## Cleanup

```sh
just ota-clean
```

`build/` is disposable and gitignored. The only tracked file a fixture build
rewrites is `v1/src/app.rs` (its two constants).

## Files here

| Path | Role |
|---|---|
| `v1/` | the updater used above: block 002's WiFi plus `<http_client>`, streaming `begin`/`write`/`finish`, no `mark_app_valid()` |
| `blink-confirm/` | the `blink` payload plus `<ota />` and `mark_app_valid()` (T4) |
| `scripts/image-info.py` | size/CRC32/slot fit, `--merged` mode, `--patch` for the v1 constants |
| `../../justfile` | every `ota-*` recipe used above |
