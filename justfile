set shell := ["bash", "-cu"]

espforge := justfile_directory() / "espforge" / "target" / "debug" / "espforge"
runtime := justfile_directory() / "espforge" / "espforge-runtime"
build_dir := justfile_directory() / "build"

ota_project := justfile_directory() / "espressifemulator" / "002-wifi-ota"
ota_dir := build_dir / "ota"
ota_src := build_dir / "ota-src"
ota_tool := ota_project / "scripts" / "image-info.py"

sht31_dir := justfile_directory() / "drivers" / "sht31"
sht31_project := sht31_dir / "firmware"
sht31_out := build_dir / "sht31"
sht31_src := build_dir / "sht31-src"
sht31_port := "127.0.0.1:5559"

@cat-all pattern="**/*":
    shopt -s globstar nullglob; files=(); for f in {{pattern}}; do [[ -f "$f" ]] && files+=("$f"); done; (( ${#files[@]} )) && bat "${files[@]}"

@cat-plain pattern="**/*":
    shopt -s globstar nullglob; files=(); for f in {{pattern}}; do [[ -f "$f" ]] && files+=("$f"); done; (( ${#files[@]} )) && bat --paging=never "${files[@]}"

build_espforge:
    (cd espforge; cargo build)

espforge command:
    (espforge/target/debug/espforge {{ command }})

check example:
    (cd {{ example}}; {{ espforge }} check)

gen example:
    (cd {{ example }}; {{ espforge }} gen --out {{ build_dir }} --runtime-path {{ runtime }})

build example:
    (cd {{ example }}; {{ espforge }} build --out {{ build_dir }}/build --runtime-path {{ runtime }})

esp-emu args:
    esp-emu {{ args }}


# ---- 002-wifi-ota emulator fixtures (testota.md) -------------------------

# Build every fixture testota.md names into build/ota/ (patches v1 constants).
ota-fixtures: ota-base ota-v1 ota-blink-confirm
    @echo "fixtures in {{ ota_dir }}:"
    @ls -l "{{ ota_dir }}"

# T0/T1 baseline: blocks/002-wifi-ota as shipped -> build/ota/base.bin (merged).
ota-base:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ota_dir }}"
    just build blocks/002-wifi-ota
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            --partition-table partitions.csv --target-app-partition ota_0 \
            "{{ ota_dir }}/base.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/wifi_ota" \
        "{{ ota_dir }}/base.elf"
    python3 "{{ ota_tool }}" --merged "{{ ota_dir }}/base.bin"

# OTA payload: the espforge blink example, app image only -> build/ota/blink.bin.
ota-blink:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ota_dir }}" "{{ ota_src }}"
    rm -rf "{{ ota_src }}/blink"
    just espforge "new --example blink --chip esp32c3 --dir build/ota-src/blink"
    just build build/ota-src/blink
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --chip esp32c3 --flash-size 4mb \
            "{{ ota_dir }}/blink.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/blink" \
        "{{ ota_dir }}/blink.elf"
    python3 "{{ ota_tool }}" "{{ ota_dir }}/blink.bin"

# Write the payload's size/CRC32 into v1/src/app.rs (rebuilds blink first).
ota-patch-v1 image=(ota_dir / "blink.bin"): ota-blink
    python3 "{{ ota_tool }}" --patch "{{ ota_project }}/v1/src/app.rs" "{{ image }}"

# Build the test-only OTA client as a merged image -> build/ota/v1.bin.
ota-build-v1:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ota_dir }}"
    just check espressifemulator/002-wifi-ota/v1
    just build espressifemulator/002-wifi-ota/v1
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            --partition-table partitions.csv --target-app-partition ota_0 \
            "{{ ota_dir }}/v1.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/wifi_ota_v1" \
        "{{ ota_dir }}/v1.elf"
    python3 "{{ ota_tool }}" --merged "{{ ota_dir }}/v1.bin"

# v1 with constants matching the current blink payload (the T2/T3 fixture).
ota-v1: ota-blink ota-patch-v1 ota-build-v1

# T4 payload: blink + <ota /> + mark_app_valid() -> build/ota/blink.fw.bin.
ota-blink-confirm:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ota_dir }}"
    just build espressifemulator/002-wifi-ota/blink-confirm
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --chip esp32c3 --flash-size 4mb \
            "{{ ota_dir }}/blink.fw.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/blink_confirm" \
        "{{ ota_dir }}/blink-confirm.elf"
    python3 "{{ ota_tool }}" "{{ ota_dir }}/blink.fw.bin"

# T5 corpus: one flipped byte at offset 4 KiB, same length, served as blink.bin.
ota-blink-bad: ota-blink
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ota_dir }}-bad"
    python3 - "{{ ota_dir }}/blink.bin" "{{ ota_dir }}-bad/blink.bin" <<'PY'
    import pathlib, sys, zlib
    src, dst = (pathlib.Path(arg) for arg in sys.argv[1:3])
    data = bytearray(src.read_bytes())
    data[4096] ^= 0xFF
    dst.write_bytes(bytes(data))
    print(f"bad payload: {len(data)} bytes, crc32 0x{zlib.crc32(bytes(data)) & 0xffffffff:08x}")
    PY
    echo "T5: just ota-serve 8000 {{ ota_dir }}-bad   then   just ota-t5"

# Size, CRC32 and slot fit for an OTA image (T6). Prints the Rust constants.
ota-image-info image=(ota_dir / "blink.bin"):
    python3 "{{ ota_tool }}" "{{ image }}"

# Serve a fixture directory to the guest's gateway (192.168.4.1:8000 -> loopback).
ota-serve port="8000" dir=ota_dir:
    python3 -m http.server {{ port }} --bind 127.0.0.1 --directory "{{ dir }}"

# T0/T1: boot block 002 as shipped and watch it associate.
ota-t0 timeout="45s":
    just esp-emu "--chip esp32c3 --firmware {{ ota_dir }}/base.bin --net user --wifi-ssid MyNetwork --wifi-password MyPassword --timeout {{ timeout }} --log-file {{ ota_dir }}/t0.log"

# T2-T5/T7: run the v1 updater with the control channel open.
ota-t2 timeout="120s":
    just esp-emu "--chip esp32c3 --firmware {{ ota_dir }}/v1.bin --net user --wifi-ssid MyNetwork --wifi-password MyPassword --control-tcp 127.0.0.1:5558 --elf {{ ota_dir }}/v1.elf --elf {{ ota_dir }}/blink.elf --timeout {{ timeout }} --log-file {{ ota_dir }}/t2.log"

# T3: boot the payload, reset, and prove the bootloader rolls back to `ota_0`.
#
# The guest console is the emulator's stdout (`$out`); `--log-file` only gets
# the emulator's own ROM/bootloader log, so the assertions read `$out`.
ota-t3 timeout="300s" wait="150":
    #!/usr/bin/env bash
    set -euo pipefail
    log="{{ ota_dir }}/t3.log"
    out="{{ ota_dir }}/t3.stdout"
    : > "$log"
    setsid just ota-t2 {{ timeout }} > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    booted=0
    for _ in $(seq 1 {{ wait }}); do
        grep -q 'blink example' "$out" && { booted=1; break; }
        sleep 1
    done
    (( booted )) || { echo "T3 FAIL: the payload never booted; see $out" >&2; exit 1; }
    echo "T3: payload booted; otadata before reset:"
    just ota-otadata 127.0.0.1:5558 "{{ ota_dir }}/otadata-t3-before.bin"
    just ota-reset 127.0.0.1:5558
    back=0
    for _ in $(seq 1 {{ wait }}); do
        (( $(grep -c 'TEST: ota begin' "$out" || true) >= 2 )) && { back=1; break; }
        sleep 1
    done
    boots=$(grep -c 'blink example' "$out" || true)
    echo "T3: payload boots=$boots (expect 1)"
    echo "T3: otadata after reset:"
    just ota-otadata 127.0.0.1:5558 "{{ ota_dir }}/otadata-t3-after.bin"
    if (( ! back )); then
        echo "T3 FAIL: v1 did not come back; the payload booted again." >&2
        echo "        If the new entry above stays state=0x0 (NEW), the second-stage" >&2
        echo "        bootloader from 'espflash save-image --merge' ignores slot state" >&2
        echo "        (no CONFIG_BOOTLOADER_APP_ROLLBACK_ENABLE), so rollback cannot fire." >&2
        exit 1
    fi
    (( boots == 1 )) || { echo "T3 FAIL: the payload booted $boots times, not 1" >&2; exit 1; }
    echo "T3 PASS: reset rolled back to ota_0"

# T5: a same-length payload with a flipped byte must fail verification.
ota-t5 timeout="240s" wait="180":
    #!/usr/bin/env bash
    set -euo pipefail
    if ! python3 -c 'import socket,sys; s=socket.socket(); s.settimeout(0.5); sys.exit(0 if s.connect_ex(("127.0.0.1",8000))==0 else 1)'; then
        echo "T5: serve the bad payload first: just ota-serve 8000 {{ ota_dir }}-bad" >&2
        exit 1
    fi
    log="{{ ota_dir }}/t5.log"
    out="{{ ota_dir }}/t5.stdout"
    : > "$log"
    setsid just ota-t2 {{ timeout }} > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    rejected=0
    for _ in $(seq 1 {{ wait }}); do
        grep -q 'TEST: ota finish failed' "$out" && { rejected=1; break; }
        sleep 1
    done
    grep -q 'TEST: ip ' "$out" || { echo "T5 FAIL: v1 never reached the network; see $out" >&2; exit 1; }
    (( rejected )) || { echo "T5 FAIL: the bad image was not rejected; see $out" >&2; exit 1; }
    if grep -q 'TEST: ota done, rebooting' "$out"; then echo "T5 FAIL: the bad image was activated" >&2; exit 1; fi
    if grep -q 'blink example' "$out"; then echo "T5 FAIL: the payload booted" >&2; exit 1; fi
    echo "T5 PASS: corrupt payload rejected, v1 still running"

# T7: with no update server, v1 must fail the download and keep running.
ota-t7 timeout="240s" wait="180":
    #!/usr/bin/env bash
    set -euo pipefail
    if python3 -c 'import socket,sys; s=socket.socket(); s.settimeout(0.5); sys.exit(0 if s.connect_ex(("127.0.0.1",8000))==0 else 1)'; then
        echo "T7: stop the fixture server first (something is listening on 8000)" >&2
        exit 1
    fi
    log="{{ ota_dir }}/t7.log"
    out="{{ ota_dir }}/t7.stdout"
    : > "$log"
    setsid just ota-t2 {{ timeout }} > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    failed=0
    for _ in $(seq 1 {{ wait }}); do
        grep -q 'TEST: download failed' "$out" && { failed=1; break; }
        sleep 1
    done
    (( failed )) || { echo "T7 FAIL: no download failure within {{ wait }}s; see $out" >&2; exit 1; }
    if grep -q 'TEST: ota done, rebooting' "$out"; then echo "T7 FAIL: it rebooted despite the failure" >&2; exit 1; fi
    if grep -q 'blink example' "$out"; then echo "T7 FAIL: the payload booted" >&2; exit 1; fi
    grep -q 'TEST: ip ' "$out" || { echo "T7 FAIL: v1 was not on the network; see $out" >&2; exit 1; }
    echo "T7 PASS: download failed, device stayed on v1"

# T3/T4: hard reset the running emulator (same as a DTR/RTS toggle).
#
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
ota-reset addr="127.0.0.1:5558":
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ addr }}"
    printf 'reset\n' | nc -q1 "${addr%:*}" "${addr##*:}"

# Evidence for T2/T3/T5: dump the 32-byte otadata region.
ota-otadata addr="127.0.0.1:5558" out=(ota_dir / "otadata.bin"):
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ addr }}"
    printf 'read-region 0xd000 8192 %s\n' "{{ out }}" | nc -q1 "${addr%:*}" "${addr##*:}"
    # otadata is two 4 KiB sectors, each with one 32-byte entry at its start.
    # The active slot is the entry with the higher `seq`, so decode both.
    python3 - "{{ out }}" <<'PY'
    import sys
    data = open(sys.argv[1], "rb").read()
    for sector in range(2):
        entry = data[sector * 0x1000 : sector * 0x1000 + 32]
        if len(entry) < 32:
            print(f"sector{sector}: short read ({len(entry)} bytes)")
            continue
        seq = int.from_bytes(entry[0:4], "little")
        # 0 NEW, 1 PENDING_VERIFY, 2 VALID, 3 INVALID, 4 ABORTED, 0xffffffff UNDEFINED
        state = int.from_bytes(entry[24:28], "little")
        crc = int.from_bytes(entry[28:32], "little")
        print(f"sector{sector}: seq={seq} state={state:#x} crc={crc:#010x}")
    PY

# T3b: work on a copy so --save-state never rewrites the pristine v1 image.
ota-state-copy:
    cp "{{ ota_dir }}/v1.bin" "{{ ota_dir }}/v1-state.bin"

# Remove the disposable fixtures (build/ is gitignored).
ota-clean:
    rm -rf "{{ ota_dir }}" "{{ ota_dir }}-bad" "{{ ota_src }}"


# ---- sht31 driver fixture (testsht31.md) --------------------------------

# Schema and resource graph for the SHT31 test firmware.
sht31-check:
    just check {{ sht31_project }}

# Build the test firmware as a merged image -> build/sht31/sht31.bin (+ .elf).
sht31-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ sht31_out }}"
    just check "{{ sht31_project }}"
    just build "{{ sht31_project }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ sht31_out }}/sht31.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/sht31_test" \
        "{{ sht31_out }}/sht31.elf"
    ls -l "{{ sht31_out }}/sht31.bin" "{{ sht31_out }}/sht31.elf"

# Run the firmware with the virtual sensor on the control channel.
# `board` defaults to the checked-in board; testsht31.md uses the variants.
sht31-run timeout="90s" board=(sht31_dir / "board.json"):
    just esp-emu "--chip esp32c3 --firmware {{ sht31_out }}/sht31.bin --cosim-board {{ board }} --control-tcp {{ sht31_port }} --elf {{ sht31_out }}/sht31.elf --timeout {{ timeout }} --log-file {{ sht31_out }}/emu.log"

# Send one control line to the running emulator, e.g. "part sensor input warmer".
#
# `sht31_port` stays host:port for esp-emu; netcat-openbsd wants them separate.
sht31-part args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ sht31_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# T1: scan the bus from the espforge i2c_scanner example, on the board's pins.
sht31-scan:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ sht31_out }}" "{{ sht31_src }}"
    rm -rf "{{ sht31_src }}/scanner"
    just espforge "new --example i2c_scanner --chip esp32c3 --dir build/sht31-src/scanner"
    sed -i -E 's/sda="[0-9]+" scl="[0-9]+"/sda="5" scl="6"/' \
        "{{ sht31_src }}/scanner/i2c_scanner.espf"
    # The generated project needs the same esp-hal I2C overlay as the firmware:
    # without it every transaction fails `AcknowledgeCheckFailed(Data)`.
    cp "{{ sht31_project }}/Cargo.extra.toml" "{{ sht31_src }}/scanner/Cargo.extra.toml"
    just build "{{ sht31_src }}/scanner"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ sht31_out }}/scanner.bin")
    out="{{ sht31_out }}/scan.stdout"
    : > "$out"
    setsid just esp-emu "--chip esp32c3 --firmware {{ sht31_out }}/scanner.bin --cosim-board {{ sht31_dir }}/board.json --timeout 25s --log-file {{ sht31_out }}/scan-emu.log" > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    for _ in $(seq 1 25); do
        grep -q 'Scan complete' "$out" && break
        sleep 1
    done
    if ! grep -q 'Found device at address 0x44' "$out"; then
        echo "FAIL: T1 the sensor at 0x44 was not found; see $out" >&2; exit 1
    fi
    for stray in 0x45 0x50; do
        if grep -q "Found device at address $stray" "$out"; then
            echo "FAIL: T1 something answered at $stray; see $out" >&2; exit 1
        fi
    done
    echo "PASS: T1 scan finds 0x44 and nothing else"

# T0/T2-T5: boot, read status and room conditions, move the part, set values.
sht31-sensor: sht31-image
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ sht31_out }}/sensor.stdout"
    : > "$out"
    setsid just sht31-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: sht31 boot" "T0 firmware boots" 40
    wait_for "TEST: status=0x8010" "T2 status after the driver's soft reset" 20
    wait_for "TEST: reading t=22.50 rh=45.00" "T3 room conditions 22.50 C / 45.00 %RH" 20
    just sht31-part "part sensor input warmer" >/dev/null
    just sht31-part "part sensor input wetter" >/dev/null
    wait_for "TEST: reading t=23.50 rh=50.00" "T4 warmer + wetter -> 23.50 C / 50.00 %RH" 20
    just sht31-part "part sensor input set:temperature=31.5" >/dev/null
    just sht31-part "part sensor input set:humidity=62" >/dev/null
    wait_for "TEST: reading t=31.50 rh=62.00" "T5 set -> 31.50 C / 62.00 %RH" 20
    echo "sht31-sensor PASS"

# T7/T8: wrong address and wrong pins must fail initialization.
sht31-negative: sht31-image sht31-boards
    #!/usr/bin/env bash
    set -euo pipefail
    neg() {
        local name="$1" board="$2" out="{{ sht31_out }}/neg-$1.stdout"
        : > "$out"
        setsid just sht31-run 30s "$board" > "$out" 2>&1 &
        local emu=$! found=1
        for _ in $(seq 1 25); do
            grep -q 'initialization failed' "$out" && { found=0; break; }
            sleep 1
        done
        kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true
        if (( found )); then echo "FAIL: $name never failed initialization; see $out" >&2; return 1; fi
        if grep -q 'TEST: reading' "$out"; then echo "FAIL: $name produced a reading" >&2; return 1; fi
        echo "PASS: $name: air did not initialize"
    }
    neg addr45 "{{ sht31_out }}/board-0x45.json"
    neg scl7 "{{ sht31_out }}/board-scl7.json"

# Variant boards for the negative cases; each sits beside a copy of the part.
sht31-boards:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ sht31_out }}"
    cp "{{ sht31_dir }}/sht31.wasm" "{{ sht31_out }}/sht31.wasm"
    printf '%s\n' '{ "parts": { "sensor": { "module": "sht31.wasm", "pins": { "SCL": 6, "SDA": 5 }, "addr": "0x45" } } }' \
        > "{{ sht31_out }}/board-0x45.json"
    printf '%s\n' '{ "parts": { "sensor": { "module": "sht31.wasm", "pins": { "SCL": 7, "SDA": 5 } } } }' \
        > "{{ sht31_out }}/board-scl7.json"

# Remove the disposable SHT31 fixtures; the driver folder stays.
sht31-clean:
    rm -rf "{{ sht31_out }}" "{{ sht31_src }}"


# ---- TMP123 external driver fixture (drivers/tmp123) ---------------------

tmp123_rel := "drivers/tmp123"
tmp123_dir := justfile_directory() / tmp123_rel
tmp123_out := build_dir / "tmp123"
tmp123_port := "127.0.0.1:5561"
tmp123_part := justfile_directory() / "esp-emulator" / "examples" / "parts" / "tmp123"

# The thin crate's own tests: the protocol math, on the host, no emulator.
tmp123-driver-test:
    cargo test --manifest-path {{ tmp123_dir }}/tmp123-driver/Cargo.toml

# Build the emulator's virtual TMP123 part -> drivers/tmp123/tmp123.wasm.
tmp123-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ tmp123_part }}/Cargo.toml"
    cp "{{ tmp123_part }}/target/wasm32-unknown-unknown/release/tmp123.wasm" \
        "{{ tmp123_dir }}/tmp123.wasm"
    ls -l "{{ tmp123_dir }}/tmp123.wasm"

# Schema, resource graph, manifest discovery and the driver's path dependency.
tmp123-check:
    just check {{ tmp123_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
tmp123-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ tmp123_out }}"
    just check "{{ tmp123_rel }}"
    just build "{{ tmp123_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ tmp123_out }}/tmp123.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/tmp123_test" \
        "{{ tmp123_out }}/tmp123.elf"
    ls -l "{{ tmp123_out }}/tmp123.bin" "{{ tmp123_out }}/tmp123.elf"

# Run the firmware with the virtual sensor on the control channel.
tmp123-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ tmp123_out }}/tmp123.bin --cosim-board {{ tmp123_dir }}/board.json --control-tcp {{ tmp123_port }} --elf {{ tmp123_out }}/tmp123.elf --timeout {{ timeout }} --log-file {{ tmp123_out }}/emu.log"

# One control line, e.g. "part thermometer input warmer".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
tmp123-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ tmp123_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/tmp123/drivers/tmp123.kdl.
# This is `espforge driver <project dir> add --local <manifest>`: the project
# dir is the positional before the subcommand, and `--local` records the file
# where it is instead of copying it.
tmp123-driver-record:
    just espforge "driver {{ tmp123_rel }} add --local {{ tmp123_rel }}/drivers/tmp123.kdl"

# Happy path: boot, read 25 °C, warmer, then an exact set value.
tmp123-sensor: tmp123-image tmp123-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ tmp123_out }}/sensor.stdout"
    # The co-simulation lines go to --log-file, the firmware console to stdout.
    log="{{ tmp123_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just tmp123-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: tmp123 boot" "firmware boots" 40
    wait_for_log "is the SPI device on chip select GPIO 7" "the part attached on the firmware's CS" 10
    wait_for "TEST: reading t=25.00 c" "fresh part reads 25.00 C" 20
    just tmp123-part-input "part thermometer input warmer" >/dev/null
    wait_for "TEST: reading t=26.00 c" "warmer -> 26.00 C" 20
    just tmp123-part-input "part thermometer input set:temperature=31.5" >/dev/null
    wait_for "TEST: reading t=31.50 c" "set:temperature=31.5 -> 31.50 C" 20
    echo "tmp123-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
tmp123-clean:
    rm -rf {{ tmp123_out }}


# ---- MH-Z19B driver fixture (drivers/mhz19b) ----------------------------

mhz19b_dir := justfile_directory() / "drivers" / "mhz19b"
mhz19b_project := mhz19b_dir / "firmware"
mhz19b_out := build_dir / "mhz19b"
mhz19b_port := "127.0.0.1:5560"

# Schema and resource graph for the MH-Z19B fixture.
mhz19b-check:
    just check {{ mhz19b_project }}

# Build the fixture as a merged image -> build/mhz19b/mhz19b.bin (+ .elf).
mhz19b-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ mhz19b_out }}"
    just check "{{ mhz19b_project }}"
    just build "{{ mhz19b_project }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ mhz19b_out }}/mhz19b.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/mhz19b_test" \
        "{{ mhz19b_out }}/mhz19b.elf"
    ls -l "{{ mhz19b_out }}/mhz19b.bin" "{{ mhz19b_out }}/mhz19b.elf"

# Run the firmware with the virtual sensor on the control channel.
mhz19b-run timeout="90s" board=(mhz19b_dir / "board.json"):
    just esp-emu "--chip esp32c3 --firmware {{ mhz19b_out }}/mhz19b.bin --cosim-board {{ board }} --control-tcp {{ mhz19b_port }} --elf {{ mhz19b_out }}/mhz19b.elf --timeout {{ timeout }} --log-file {{ mhz19b_out }}/emu.log"

# `port` stays host:port for readability; netcat-openbsd wants them separate.
# Send one control line to the running emulator, e.g. "part co2 input more".
mhz19b-part args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ mhz19b_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Happy path: boot, read 400 ppm, set 1200 ppm, read it back.
mhz19b-sensor: mhz19b-image
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ mhz19b_out }}/sensor.stdout"
    : > "$out"
    setsid just mhz19b-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: mhz19b boot" "firmware boots" 40
    wait_for "TEST: co2=400 ppm" "fresh air reads 400 ppm" 20
    just mhz19b-part "part co2 input set:co2=1200" >/dev/null
    wait_for "TEST: co2=1200 ppm" "set:co2=1200 reads back as 1200 ppm" 20
    echo "mhz19b-sensor PASS"

# Remove the disposable MH-Z19B fixture; the driver folder stays.
mhz19b-clean:
    rm -rf {{ mhz19b_out }}


# ---- i2c scanner against the emulator's own bus (drivers/i2cscanner) -----

scanner_project := justfile_directory() / "drivers" / "i2cscanner"
scanner_out := build_dir / "i2cscanner"

# Schema and resource graph for the scanner firmware.
i2cscanner-check:
    just check {{ scanner_project }}

# Build the scanner as a merged image -> build/i2cscanner/scanner.bin (+ .elf).
i2cscanner-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ scanner_out }}"
    just check "{{ scanner_project }}"
    just build "{{ scanner_project }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ scanner_out }}/scanner.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/i2c_scanner" \
        "{{ scanner_out }}/scanner.elf"
    ls -l "{{ scanner_out }}/scanner.bin" "{{ scanner_out }}/scanner.elf"

# Run the scanner with no --cosim-board (emulator's built-in EEPROM at 0x50);
# known red on esp-hal 1.2.2 + esp-emu 1.0.0, see testsht31.md §1a.
i2cscanner-run timeout="30s":
    just esp-emu "--chip esp32c3 --firmware {{ scanner_out }}/scanner.bin --elf {{ scanner_out }}/scanner.elf --timeout {{ timeout }} --log-file {{ scanner_out }}/emu.log"

# T-gate: the scan must find the built-in EEPROM at 0x50 and nothing else.
i2cscanner-test: i2cscanner-image
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ scanner_out }}/scan.stdout"
    : > "$out"
    setsid just i2cscanner-run 30s > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    for _ in $(seq 1 30); do
        grep -q 'Scan complete' "$out" && break
        sleep 1
    done
    grep -q 'Scan complete' "$out" || { echo "FAIL: the scan never completed; see $out" >&2; exit 1; }
    grep -E 'Found device at address|Scan complete' "$out"
    if ! grep -q 'Found device at address 0x50' "$out"; then
        echo "FAIL: the built-in EEPROM at 0x50 was not found; see $out" >&2
        exit 1
    fi
    found=$(grep -c 'Found device at address' "$out" || true)
    if (( found != 1 )); then
        echo "FAIL: $found devices answered, not just 0x50; see $out" >&2
        exit 1
    fi
    echo "PASS: scan found the emulator's built-in EEPROM at 0x50, and nothing else"


# ---- ADXL345 virtual part fixture (drivers/adxl345) ----------------------

adxl345_rel := "drivers/adxl345"
adxl345_dir := justfile_directory() / adxl345_rel
adxl345_out := build_dir / "adxl345"
adxl345_port := "127.0.0.1:5562"
adxl345_part_dir := adxl345_dir / "parts" / "adxl345"
adxl345_part_wasm := adxl345_dir / "parts" / "adxl345.wasm"

# The part's own tests: DEVID, the axis bytes and the inputs, on the host.
adxl345-part-test:
    cargo test --manifest-path {{ adxl345_part_dir }}/Cargo.toml

# Build the virtual ADXL345 -> drivers/adxl345/parts/adxl345.wasm.
adxl345-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ adxl345_part_dir }}/Cargo.toml"
    cp "{{ adxl345_part_dir }}/target/wasm32-unknown-unknown/release/adxl345.wasm" \
        "{{ adxl345_part_wasm }}"
    ls -l "{{ adxl345_part_wasm }}"

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
    rm -rf {{ adxl345_out }} {{ adxl345_part_dir }}/target


# ---- BME280 SPI virtual part fixture (drivers/bme280_spi) ----------------

bme280_spi_rel := "drivers/bme280_spi"
bme280_spi_dir := justfile_directory() / bme280_spi_rel
bme280_spi_out := build_dir / "bme280_spi"
bme280_spi_port := "127.0.0.1:5563"
bme280_spi_part_dir := bme280_spi_dir / "parts" / "bme280_spi"
bme280_spi_part_wasm := bme280_spi_dir / "parts" / "bme280_spi.wasm"

# The part's own tests: ID, the calibration block, the data bytes and the
# inputs, on the host.
bme280-spi-part-test:
    cargo test --manifest-path {{ bme280_spi_part_dir }}/Cargo.toml

# Build the virtual BME280 -> drivers/bme280_spi/parts/bme280_spi.wasm.
bme280-spi-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ bme280_spi_part_dir }}/Cargo.toml"
    cp "{{ bme280_spi_part_dir }}/target/wasm32-unknown-unknown/release/bme280_spi.wasm" \
        "{{ bme280_spi_part_wasm }}"
    ls -l "{{ bme280_spi_part_wasm }}"

# The driver's own tests: the ID check, the writes and the compensation, on
# the host.
bme280-spi-driver-test:
    cargo test --manifest-path {{ bme280_spi_dir }}/bme280-spi-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
bme280-spi-check:
    just check {{ bme280_spi_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
bme280-spi-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ bme280_spi_out }}"
    just check "{{ bme280_spi_rel }}"
    just build "{{ bme280_spi_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ bme280_spi_out }}/bme280_spi.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/bme280_spi_test" \
        "{{ bme280_spi_out }}/bme280_spi.elf"
    ls -l "{{ bme280_spi_out }}/bme280_spi.bin" "{{ bme280_spi_out }}/bme280_spi.elf"

# Run the firmware with the virtual sensor on the control channel.
bme280-spi-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ bme280_spi_out }}/bme280_spi.bin --cosim-board {{ bme280_spi_dir }}/board.json --control-tcp {{ bme280_spi_port }} --elf {{ bme280_spi_out }}/bme280_spi.elf --timeout {{ timeout }} --log-file {{ bme280_spi_out }}/emu.log"

# One control line, e.g. "part climate input warmer".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
bme280-spi-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ bme280_spi_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/bme280_spi/drivers/bme280_spi.kdl.
bme280-spi-driver-record:
    just espforge "driver {{ bme280_spi_rel }} add --local {{ bme280_spi_rel }}/drivers/bme280_spi.kdl"

# Happy path: boot, room conditions, warmer, a pressure step, a humidity step.
bme280-spi-sensor: bme280-spi-image bme280-spi-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ bme280_spi_out }}/sensor.stdout"
    log="{{ bme280_spi_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just bme280-spi-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: bme280_spi boot" "firmware boots" 40
    wait_for_log "is the SPI device on chip select GPIO 7" "the part attached on the firmware's CS" 10
    wait_for "TEST: climate t=22.50 c p=1013.25 hpa rh=45.00 %" "room conditions" 20
    just bme280-spi-part-input "part climate input warmer" >/dev/null
    wait_for "TEST: climate t=23.50 c p=1013.25 hpa rh=45.00 %" "warmer -> 23.50 C" 20
    just bme280-spi-part-input "part climate input set:pressure=1000" >/dev/null
    wait_for "TEST: climate t=23.50 c p=1000.00 hpa rh=45.00 %" "set:pressure=1000 -> 1000.00 hPa" 20
    just bme280-spi-part-input "part climate input set:humidity=62" >/dev/null
    wait_for "TEST: climate t=23.50 c p=1000.00 hpa rh=62.00 %" "set:humidity=62 -> 62.00 %RH" 20
    echo "bme280-spi-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
bme280-spi-clean:
    rm -rf {{ bme280_spi_out }} {{ bme280_spi_part_dir }}/target


# ---- MFRC522 virtual part fixture (drivers/mfrc522) ----------------------

mfrc522_rel := "drivers/mfrc522"
mfrc522_dir := justfile_directory() / mfrc522_rel
mfrc522_out := build_dir / "mfrc522"
mfrc522_port := "127.0.0.1:5564"
mfrc522_part_dir := mfrc522_dir / "parts" / "mfrc522"
mfrc522_part_wasm := mfrc522_dir / "parts" / "mfrc522.wasm"

# The part's own tests: version, soft reset, FIFO, the three frames, BCC/SAK,
# the no-card IRQ and the inputs, on the host.
mfrc522-part-test:
    cargo test --manifest-path {{ mfrc522_part_dir }}/Cargo.toml

# Build the virtual MFRC522 -> drivers/mfrc522/parts/mfrc522.wasm.
mfrc522-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ mfrc522_part_dir }}/Cargo.toml"
    cp "{{ mfrc522_part_dir }}/target/wasm32-unknown-unknown/release/mfrc522.wasm" \
        "{{ mfrc522_part_wasm }}"
    ls -l "{{ mfrc522_part_wasm }}"

# The driver's own tests: the version check, the init writes, the three frames
# and CRC_A, on the host.
mfrc522-driver-test:
    cargo test --manifest-path {{ mfrc522_dir }}/mfrc522-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
mfrc522-check:
    just check {{ mfrc522_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
mfrc522-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ mfrc522_out }}"
    just check "{{ mfrc522_rel }}"
    just build "{{ mfrc522_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ mfrc522_out }}/mfrc522.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/mfrc522_test" \
        "{{ mfrc522_out }}/mfrc522.elf"
    ls -l "{{ mfrc522_out }}/mfrc522.bin" "{{ mfrc522_out }}/mfrc522.elf"

# Run the firmware with the virtual reader on the control channel.
mfrc522-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ mfrc522_out }}/mfrc522.bin --cosim-board {{ mfrc522_dir }}/board.json --control-tcp {{ mfrc522_port }} --elf {{ mfrc522_out }}/mfrc522.elf --timeout {{ timeout }} --log-file {{ mfrc522_out }}/emu.log"

# One control line, e.g. "part reader input absent".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
mfrc522-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ mfrc522_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/mfrc522/drivers/mfrc522.kdl.
mfrc522-driver-record:
    just espforge "driver {{ mfrc522_rel }} add --local {{ mfrc522_rel }}/drivers/mfrc522.kdl"

# Happy path: boot, select DEADBEEF, take the card away, give it another UID.
mfrc522-sensor: mfrc522-image mfrc522-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ mfrc522_out }}/sensor.stdout"
    log="{{ mfrc522_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just mfrc522-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: mfrc522 boot" "firmware boots" 40
    wait_for_log "is the SPI device on chip select GPIO 7" "the part attached on the firmware's CS" 10
    wait_for "TEST: tag uid=deadbeef sak=0x08 atqa=0x0004" "tag DEADBEEF selected with SAK 0x08" 20
    just mfrc522-part-input "part reader input absent" >/dev/null
    wait_for "TEST: tag none" "absent -> no tag" 20
    just mfrc522-part-input "part reader input set:uid=01:02:03:04" >/dev/null
    just mfrc522-part-input "part reader input present" >/dev/null
    wait_for "TEST: tag uid=01020304 sak=0x08 atqa=0x0004" "set:uid=01020304 -> 01020304" 20
    echo "mfrc522-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
mfrc522-clean:
    rm -rf {{ mfrc522_out }} {{ mfrc522_part_dir }}/target

# ---- MPU-9250 virtual part fixture (drivers/mpu9250) ----------------------

mpu9250_rel := "drivers/mpu9250"
mpu9250_dir := justfile_directory() / mpu9250_rel
mpu9250_out := build_dir / "mpu9250"
mpu9250_port := "127.0.0.1:5565"
mpu9250_part_dir := mpu9250_dir / "parts" / "mpu9250"
mpu9250_part_wasm := mpu9250_dir / "parts" / "mpu9250.wasm"

# The part's own tests: the identity, the power-on registers, the fourteen data
# bytes, the self-clearing reset, the inputs and clamps, on the host.
mpu9250-part-test:
    cargo test --manifest-path {{ mpu9250_part_dir }}/Cargo.toml

# Build the virtual MPU-9250 -> drivers/mpu9250/parts/mpu9250.wasm.
mpu9250-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ mpu9250_part_dir }}/Cargo.toml"
    cp "{{ mpu9250_part_dir }}/target/wasm32-unknown-unknown/release/mpu9250.wasm" \
        "{{ mpu9250_part_wasm }}"
    ls -l "{{ mpu9250_part_wasm }}"

# The driver's own tests: the identity check, the reset poll, the configuration
# writes, the fourteen-byte read and the decode, on the host.
mpu9250-driver-test:
    cargo test --manifest-path {{ mpu9250_dir }}/mpu9250-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
mpu9250-check:
    just check {{ mpu9250_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
mpu9250-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ mpu9250_out }}"
    just check "{{ mpu9250_rel }}"
    just build "{{ mpu9250_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ mpu9250_out }}/mpu9250.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/mpu9250_test" \
        "{{ mpu9250_out }}/mpu9250.elf"
    ls -l "{{ mpu9250_out }}/mpu9250.bin" "{{ mpu9250_out }}/mpu9250.elf"

# Run the firmware with the virtual IMU on the control channel.
mpu9250-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ mpu9250_out }}/mpu9250.bin --cosim-board {{ mpu9250_dir }}/board.json --control-tcp {{ mpu9250_port }} --elf {{ mpu9250_out }}/mpu9250.elf --timeout {{ timeout }} --log-file {{ mpu9250_out }}/emu.log"

# One control line, e.g. "part imu input set:az=0".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
mpu9250-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ mpu9250_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/mpu9250/drivers/mpu9250.kdl.
mpu9250-driver-record:
    just espforge "driver {{ mpu9250_rel }} add --local {{ mpu9250_rel }}/drivers/mpu9250.kdl"

# Happy path: boot, read the flat part, then move it on X and turn the gyro.
mpu9250-sensor: mpu9250-image mpu9250-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ mpu9250_out }}/sensor.stdout"
    log="{{ mpu9250_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just mpu9250-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: mpu9250 boot" "firmware boots" 40
    wait_for_log "is the SPI device on chip select GPIO 7" "the part attached on the firmware's CS" 10
    wait_for "TEST: imu ax=0.00 ay=0.00 az=1.00 g gx=0.00 gy=0.00 gz=0.00 dps t=21.00 c" "flat part reads 1 g on Z at 21.00 C" 20
    just mpu9250-part-input "part imu input set:ax=0.5" >/dev/null
    wait_for "TEST: imu ax=0.50 ay=0.00 az=1.00 g" "set:ax=0.5 -> 0.50 g" 20
    just mpu9250-part-input "part imu input set:gz=90" >/dev/null
    wait_for "gz=90.00 dps" "set:gz=90 -> 90.00 dps" 20
    echo "mpu9250-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
mpu9250-clean:
    rm -rf {{ mpu9250_out }} {{ mpu9250_part_dir }}/target


# ---- BME280 I2C virtual part fixture (drivers/bme280_i2c) ----------------

bme280_i2c_rel := "drivers/bme280_i2c"
bme280_i2c_dir := justfile_directory() / bme280_i2c_rel
bme280_i2c_out := build_dir / "bme280_i2c"
bme280_i2c_port := "127.0.0.1:5566"
bme280_i2c_part_dir := bme280_i2c_dir / "parts" / "bme280_i2c"
bme280_i2c_part_wasm := bme280_i2c_dir / "parts" / "bme280_i2c.wasm"

# The part's own tests: ID, the register file, the data bytes and the inputs,
# on the host.
bme280-i2c-part-test:
    cargo test --manifest-path {{ bme280_i2c_part_dir }}/Cargo.toml

# Build the virtual BME280 -> drivers/bme280_i2c/parts/bme280_i2c.wasm.
bme280-i2c-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ bme280_i2c_part_dir }}/Cargo.toml"
    cp "{{ bme280_i2c_part_dir }}/target/wasm32-unknown-unknown/release/bme280_i2c.wasm" \
        "{{ bme280_i2c_part_wasm }}"
    ls -l "{{ bme280_i2c_part_wasm }}"

# The driver's own tests: the ID check, the configuration writes and the
# compensation, on the host.
bme280-i2c-driver-test:
    cargo test --manifest-path {{ bme280_i2c_dir }}/bme280-i2c-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
bme280-i2c-check:
    just check {{ bme280_i2c_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
bme280-i2c-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ bme280_i2c_out }}"
    just check "{{ bme280_i2c_rel }}"
    just build "{{ bme280_i2c_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ bme280_i2c_out }}/bme280_i2c.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/bme280_i2c_test" \
        "{{ bme280_i2c_out }}/bme280_i2c.elf"
    ls -l "{{ bme280_i2c_out }}/bme280_i2c.bin" "{{ bme280_i2c_out }}/bme280_i2c.elf"

# Run the firmware with the virtual sensor on the control channel.
bme280-i2c-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ bme280_i2c_out }}/bme280_i2c.bin --cosim-board {{ bme280_i2c_dir }}/board.json --control-tcp {{ bme280_i2c_port }} --elf {{ bme280_i2c_out }}/bme280_i2c.elf --timeout {{ timeout }} --log-file {{ bme280_i2c_out }}/emu.log"

# One control line, e.g. "part climate input warmer".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
bme280-i2c-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ bme280_i2c_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/bme280_i2c/drivers/bme280_i2c.kdl.
bme280-i2c-driver-record:
    just espforge "driver {{ bme280_i2c_rel }} add --local {{ bme280_i2c_rel }}/drivers/bme280_i2c.kdl"

# Happy path: boot, room conditions, warmer, a pressure step, a humidity step.
bme280-i2c-sensor: bme280-i2c-image bme280-i2c-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ bme280_i2c_out }}/sensor.stdout"
    log="{{ bme280_i2c_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just bme280-i2c-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: bme280_i2c boot" "firmware boots" 40
    wait_for_log "answers I2C 0x76 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: climate t=22.50 c p=1013.25 hpa rh=45.00 %" "room conditions" 20
    just bme280-i2c-part-input "part climate input warmer" >/dev/null
    wait_for "TEST: climate t=23.50 c p=1013.25 hpa rh=45.00 %" "warmer -> 23.50 C" 20
    just bme280-i2c-part-input "part climate input set:pressure=1000" >/dev/null
    wait_for "TEST: climate t=23.50 c p=1000.00 hpa rh=45.00 %" "set:pressure=1000 -> 1000.00 hPa" 20
    just bme280-i2c-part-input "part climate input set:humidity=62" >/dev/null
    wait_for "TEST: climate t=23.50 c p=1000.00 hpa rh=62.00 %" "set:humidity=62 -> 62.00 %RH" 20
    echo "bme280-i2c-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
bme280-i2c-clean:
    rm -rf {{ bme280_i2c_out }} {{ bme280_i2c_part_dir }}/target


# ---- ADS1x15 virtual part fixture (drivers/ads1x15) -----------------------

ads1x15_rel := "drivers/ads1x15"
ads1x15_dir := justfile_directory() / ads1x15_rel
ads1x15_out := build_dir / "ads1x15"
ads1x15_port := "127.0.0.1:5567"
ads1x15_part_dir := ads1x15_dir / "parts" / "ads1x15"
ads1x15_part_wasm := ads1x15_dir / "parts" / "ads1x15.wasm"

# The part's own tests: the register file, the conversions, the gain and the
# inputs, on the host.
ads1x15-part-test:
    cargo test --manifest-path {{ ads1x15_part_dir }}/Cargo.toml

# Build the virtual ADS1115 -> drivers/ads1x15/parts/ads1x15.wasm.
ads1x15-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ ads1x15_part_dir }}/Cargo.toml"
    cp "{{ ads1x15_part_dir }}/target/wasm32-unknown-unknown/release/ads1x15.wasm" \
        "{{ ads1x15_part_wasm }}"
    ls -l "{{ ads1x15_part_wasm }}"

# The driver's own tests: the configuration write and the decode, on the host.
ads1x15-driver-test:
    cargo test --manifest-path {{ ads1x15_dir }}/ads1x15-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
ads1x15-check:
    just check {{ ads1x15_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
ads1x15-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ads1x15_out }}"
    just check "{{ ads1x15_rel }}"
    just build "{{ ads1x15_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ ads1x15_out }}/ads1x15.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/ads1x15_test" \
        "{{ ads1x15_out }}/ads1x15.elf"
    ls -l "{{ ads1x15_out }}/ads1x15.bin" "{{ ads1x15_out }}/ads1x15.elf"

# Run the firmware with the virtual ADC on the control channel.
ads1x15-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ ads1x15_out }}/ads1x15.bin --cosim-board {{ ads1x15_dir }}/board.json --control-tcp {{ ads1x15_port }} --elf {{ ads1x15_out }}/ads1x15.elf --timeout {{ timeout }} --log-file {{ ads1x15_out }}/emu.log"

# One control line, e.g. "part adc input set:ain0=1.25".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
ads1x15-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ ads1x15_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/ads1x15/drivers/ads1x15.kdl.
ads1x15-driver-record:
    just espforge "driver {{ ads1x15_rel }} add --local {{ ads1x15_rel }}/drivers/ads1x15.kdl"

# Happy path: boot, the four channels, a differential pair, two set inputs.
ads1x15-sensor: ads1x15-image ads1x15-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ ads1x15_out }}/sensor.stdout"
    log="{{ ads1x15_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just ads1x15-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: ads1x15 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x48 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: adc a0=0.512 a1=1.024 a2=1.500 a3=0.128 v" "the four channels at their defaults" 20
    wait_for "TEST: adc a0-a1=-0.512 v" "AIN0 - AIN1 reads -0.512 V" 20
    just ads1x15-part-input "part adc input set:ain2=1" >/dev/null
    wait_for "TEST: adc a0=0.512 a1=1.024 a2=1.000 a3=0.128 v" "set:ain2=1 -> 1.000 V" 20
    just ads1x15-part-input "part adc input set:ain0=1.25" >/dev/null
    wait_for "TEST: adc a0=1.250 a1=1.024 a2=1.000 a3=0.128 v" "set:ain0=1.25 -> 1.250 V" 20
    echo "ads1x15-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
ads1x15-clean:
    rm -rf {{ ads1x15_out }} {{ ads1x15_part_dir }}/target


# ---- AHT20 virtual part fixture (drivers/aht20) ---------------------------

aht20_rel := "drivers/aht20"
aht20_dir := justfile_directory() / aht20_rel
aht20_out := build_dir / "aht20"
aht20_port := "127.0.0.1:5568"
aht20_part_dir := aht20_dir / "parts" / "aht20"
aht20_part_wasm := aht20_dir / "parts" / "aht20.wasm"

# The part's own tests: the status, the counts, the CRC and the inputs, on the
# host.
aht20-part-test:
    cargo test --manifest-path {{ aht20_part_dir }}/Cargo.toml

# Build the virtual AHT20 -> drivers/aht20/parts/aht20.wasm.
aht20-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ aht20_part_dir }}/Cargo.toml"
    cp "{{ aht20_part_dir }}/target/wasm32-unknown-unknown/release/aht20.wasm" \
        "{{ aht20_part_wasm }}"
    ls -l "{{ aht20_part_wasm }}"

# The driver's own tests: the status check and the decode, on the host.
aht20-driver-test:
    cargo test --manifest-path {{ aht20_dir }}/aht20-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
aht20-check:
    just check {{ aht20_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
aht20-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ aht20_out }}"
    just check "{{ aht20_rel }}"
    just build "{{ aht20_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ aht20_out }}/aht20.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/aht20_test" \
        "{{ aht20_out }}/aht20.elf"
    ls -l "{{ aht20_out }}/aht20.bin" "{{ aht20_out }}/aht20.elf"

# Run the firmware with the virtual sensor on the control channel.
aht20-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ aht20_out }}/aht20.bin --cosim-board {{ aht20_dir }}/board.json --control-tcp {{ aht20_port }} --elf {{ aht20_out }}/aht20.elf --timeout {{ timeout }} --log-file {{ aht20_out }}/emu.log"

# One control line, e.g. "part climate input set:temperature=30".
aht20-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ aht20_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/aht20/drivers/aht20.kdl.
aht20-driver-record:
    just espforge "driver {{ aht20_rel }} add --local {{ aht20_rel }}/drivers/aht20.kdl"

# Happy path: boot, the default room, then a warmer and a wetter room.
aht20-sensor: aht20-image aht20-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ aht20_out }}/sensor.stdout"
    log="{{ aht20_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just aht20-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: aht20 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x38 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: climate t=22.50 c rh=45.00 %" "the part reads 22.50 C and 45.00 %RH" 20
    just aht20-part-input "part climate input set:temperature=30" >/dev/null
    wait_for "TEST: climate t=30.00 c rh=45.00 %" "set:temperature=30 -> 30.00 C" 20
    just aht20-part-input "part climate input set:humidity=60" >/dev/null
    wait_for "TEST: climate t=30.00 c rh=60.00 %" "set:humidity=60 -> 60.00 %RH" 20
    echo "aht20-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
aht20-clean:
    rm -rf {{ aht20_out }} {{ aht20_part_dir }}/target


# ---- AiP31068 virtual part fixture (drivers/aip31068) ----------------------

aip31068_rel := "drivers/aip31068"
aip31068_dir := justfile_directory() / aip31068_rel
aip31068_out := build_dir / "aip31068"
aip31068_port := "127.0.0.1:5569"
aip31068_part_dir := aip31068_dir / "parts" / "aip31068"
aip31068_part_wasm := aip31068_dir / "parts" / "aip31068.wasm"

# The part's own tests: the instruction set, DDRAM and the state, on the host.
aip31068-part-test:
    cargo test --manifest-path {{ aip31068_part_dir }}/Cargo.toml

# Build the virtual AiP31068 -> drivers/aip31068/parts/aip31068.wasm.
aip31068-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ aip31068_part_dir }}/Cargo.toml"
    cp "{{ aip31068_part_dir }}/target/wasm32-unknown-unknown/release/aip31068.wasm" \
        "{{ aip31068_part_wasm }}"
    ls -l "{{ aip31068_part_wasm }}"

# The driver's own tests: the initialization and the two rows, on the host.
aip31068-driver-test:
    cargo test --manifest-path {{ aip31068_dir }}/aip31068-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
aip31068-check:
    just check {{ aip31068_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
aip31068-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ aip31068_out }}"
    just check "{{ aip31068_rel }}"
    just build "{{ aip31068_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ aip31068_out }}/aip31068.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/aip31068_test" \
        "{{ aip31068_out }}/aip31068.elf"
    ls -l "{{ aip31068_out }}/aip31068.bin" "{{ aip31068_out }}/aip31068.elf"

# Run the firmware with the virtual panel on the control channel.
aip31068-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ aip31068_out }}/aip31068.bin --cosim-board {{ aip31068_dir }}/board.json --control-tcp {{ aip31068_port }} --elf {{ aip31068_out }}/aip31068.elf --timeout {{ timeout }} --log-file {{ aip31068_out }}/emu.log"

# One control line, e.g. "part lcd state".
aip31068-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ aip31068_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/aip31068/drivers/aip31068.kdl.
aip31068-driver-record:
    just espforge "driver {{ aip31068_rel }} add --local {{ aip31068_rel }}/drivers/aip31068.kdl"

# Happy path: boot, the two lines written, and the panel's DDRAM read back.
aip31068-sensor: aip31068-image aip31068-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ aip31068_out }}/sensor.stdout"
    log="{{ aip31068_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just aip31068-run 90s > "$out" 2>&1 &
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
    wait_for_state() {
        local i state
        for i in $(seq 1 "$3"); do
            state=$(just aip31068-part-input "part lcd state" 2>/dev/null || true)
            case "$state" in
                *"$1"*) echo "PASS: $2"; return 0 ;;
            esac
            sleep 1
        done
        echo "FAIL: $2 (no '$1'); got '$state'" >&2
        return 1
    }
    wait_for "TEST: aip31068 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x3" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: lcd aip31068-lcd / hello-i2c" "the firmware wrote both lines" 20
    wait_for_state "line1=aip31068-lcd line2=hello-i2c" "the panel shows aip31068-lcd and hello-i2c" 20
    echo "aip31068-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
aip31068-clean:
    rm -rf {{ aip31068_out }} {{ aip31068_part_dir }}/target


# ---- AM2320 virtual part fixture (drivers/am2320) --------------------------

am2320_rel := "drivers/am2320"
am2320_dir := justfile_directory() / am2320_rel
am2320_out := build_dir / "am2320"
am2320_port := "127.0.0.1:5570"
am2320_part_dir := am2320_dir / "parts" / "am2320"
am2320_part_wasm := am2320_dir / "parts" / "am2320.wasm"

# The part's own tests: the registers, the CRC-16 and the inputs, on the host.
am2320-part-test:
    cargo test --manifest-path {{ am2320_part_dir }}/Cargo.toml

# Build the virtual AM2320 -> drivers/am2320/parts/am2320.wasm.
am2320-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ am2320_part_dir }}/Cargo.toml"
    cp "{{ am2320_part_dir }}/target/wasm32-unknown-unknown/release/am2320.wasm" \
        "{{ am2320_part_wasm }}"
    ls -l "{{ am2320_part_wasm }}"

# The driver's own tests: the read command, the decode and the sign, on the
# host.
am2320-driver-test:
    cargo test --manifest-path {{ am2320_dir }}/am2320-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
am2320-check:
    just check {{ am2320_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
am2320-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ am2320_out }}"
    just check "{{ am2320_rel }}"
    just build "{{ am2320_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ am2320_out }}/am2320.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/am2320_test" \
        "{{ am2320_out }}/am2320.elf"
    ls -l "{{ am2320_out }}/am2320.bin" "{{ am2320_out }}/am2320.elf"

# Run the firmware with the virtual sensor on the control channel.
am2320-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ am2320_out }}/am2320.bin --cosim-board {{ am2320_dir }}/board.json --control-tcp {{ am2320_port }} --elf {{ am2320_out }}/am2320.elf --timeout {{ timeout }} --log-file {{ am2320_out }}/emu.log"

# One control line, e.g. "part climate input set:temperature=30".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
am2320-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ am2320_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/am2320/drivers/am2320.kdl.
am2320-driver-record:
    just espforge "driver {{ am2320_rel }} add --local {{ am2320_rel }}/drivers/am2320.kdl"

# Happy path: boot, 22.50 C / 45.00 %RH, then a warmer and a wetter part.
am2320-sensor: am2320-image am2320-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ am2320_out }}/sensor.stdout"
    log="{{ am2320_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just am2320-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: am2320 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x5C on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: climate t=22.50 c rh=45.00 %" "the part reads 22.50 C and 45.00 %RH" 20
    just am2320-part-input "part climate input set:temperature=30" >/dev/null
    wait_for "TEST: climate t=30.00 c rh=45.00 %" "set:temperature=30 -> 30.00 C" 20
    just am2320-part-input "part climate input set:humidity=60" >/dev/null
    wait_for "TEST: climate t=30.00 c rh=60.00 %" "set:humidity=60 -> 60.00 %RH" 20
    echo "am2320-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
am2320-clean:
    rm -rf {{ am2320_out }} {{ am2320_part_dir }}/target


# ---- AS7262 virtual part fixture (drivers/as7262) --------------------------

as7262_rel := "drivers/as7262"
as7262_dir := justfile_directory() / as7262_rel
as7262_out := build_dir / "as7262"
as7262_port := "127.0.0.1:5571"
as7262_part_dir := as7262_dir / "parts" / "as7262"
as7262_part_wasm := as7262_dir / "parts" / "as7262.wasm"

# The part's own tests: the virtual registers, the latching and the inputs, on
# the host.
as7262-part-test:
    cargo test --manifest-path {{ as7262_part_dir }}/Cargo.toml

# Build the virtual AS7262 -> drivers/as7262/parts/as7262.wasm.
as7262-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ as7262_part_dir }}/Cargo.toml"
    cp "{{ as7262_part_dir }}/target/wasm32-unknown-unknown/release/as7262.wasm" \
        "{{ as7262_part_wasm }}"
    ls -l "{{ as7262_part_wasm }}"

# The driver's own tests: the reset, the reads and the decode, on the host.
as7262-driver-test:
    cargo test --manifest-path {{ as7262_dir }}/as7262-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
as7262-check:
    just check {{ as7262_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
as7262-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ as7262_out }}"
    just check "{{ as7262_rel }}"
    just build "{{ as7262_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ as7262_out }}/as7262.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/as7262_test" \
        "{{ as7262_out }}/as7262.elf"
    ls -l "{{ as7262_out }}/as7262.bin" "{{ as7262_out }}/as7262.elf"

# Run the firmware with the virtual sensor on the control channel.
as7262-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ as7262_out }}/as7262.bin --cosim-board {{ as7262_dir }}/board.json --control-tcp {{ as7262_port }} --elf {{ as7262_out }}/as7262.elf --timeout {{ timeout }} --log-file {{ as7262_out }}/emu.log"

# One control line, e.g. "part spectrum input set:red=999".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
as7262-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ as7262_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/as7262/drivers/as7262.kdl.
as7262-driver-record:
    just espforge "driver {{ as7262_rel }} add --local {{ as7262_rel }}/drivers/as7262.kdl"

# Happy path: boot, the default spectrum, a red step and a dark part.
as7262-sensor: as7262-image as7262-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ as7262_out }}/sensor.stdout"
    log="{{ as7262_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just as7262-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: as7262 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x49 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: as7262 v=1200 b=2400 g=3200 y=3100 o=2800 r=2200" "the default spectrum (1200 2400 3200 3100 2800 2200)" 20
    just as7262-part-input "part spectrum input set:red=999" >/dev/null
    wait_for "TEST: as7262 v=1200 b=2400 g=3200 y=3100 o=2800 r=999" "set:red=999 -> the red channel moves" 20
    just as7262-part-input "part spectrum input dark" >/dev/null
    wait_for "TEST: as7262 v=0 b=0 g=0 y=0 o=0 r=0" "dark -> all six channels read 0" 20
    echo "as7262-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
as7262-clean:
    rm -rf {{ as7262_out }} {{ as7262_part_dir }}/target


# ---- BH1750 virtual part fixture (drivers/bh1750) --------------------------

bh1750_rel := "drivers/bh1750"
bh1750_dir := justfile_directory() / bh1750_rel
bh1750_out := build_dir / "bh1750"
bh1750_port := "127.0.0.1:5572"
bh1750_part_dir := bh1750_dir / "parts" / "bh1750"
bh1750_part_wasm := bh1750_dir / "parts" / "bh1750.wasm"

# The part's own tests: the commands, the modes and the inputs, on the host.
bh1750-part-test:
    cargo test --manifest-path {{ bh1750_part_dir }}/Cargo.toml

# Build the virtual BH1750 -> drivers/bh1750/parts/bh1750.wasm.
bh1750-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ bh1750_part_dir }}/Cargo.toml"
    cp "{{ bh1750_part_dir }}/target/wasm32-unknown-unknown/release/bh1750.wasm" \
        "{{ bh1750_part_wasm }}"
    ls -l "{{ bh1750_part_wasm }}"

# The driver's own tests: the power-on, the read and the lux conversion, on
# the host.
bh1750-driver-test:
    cargo test --manifest-path {{ bh1750_dir }}/bh1750-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
bh1750-check:
    just check {{ bh1750_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
bh1750-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ bh1750_out }}"
    just check "{{ bh1750_rel }}"
    just build "{{ bh1750_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ bh1750_out }}/bh1750.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/bh1750_test" \
        "{{ bh1750_out }}/bh1750.elf"
    ls -l "{{ bh1750_out }}/bh1750.bin" "{{ bh1750_out }}/bh1750.elf"

# Run the firmware with the virtual sensor on the control channel.
bh1750-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ bh1750_out }}/bh1750.bin --cosim-board {{ bh1750_dir }}/board.json --control-tcp {{ bh1750_port }} --elf {{ bh1750_out }}/bh1750.elf --timeout {{ timeout }} --log-file {{ bh1750_out }}/emu.log"

# One control line, e.g. "part light input set:lux=1000".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
bh1750-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ bh1750_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/bh1750/drivers/bh1750.kdl.
bh1750-driver-record:
    just espforge "driver {{ bh1750_rel }} add --local {{ bh1750_rel }}/drivers/bh1750.kdl"

# Happy path: boot, the default light, a brighter room and a dark one.
bh1750-sensor: bh1750-image bh1750-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ bh1750_out }}/sensor.stdout"
    log="{{ bh1750_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just bh1750-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: bh1750 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x23 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: bh1750 lux=300.0" "the default light (300.0 lux)" 20
    just bh1750-part-input "part light input set:lux=1000" >/dev/null
    wait_for "TEST: bh1750 lux=1000.0" "set:lux=1000 -> the reading moves" 20
    just bh1750-part-input "part light input dark" >/dev/null
    wait_for "TEST: bh1750 lux=0.0" "dark -> 0.0 lux" 20
    echo "bh1750-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
bh1750-clean:
    rm -rf {{ bh1750_out }} {{ bh1750_part_dir }}/target


# ---- BMxx80 virtual part fixture (drivers/bmxx80) ---------------------------

bmxx80_rel := "drivers/bmxx80"
bmxx80_dir := justfile_directory() / bmxx80_rel
bmxx80_out := build_dir / "bmxx80"
bmxx80_port := "127.0.0.1:5573"
bmxx80_part_dir := bmxx80_dir / "parts" / "bmxx80"
bmxx80_part_wasm := bmxx80_dir / "parts" / "bmxx80.wasm"

# The part's own tests: the registers, the calibration and the inputs, on the
# host.
bmxx80-part-test:
    cargo test --manifest-path {{ bmxx80_part_dir }}/Cargo.toml

# Build the virtual BME280 -> drivers/bmxx80/parts/bmxx80.wasm.
bmxx80-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ bmxx80_part_dir }}/Cargo.toml"
    cp "{{ bmxx80_part_dir }}/target/wasm32-unknown-unknown/release/bmxx80.wasm" \
        "{{ bmxx80_part_wasm }}"
    ls -l "{{ bmxx80_part_wasm }}"

# The driver's own tests: the chip check, the configuration and the
# compensation, on the host.
bmxx80-driver-test:
    cargo test --manifest-path {{ bmxx80_dir }}/bmxx80-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
bmxx80-check:
    just check {{ bmxx80_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
bmxx80-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ bmxx80_out }}"
    just check "{{ bmxx80_rel }}"
    just build "{{ bmxx80_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ bmxx80_out }}/bmxx80.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/bmxx80_test" \
        "{{ bmxx80_out }}/bmxx80.elf"
    ls -l "{{ bmxx80_out }}/bmxx80.bin" "{{ bmxx80_out }}/bmxx80.elf"

# Run the firmware with the virtual sensor on the control channel.
bmxx80-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ bmxx80_out }}/bmxx80.bin --cosim-board {{ bmxx80_dir }}/board.json --control-tcp {{ bmxx80_port }} --elf {{ bmxx80_out }}/bmxx80.elf --timeout {{ timeout }} --log-file {{ bmxx80_out }}/emu.log"

# One control line, e.g. "part weather input set:temperature=30".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
bmxx80-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ bmxx80_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/bmxx80/drivers/bmxx80.kdl.
bmxx80-driver-record:
    just espforge "driver {{ bmxx80_rel }} add --local {{ bmxx80_rel }}/drivers/bmxx80.kdl"

# Happy path: boot, the default weather, a warmer room and a more humid one.
bmxx80-sensor: bmxx80-image bmxx80-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ bmxx80_out }}/sensor.stdout"
    log="{{ bmxx80_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just bmxx80-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: bmxx80 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x76 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: bmxx80 t=25.00 p=1013.25 h=45.00" "the default weather (25.00 C, 1013.25 hPa, 45.00 %RH)" 20
    just bmxx80-part-input "part weather input set:temperature=30" >/dev/null
    wait_for "TEST: bmxx80 t=30.00 p=1013.25 h=45.00" "set:temperature=30 -> the reading moves" 20
    just bmxx80-part-input "part weather input set:humidity=60" >/dev/null
    wait_for "TEST: bmxx80 t=30.00 p=1013.25 h=60.00" "set:humidity=60 -> the reading moves again" 20
    echo "bmxx80-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
bmxx80-clean:
    rm -rf {{ bmxx80_out }} {{ bmxx80_part_dir }}/target


# ---- DS3231 virtual part fixture (drivers/ds3231) --------------------------

ds3231_rel := "drivers/ds3231"
ds3231_dir := justfile_directory() / ds3231_rel
ds3231_out := build_dir / "ds3231"
ds3231_port := "127.0.0.1:5574"
ds3231_part_dir := ds3231_dir / "parts" / "ds3231"
ds3231_part_wasm := ds3231_dir / "parts" / "ds3231.wasm"

# The part's own tests: the registers, the BCD and the inputs, on the host.
ds3231-part-test:
    cargo test --manifest-path {{ ds3231_part_dir }}/Cargo.toml

# Build the virtual DS3231 -> drivers/ds3231/parts/ds3231.wasm.
ds3231-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ ds3231_part_dir }}/Cargo.toml"
    cp "{{ ds3231_part_dir }}/target/wasm32-unknown-unknown/release/ds3231.wasm" \
        "{{ ds3231_part_wasm }}"
    ls -l "{{ ds3231_part_wasm }}"

# The driver's own tests: the flag, the decode and the temperature, on the
# host.
ds3231-driver-test:
    cargo test --manifest-path {{ ds3231_dir }}/ds3231-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
ds3231-check:
    just check {{ ds3231_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
ds3231-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ds3231_out }}"
    just check "{{ ds3231_rel }}"
    just build "{{ ds3231_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ ds3231_out }}/ds3231.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/ds3231_test" \
        "{{ ds3231_out }}/ds3231.elf"
    ls -l "{{ ds3231_out }}/ds3231.bin" "{{ ds3231_out }}/ds3231.elf"

# Run the firmware with the virtual clock on the control channel.
ds3231-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ ds3231_out }}/ds3231.bin --cosim-board {{ ds3231_dir }}/board.json --control-tcp {{ ds3231_port }} --elf {{ ds3231_out }}/ds3231.elf --timeout {{ timeout }} --log-file {{ ds3231_out }}/emu.log"

# One control line, e.g. "part rtc input set:hour=8".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
ds3231-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ ds3231_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/ds3231/drivers/ds3231.kdl.
ds3231-driver-record:
    just espforge "driver {{ ds3231_rel }} add --local {{ ds3231_rel }}/drivers/ds3231.kdl"

# Happy path: boot, the power-on time, the die temperature, then a set clock.
ds3231-sensor: ds3231-image ds3231-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ ds3231_out }}/sensor.stdout"
    log="{{ ds3231_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just ds3231-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: ds3231 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x68 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: ds3231 2026-01-01 00:00:00 w=1" "the power-on time (2026-01-01 00:00:00, day 1)" 20
    wait_for "TEST: ds3231 t=25.00 c" "the die temperature (25.00 C)" 20
    just ds3231-part-input "part rtc input set:year=2030" >/dev/null
    just ds3231-part-input "part rtc input set:month=6" >/dev/null
    just ds3231-part-input "part rtc input set:day=15" >/dev/null
    just ds3231-part-input "part rtc input set:weekday=3" >/dev/null
    just ds3231-part-input "part rtc input set:hour=8" >/dev/null
    just ds3231-part-input "part rtc input set:minute=30" >/dev/null
    wait_for "TEST: ds3231 2030-06-15 08:30:00 w=3" "set:* -> 2030-06-15 08:30:00 on day 3" 20
    just ds3231-part-input "part rtc input set:temperature=21.5" >/dev/null
    wait_for "TEST: ds3231 t=21.50 c" "set:temperature=21.5 -> 21.50 C" 20
    echo "ds3231-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
ds3231-clean:
    rm -rf {{ ds3231_out }} {{ ds3231_part_dir }}/target


# ---- CCS811 virtual part fixture (drivers/ccs811) --------------------------

ccs811_rel := "drivers/ccs811"
ccs811_dir := justfile_directory() / ccs811_rel
ccs811_out := build_dir / "ccs811"
ccs811_port := "127.0.0.1:5575"
ccs811_part_dir := ccs811_dir / "parts" / "ccs811"
ccs811_part_wasm := ccs811_dir / "parts" / "ccs811.wasm"

# The part's own tests: the status bits, the result bytes and the inputs, on
# the host.
ccs811-part-test:
    cargo test --manifest-path {{ ccs811_part_dir }}/Cargo.toml

# Build the virtual CCS811 -> drivers/ccs811/parts/ccs811.wasm.
ccs811-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ ccs811_part_dir }}/Cargo.toml"
    cp "{{ ccs811_part_dir }}/target/wasm32-unknown-unknown/release/ccs811.wasm" \
        "{{ ccs811_part_wasm }}"
    ls -l "{{ ccs811_part_wasm }}"

# The driver's own tests: the id check, boot mode and the decode, on the host.
ccs811-driver-test:
    cargo test --manifest-path {{ ccs811_dir }}/ccs811-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
ccs811-check:
    just check {{ ccs811_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
ccs811-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ccs811_out }}"
    just check "{{ ccs811_rel }}"
    just build "{{ ccs811_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ ccs811_out }}/ccs811.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/ccs811_test" \
        "{{ ccs811_out }}/ccs811.elf"
    ls -l "{{ ccs811_out }}/ccs811.bin" "{{ ccs811_out }}/ccs811.elf"

# Run the firmware with the virtual sensor on the control channel.
ccs811-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ ccs811_out }}/ccs811.bin --cosim-board {{ ccs811_dir }}/board.json --control-tcp {{ ccs811_port }} --elf {{ ccs811_out }}/ccs811.elf --timeout {{ timeout }} --log-file {{ ccs811_out }}/emu.log"

# One control line, e.g. "part air input set:eco2=800".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
ccs811-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ ccs811_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/ccs811/drivers/ccs811.kdl.
ccs811-driver-record:
    just espforge "driver {{ ccs811_rel }} add --local {{ ccs811_rel }}/drivers/ccs811.kdl"

# Happy path: boot, fresh air, a dirtier room and a dustier one.
ccs811-sensor: ccs811-image ccs811-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ ccs811_out }}/sensor.stdout"
    log="{{ ccs811_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just ccs811-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: ccs811 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x5A on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: ccs811 eco2=400 tvoc=8" "fresh air (400 ppm eCO2, 8 ppb TVOC)" 20
    just ccs811-part-input "part air input set:eco2=800" >/dev/null
    wait_for "TEST: ccs811 eco2=800 tvoc=8" "set:eco2=800 -> 800 ppm" 20
    just ccs811-part-input "part air input set:tvoc=50" >/dev/null
    wait_for "TEST: ccs811 eco2=800 tvoc=50" "set:tvoc=50 -> 50 ppb" 20
    echo "ccs811-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
ccs811-clean:
    rm -rf {{ ccs811_out }} {{ ccs811_part_dir }}/target


# ---- HDC302x virtual part fixture (drivers/hdc302x) ------------------------

hdc302x_rel := "drivers/hdc302x"
hdc302x_dir := justfile_directory() / hdc302x_rel
hdc302x_out := build_dir / "hdc302x"
hdc302x_port := "127.0.0.1:5576"
hdc302x_part_dir := hdc302x_dir / "parts" / "hdc302x"
hdc302x_part_wasm := hdc302x_dir / "parts" / "hdc302x.wasm"

# The part's own tests: the commands, the conversion and the inputs, on the
# host.
hdc302x-part-test:
    cargo test --manifest-path {{ hdc302x_part_dir }}/Cargo.toml

# Build the virtual HDC302x -> drivers/hdc302x/parts/hdc302x.wasm.
hdc302x-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ hdc302x_part_dir }}/Cargo.toml"
    cp "{{ hdc302x_part_dir }}/target/wasm32-unknown-unknown/release/hdc302x.wasm" \
        "{{ hdc302x_part_wasm }}"
    ls -l "{{ hdc302x_part_wasm }}"

# The driver's own tests: the reset, the trigger and the decode, on the host.
hdc302x-driver-test:
    cargo test --manifest-path {{ hdc302x_dir }}/hdc302x-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
hdc302x-check:
    just check {{ hdc302x_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
hdc302x-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ hdc302x_out }}"
    just check "{{ hdc302x_rel }}"
    just build "{{ hdc302x_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ hdc302x_out }}/hdc302x.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/hdc302x_test" \
        "{{ hdc302x_out }}/hdc302x.elf"
    ls -l "{{ hdc302x_out }}/hdc302x.bin" "{{ hdc302x_out }}/hdc302x.elf"

# Run the firmware with the virtual sensor on the control channel.
hdc302x-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ hdc302x_out }}/hdc302x.bin --cosim-board {{ hdc302x_dir }}/board.json --control-tcp {{ hdc302x_port }} --elf {{ hdc302x_out }}/hdc302x.elf --timeout {{ timeout }} --log-file {{ hdc302x_out }}/emu.log"

# One control line, e.g. "part climate input set:temperature=30".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
hdc302x-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ hdc302x_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/hdc302x/drivers/hdc302x.kdl.
hdc302x-driver-record:
    just espforge "driver {{ hdc302x_rel }} add --local {{ hdc302x_rel }}/drivers/hdc302x.kdl"

# Happy path: boot, the default room, a warmer one and a wetter one.
hdc302x-sensor: hdc302x-image hdc302x-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ hdc302x_out }}/sensor.stdout"
    log="{{ hdc302x_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just hdc302x-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: hdc302x boot" "firmware boots" 40
    wait_for_log "answers I2C 0x44 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: hdc302x t=22.50 rh=45.00" "the default room (22.50 C, 45.00 %RH)" 20
    just hdc302x-part-input "part climate input set:temperature=30" >/dev/null
    wait_for "TEST: hdc302x t=30.00 rh=45.00" "set:temperature=30 -> 30.00 C" 20
    just hdc302x-part-input "part climate input set:humidity=60" >/dev/null
    wait_for "TEST: hdc302x t=30.00 rh=60.00" "set:humidity=60 -> 60.00 %RH" 20
    echo "hdc302x-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
hdc302x-clean:
    rm -rf {{ hdc302x_out }} {{ hdc302x_part_dir }}/target


# ---- INA219 virtual part fixture (drivers/ina219) ------------------------

ina219_rel := "drivers/ina219"
ina219_dir := justfile_directory() / ina219_rel
ina219_out := build_dir / "ina219"
ina219_port := "127.0.0.1:5577"
ina219_part_dir := ina219_dir / "parts" / "ina219"
ina219_part_wasm := ina219_dir / "parts" / "ina219.wasm"

# The part's own tests: the register file, the scale factors and the inputs,
# on the host.
ina219-part-test:
    cargo test --manifest-path {{ ina219_part_dir }}/Cargo.toml

# Build the virtual INA219 -> drivers/ina219/parts/ina219.wasm.
ina219-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ ina219_part_dir }}/Cargo.toml"
    cp "{{ ina219_part_dir }}/target/wasm32-unknown-unknown/release/ina219.wasm" \
        "{{ ina219_part_wasm }}"
    ls -l "{{ ina219_part_wasm }}"

# The driver's own tests: the configuration, the calibration and the decode,
# on the host.
ina219-driver-test:
    cargo test --manifest-path {{ ina219_dir }}/ina219-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
ina219-check:
    just check {{ ina219_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
ina219-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ina219_out }}"
    just check "{{ ina219_rel }}"
    just build "{{ ina219_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ ina219_out }}/ina219.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/ina219_test" \
        "{{ ina219_out }}/ina219.elf"
    ls -l "{{ ina219_out }}/ina219.bin" "{{ ina219_out }}/ina219.elf"

# Run the firmware with the virtual monitor on the control channel.
ina219-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ ina219_out }}/ina219.bin --cosim-board {{ ina219_dir }}/board.json --control-tcp {{ ina219_port }} --elf {{ ina219_out }}/ina219.elf --timeout {{ timeout }} --log-file {{ ina219_out }}/emu.log"

# One control line, e.g. "part power input set:current=1".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
ina219-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ ina219_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/ina219/drivers/ina219.kdl.
ina219-driver-record:
    just espforge "driver {{ ina219_rel }} add --local {{ ina219_rel }}/drivers/ina219.kdl"

# Happy path: boot, the default load, a 12 V bus and a heavier load.
ina219-sensor: ina219-image ina219-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ ina219_out }}/sensor.stdout"
    log="{{ ina219_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just ina219-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: ina219 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x40 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: ina219 bus=5.00 v shunt=10.00 mv current=100.00 ma power=500.00 mw" "the default load (5.00 V, 100.00 mA)" 20
    just ina219-part-input "part power input set:voltage=12" >/dev/null
    wait_for "TEST: ina219 bus=12.00 v shunt=10.00 mv current=100.00 ma power=1200.00 mw" "set:voltage=12 -> 12.00 V" 20
    just ina219-part-input "part power input set:current=1" >/dev/null
    wait_for "TEST: ina219 bus=12.00 v shunt=100.00 mv current=1000.00 ma power=12000.00 mw" "set:current=1 -> 1000.00 mA" 20
    echo "ina219-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
ina219-clean:
    rm -rf {{ ina219_out }} {{ ina219_part_dir }}/target


# ---- MCP9808 virtual part fixture (drivers/mcp9808) ----------------------

mcp9808_rel := "drivers/mcp9808"
mcp9808_dir := justfile_directory() / mcp9808_rel
mcp9808_out := build_dir / "mcp9808"
mcp9808_port := "127.0.0.1:5578"
mcp9808_part_dir := mcp9808_dir / "parts" / "mcp9808"
mcp9808_part_wasm := mcp9808_dir / "parts" / "mcp9808.wasm"

# The part's own tests: the register file, the encoding and the inputs, on the
# host.
mcp9808-part-test:
    cargo test --manifest-path {{ mcp9808_part_dir }}/Cargo.toml

# Build the virtual MCP9808 -> drivers/mcp9808/parts/mcp9808.wasm.
mcp9808-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ mcp9808_part_dir }}/Cargo.toml"
    cp "{{ mcp9808_part_dir }}/target/wasm32-unknown-unknown/release/mcp9808.wasm" \
        "{{ mcp9808_part_wasm }}"
    ls -l "{{ mcp9808_part_wasm }}"

# The driver's own tests: the identification, the resolution and the decode,
# on the host.
mcp9808-driver-test:
    cargo test --manifest-path {{ mcp9808_dir }}/mcp9808-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
mcp9808-check:
    just check {{ mcp9808_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
mcp9808-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ mcp9808_out }}"
    just check "{{ mcp9808_rel }}"
    just build "{{ mcp9808_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ mcp9808_out }}/mcp9808.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/mcp9808_test" \
        "{{ mcp9808_out }}/mcp9808.elf"
    ls -l "{{ mcp9808_out }}/mcp9808.bin" "{{ mcp9808_out }}/mcp9808.elf"

# Run the firmware with the virtual monitor on the control channel.
mcp9808-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ mcp9808_out }}/mcp9808.bin --cosim-board {{ mcp9808_dir }}/board.json --control-tcp {{ mcp9808_port }} --elf {{ mcp9808_out }}/mcp9808.elf --timeout {{ timeout }} --log-file {{ mcp9808_out }}/emu.log"

# One control line, e.g. "part temperature input set:temperature=30".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
mcp9808-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ mcp9808_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/mcp9808/drivers/mcp9808.kdl.
mcp9808-driver-record:
    just espforge "driver {{ mcp9808_rel }} add --local {{ mcp9808_rel }}/drivers/mcp9808.kdl"

# Happy path: boot, room temperature, then a warm and a cold reading.
mcp9808-sensor: mcp9808-image mcp9808-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ mcp9808_out }}/sensor.stdout"
    log="{{ mcp9808_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just mcp9808-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: mcp9808 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x18 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: mcp9808 temp=24.00 c" "the default 24.00 °C" 20
    just mcp9808-part-input "part temperature input set:temperature=30" >/dev/null
    wait_for "TEST: mcp9808 temp=30.00 c" "set:temperature=30 -> 30.00 °C" 20
    just mcp9808-part-input "part temperature input set:temperature=-5.5" >/dev/null
    wait_for "TEST: mcp9808 temp=-5.50 c" "set:temperature=-5.5 -> -5.50 °C" 20
    echo "mcp9808-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
mcp9808-clean:
    rm -rf {{ mcp9808_out }} {{ mcp9808_part_dir }}/target


# ---- SCD4x virtual part fixture (drivers/scd4x) --------------------------

scd4x_rel := "drivers/scd4x"
scd4x_dir := justfile_directory() / scd4x_rel
scd4x_out := build_dir / "scd4x"
scd4x_port := "127.0.0.1:5581"
scd4x_part_dir := scd4x_dir / "parts" / "scd4x"
scd4x_part_wasm := scd4x_dir / "parts" / "scd4x.wasm"

# The part's own tests: the encoding, the CRC and the inputs, on the host.
scd4x-part-test:
    cargo test --manifest-path {{ scd4x_part_dir }}/Cargo.toml

# Build the virtual SCD4x -> drivers/scd4x/parts/scd4x.wasm.
scd4x-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ scd4x_part_dir }}/Cargo.toml"
    cp "{{ scd4x_part_dir }}/target/wasm32-unknown-unknown/release/scd4x.wasm" \
        "{{ scd4x_part_wasm }}"
    ls -l "{{ scd4x_part_wasm }}"

# Schema and resource graph for the bundled `<scd4x>` element.
scd4x-check:
    just check {{ scd4x_rel }}

# Compile the generated project with the bundled driver.
scd4x-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ scd4x_out }}"
    just check "{{ scd4x_rel }}"
    just build "{{ scd4x_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ scd4x_out }}/scd4x.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/scd4x_test" \
        "{{ scd4x_out }}/scd4x.elf"
    ls -l "{{ scd4x_out }}/scd4x.bin" "{{ scd4x_out }}/scd4x.elf"

# Run the firmware with the virtual sensor on the control channel.
scd4x-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ scd4x_out }}/scd4x.bin --cosim-board {{ scd4x_dir }}/board.json --control-tcp {{ scd4x_port }} --elf {{ scd4x_out }}/scd4x.elf --timeout {{ timeout }} --log-file {{ scd4x_out }}/emu.log"

# One control line, e.g. "part co2 input set:co2=1000".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
scd4x-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ scd4x_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Happy path: boot, fresh air, more CO2, then a warmer and wetter room.
scd4x-sensor: scd4x-image scd4x-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ scd4x_out }}/sensor.stdout"
    log="{{ scd4x_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just scd4x-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: scd4x boot" "firmware boots" 40
    wait_for "TEST: scd4x started" "the bundled driver starts periodic measurement" 10
    wait_for_log "answers I2C 0x62 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: scd4x co2=400 ppm t=22.49 c rh=44.99" "the default 400 ppm at 22.49 C / 44.99 %RH" 20
    just scd4x-part-input "part co2 input set:co2=1000" >/dev/null
    wait_for "TEST: scd4x co2=1000 ppm t=22.49 c rh=44.99" "set:co2=1000 -> 1000 ppm" 20
    just scd4x-part-input "part co2 input set:temperature=30" >/dev/null
    just scd4x-part-input "part co2 input set:humidity=60" >/dev/null
    wait_for "TEST: scd4x co2=1000 ppm t=30.00 c rh=60.00" "set temperature and humidity -> 30.00 C / 60.00 %RH" 20
    echo "scd4x-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
scd4x-clean:
    rm -rf {{ scd4x_out }} {{ scd4x_part_dir }}/target


# ---- SEN6x virtual part fixture (drivers/sen6x) --------------------------

sen6x_rel := "drivers/sen6x"
sen6x_dir := justfile_directory() / sen6x_rel
sen6x_out := build_dir / "sen6x"
sen6x_port := "127.0.0.1:5582"
sen6x_part_dir := sen6x_dir / "parts" / "sen6x"
sen6x_part_wasm := sen6x_dir / "parts" / "sen6x.wasm"

# The part's own tests: the encoding, the CRC and the inputs, on the host.
sen6x-part-test:
    cargo test --manifest-path {{ sen6x_part_dir }}/Cargo.toml

# Build the virtual SEN6x -> drivers/sen6x/parts/sen6x.wasm.
sen6x-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ sen6x_part_dir }}/Cargo.toml"
    cp "{{ sen6x_part_dir }}/target/wasm32-unknown-unknown/release/sen6x.wasm" \
        "{{ sen6x_part_wasm }}"
    ls -l "{{ sen6x_part_wasm }}"

# The driver's own tests: the commands, the CRC check and the decode, on the
# host.
sen6x-driver-test:
    cargo test --manifest-path {{ sen6x_dir }}/sen6x-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
sen6x-check:
    just check {{ sen6x_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
sen6x-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ sen6x_out }}"
    just check "{{ sen6x_rel }}"
    just build "{{ sen6x_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ sen6x_out }}/sen6x.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/sen6x_test" \
        "{{ sen6x_out }}/sen6x.elf"
    ls -l "{{ sen6x_out }}/sen6x.bin" "{{ sen6x_out }}/sen6x.elf"

# Run the firmware with the virtual sensor on the control channel.
sen6x-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ sen6x_out }}/sen6x.bin --cosim-board {{ sen6x_dir }}/board.json --control-tcp {{ sen6x_port }} --elf {{ sen6x_out }}/sen6x.elf --timeout {{ timeout }} --log-file {{ sen6x_out }}/emu.log"

# One control line, e.g. "part air input set:pm2-5=50".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
sen6x-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ sen6x_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/sen6x/drivers/sen6x.kdl.
sen6x-driver-record:
    just espforge "driver {{ sen6x_rel }} add --local {{ sen6x_rel }}/drivers/sen6x.kdl"

# Happy path: boot, indoor air, more particles and CO2, then a warmer room.
sen6x-sensor: sen6x-image sen6x-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ sen6x_out }}/sensor.stdout"
    log="{{ sen6x_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just sen6x-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: sen6x boot" "firmware boots" 40
    wait_for_log "answers I2C 0x6B on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: sen6x pm1=8.0 pm2.5=12.5 pm4=15.0 pm10=20.0 ug/m3" "the default indoor air (8.0 / 12.5 / 15.0 / 20.0 ug/m3)" 20
    wait_for "t=22.50 c rh=45.00 voc=100.0 nox=1.0 co2=400 ppm" "the default 22.50 C / 45.00 %RH / 400 ppm" 20
    just sen6x-part-input "part air input set:pm2-5=50" >/dev/null
    wait_for "TEST: sen6x pm1=8.0 pm2.5=50.0" "set:pm2-5=50 -> 50.0 ug/m3" 20
    just sen6x-part-input "part air input set:co2=1000" >/dev/null
    wait_for "co2=1000 ppm" "set:co2=1000 -> 1000 ppm" 20
    just sen6x-part-input "part air input set:temperature=30" >/dev/null
    just sen6x-part-input "part air input set:humidity=60" >/dev/null
    wait_for "t=30.00 c rh=60.00" "set temperature and humidity -> 30.00 C / 60.00 %RH" 20
    echo "sen6x-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
sen6x-clean:
    rm -rf {{ sen6x_out }} {{ sen6x_part_dir }}/target


# ---- SGP30 virtual part fixture (drivers/sgp30) --------------------------

sgp30_rel := "drivers/sgp30"
sgp30_dir := justfile_directory() / sgp30_rel
sgp30_out := build_dir / "sgp30"
sgp30_port := "127.0.0.1:5583"
sgp30_part_dir := sgp30_dir / "parts" / "sgp30"
sgp30_part_wasm := sgp30_dir / "parts" / "sgp30.wasm"

# The part's own tests: the encoding, the CRC and the inputs, on the host.
sgp30-part-test:
    cargo test --manifest-path {{ sgp30_part_dir }}/Cargo.toml

# Build the virtual SGP30 -> drivers/sgp30/parts/sgp30.wasm.
sgp30-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ sgp30_part_dir }}/Cargo.toml"
    cp "{{ sgp30_part_dir }}/target/wasm32-unknown-unknown/release/sgp30.wasm" \
        "{{ sgp30_part_wasm }}"
    ls -l "{{ sgp30_part_wasm }}"

# The driver's own tests: the commands, the CRC check and the decode, on the
# host.
sgp30-driver-test:
    cargo test --manifest-path {{ sgp30_dir }}/sgp30-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
sgp30-check:
    just check {{ sgp30_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
sgp30-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ sgp30_out }}"
    just check "{{ sgp30_rel }}"
    just build "{{ sgp30_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ sgp30_out }}/sgp30.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/sgp30_test" \
        "{{ sgp30_out }}/sgp30.elf"
    ls -l "{{ sgp30_out }}/sgp30.bin" "{{ sgp30_out }}/sgp30.elf"

# Run the firmware with the virtual sensor on the control channel.
sgp30-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ sgp30_out }}/sgp30.bin --cosim-board {{ sgp30_dir }}/board.json --control-tcp {{ sgp30_port }} --elf {{ sgp30_out }}/sgp30.elf --timeout {{ timeout }} --log-file {{ sgp30_out }}/emu.log"

# One control line, e.g. "part air input set:eco2=1000".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
sgp30-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ sgp30_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/sgp30/drivers/sgp30.kdl.
sgp30-driver-record:
    just espforge "driver {{ sgp30_rel }} add --local {{ sgp30_rel }}/drivers/sgp30.kdl"

# Happy path: boot, fresh air, more CO2, then a higher TVOC reading.
sgp30-sensor: sgp30-image sgp30-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ sgp30_out }}/sensor.stdout"
    log="{{ sgp30_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just sgp30-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: sgp30 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x58 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: sgp30 eco2=400 ppm tvoc=0 ppb" "the default 400 ppm / 0 ppb" 20
    just sgp30-part-input "part air input set:eco2=1000" >/dev/null
    wait_for "TEST: sgp30 eco2=1000 ppm tvoc=0 ppb" "set:eco2=1000 -> 1000 ppm" 20
    just sgp30-part-input "part air input set:tvoc=137" >/dev/null
    wait_for "TEST: sgp30 eco2=1000 ppm tvoc=137 ppb" "set:tvoc=137 -> 137 ppb" 20
    echo "sgp30-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
sgp30-clean:
    rm -rf {{ sgp30_out }} {{ sgp30_part_dir }}/target


# ---- PCA9685 virtual part fixture (drivers/pca9685) ----------------------

pca9685_rel := "drivers/pca9685"
pca9685_dir := justfile_directory() / pca9685_rel
pca9685_out := build_dir / "pca9685"
pca9685_port := "127.0.0.1:5585"
pca9685_part_dir := pca9685_dir / "parts" / "pca9685"
pca9685_part_wasm := pca9685_dir / "parts" / "pca9685.wasm"

# The part's own tests: the register file, the writes and the state, on the
# host.
pca9685-part-test:
    cargo test --manifest-path {{ pca9685_part_dir }}/Cargo.toml

# Build the virtual PCA9685 -> drivers/pca9685/parts/pca9685.wasm.
pca9685-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ pca9685_part_dir }}/Cargo.toml"
    cp "{{ pca9685_part_dir }}/target/wasm32-unknown-unknown/release/pca9685.wasm" \
        "{{ pca9685_part_wasm }}"
    ls -l "{{ pca9685_part_wasm }}"

# The driver's own tests: the init sequence, the prescaler and the duty
# writes, on the host.
pca9685-driver-test:
    cargo test --manifest-path {{ pca9685_dir }}/pca9685-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
pca9685-check:
    just check {{ pca9685_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
pca9685-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ pca9685_out }}"
    just check "{{ pca9685_rel }}"
    just build "{{ pca9685_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ pca9685_out }}/pca9685.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/pca9685_test" \
        "{{ pca9685_out }}/pca9685.elf"
    ls -l "{{ pca9685_out }}/pca9685.bin" "{{ pca9685_out }}/pca9685.elf"

# Run the firmware with the virtual part on the control channel.
pca9685-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ pca9685_out }}/pca9685.bin --cosim-board {{ pca9685_dir }}/board.json --control-tcp {{ pca9685_port }} --elf {{ pca9685_out }}/pca9685.elf --timeout {{ timeout }} --log-file {{ pca9685_out }}/emu.log"

# One control line, e.g. "part pwm state".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
pca9685-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ pca9685_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/pca9685/drivers/pca9685.kdl.
pca9685-driver-record:
    just espforge "driver {{ pca9685_rel }} add --local {{ pca9685_rel }}/drivers/pca9685.kdl"

# Happy path: boot, two duties on the bus, and the part's own view of them.
pca9685-sensor: pca9685-image pca9685-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ pca9685_out }}/sensor.stdout"
    log="{{ pca9685_out }}/emu.log"
    state="{{ pca9685_out }}/state.txt"
    : > "$out"
    : > "$log"
    setsid just pca9685-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: pca9685 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x40 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: pca9685 ch0=25 ch1=50" "ch0 at 25 % and ch1 at 50 %" 20
    just pca9685-part-input "part pwm state" > "$state" 2>&1 || true
    grep -Eq 'prescale[=: ]+121' "$state" \
        && echo "PASS: 50 Hz prescale" \
        || { echo "FAIL: no 50 Hz prescale; see $state" >&2; cat "$state" >&2; exit 1; }
    grep -Eq 'ch0[=: ]+25' "$state" \
        && echo "PASS: ch0 reads back at 25 %" \
        || { echo "FAIL: ch0 is not 25 %; see $state" >&2; cat "$state" >&2; exit 1; }
    echo "pca9685-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
pca9685-clean:
    rm -rf {{ pca9685_out }} {{ pca9685_part_dir }}/target


# ---- TCA95xx virtual part fixture (drivers/tca95xx) ----------------------

tca95xx_rel := "drivers/tca95xx"
tca95xx_dir := justfile_directory() / tca95xx_rel
tca95xx_out := build_dir / "tca95xx"
tca95xx_port := "127.0.0.1:5586"
tca95xx_part_dir := tca95xx_dir / "parts" / "tca95xx"
tca95xx_part_wasm := tca95xx_dir / "parts" / "tca95xx.wasm"

# The part's own tests: the register file, the pins and the state, on the host.
tca95xx-part-test:
    cargo test --manifest-path {{ tca95xx_part_dir }}/Cargo.toml

# Build the virtual TCA9535 -> drivers/tca95xx/parts/tca95xx.wasm.
tca95xx-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ tca95xx_part_dir }}/Cargo.toml"
    cp "{{ tca95xx_part_dir }}/target/wasm32-unknown-unknown/release/tca95xx.wasm" \
        "{{ tca95xx_part_wasm }}"
    ls -l "{{ tca95xx_part_wasm }}"

# The driver's own tests: the configuration and the word write, on the host.
tca95xx-driver-test:
    cargo test --manifest-path {{ tca95xx_dir }}/tca95xx-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
tca95xx-check:
    just check {{ tca95xx_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
tca95xx-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ tca95xx_out }}"
    just check "{{ tca95xx_rel }}"
    just build "{{ tca95xx_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ tca95xx_out }}/tca95xx.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/tca95xx_test" \
        "{{ tca95xx_out }}/tca95xx.elf"
    ls -l "{{ tca95xx_out }}/tca95xx.bin" "{{ tca95xx_out }}/tca95xx.elf"

# Run the firmware with the virtual part on the control channel.
tca95xx-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ tca95xx_out }}/tca95xx.bin --cosim-board {{ tca95xx_dir }}/board.json --control-tcp {{ tca95xx_port }} --elf {{ tca95xx_out }}/tca95xx.elf --timeout {{ timeout }} --log-file {{ tca95xx_out }}/emu.log"

# One control line, e.g. "part io state".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
tca95xx-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ tca95xx_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/tca95xx/drivers/tca95xx.kdl.
tca95xx-driver-record:
    just espforge "driver {{ tca95xx_rel }} add --local {{ tca95xx_rel }}/drivers/tca95xx.kdl"

# Happy path: boot, the pattern on the bus, and the part's own view of it.
tca95xx-sensor: tca95xx-image tca95xx-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ tca95xx_out }}/sensor.stdout"
    log="{{ tca95xx_out }}/emu.log"
    state="{{ tca95xx_out }}/state.txt"
    : > "$out"
    : > "$log"
    setsid just tca95xx-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: tca95xx boot" "firmware boots" 40
    wait_for_log "answers I2C 0x20 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: tca95xx outputs=0x00A5" "the app drives 0x00A5" 20
    just tca95xx-part-input "part io state" > "$state" 2>&1 || true
    grep -Eq 'config[=: ]+0([[:space:]]|$)' "$state" \
        && echo "PASS: both ports are outputs" \
        || { echo "FAIL: the configuration is not 0; see $state" >&2; cat "$state" >&2; exit 1; }
    grep -Eq 'outputs[=: ]+165([[:space:]]|$)' "$state" \
        && echo "PASS: the part reads the latch back" \
        || { echo "FAIL: the latch is not 0x00A5; see $state" >&2; cat "$state" >&2; exit 1; }
    echo "tca95xx-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
tca95xx-clean:
    rm -rf {{ tca95xx_out }} {{ tca95xx_part_dir }}/target


# ---- TLV493D virtual part fixture (drivers/tlv493d) ----------------------

tlv493d_rel := "drivers/tlv493d"
tlv493d_dir := justfile_directory() / tlv493d_rel
tlv493d_out := build_dir / "tlv493d"
tlv493d_port := "127.0.0.1:5587"
tlv493d_part_dir := tlv493d_dir / "parts" / "tlv493d"
tlv493d_part_wasm := tlv493d_dir / "parts" / "tlv493d.wasm"

# The part's own tests: the frame, the factory bytes and the inputs, on the
# host.
tlv493d-part-test:
    cargo test --manifest-path {{ tlv493d_part_dir }}/Cargo.toml

# Build the virtual TLV493D -> drivers/tlv493d/parts/tlv493d.wasm.
tlv493d-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ tlv493d_part_dir }}/Cargo.toml"
    cp "{{ tlv493d_part_dir }}/target/wasm32-unknown-unknown/release/tlv493d.wasm" \
        "{{ tlv493d_part_wasm }}"
    ls -l "{{ tlv493d_part_wasm }}"

# The driver's own tests: the frame layout, the scale and the decode, on the
# host.
tlv493d-driver-test:
    cargo test --manifest-path {{ tlv493d_dir }}/tlv493d-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
tlv493d-check:
    just check {{ tlv493d_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
tlv493d-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ tlv493d_out }}"
    just check "{{ tlv493d_rel }}"
    just build "{{ tlv493d_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ tlv493d_out }}/tlv493d.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/tlv493d_test" \
        "{{ tlv493d_out }}/tlv493d.elf"
    ls -l "{{ tlv493d_out }}/tlv493d.bin" "{{ tlv493d_out }}/tlv493d.elf"

# Run the firmware with the virtual sensor on the control channel.
tlv493d-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ tlv493d_out }}/tlv493d.bin --cosim-board {{ tlv493d_dir }}/board.json --control-tcp {{ tlv493d_port }} --elf {{ tlv493d_out }}/tlv493d.elf --timeout {{ timeout }} --log-file {{ tlv493d_out }}/emu.log"

# One control line, e.g. "part field input set:bx=0.98".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
tlv493d-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ tlv493d_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/tlv493d/drivers/tlv493d.kdl.
tlv493d-driver-record:
    just espforge "driver {{ tlv493d_rel }} add --local {{ tlv493d_rel }}/drivers/tlv493d.kdl"

# Happy path: boot, the field of a fresh part, then two moves and a warmer part.
tlv493d-sensor: tlv493d-image tlv493d-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ tlv493d_out }}/sensor.stdout"
    log="{{ tlv493d_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just tlv493d-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: tlv493d boot" "firmware boots" 40
    wait_for_log "answers I2C 0x5E on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: tlv493d bx=0.00 by=0.00 bz=0.00 mt t=25.00 c" "a fresh part reads no field at 25.00 C" 20
    just tlv493d-part-input "part field input set:bx=0.98" >/dev/null
    wait_for "TEST: tlv493d bx=0.98 by=0.00 bz=0.00 mt t=25.00 c" "set:bx=0.98 -> 0.98 mT" 20
    just tlv493d-part-input "part field input set:bz=-0.49" >/dev/null
    wait_for "TEST: tlv493d bx=0.98 by=0.00 bz=-0.49 mt t=25.00 c" "set:bz=-0.49 -> -0.49 mT" 20
    just tlv493d-part-input "part field input set:temperature=26.1" >/dev/null
    wait_for "TEST: tlv493d bx=0.98 by=0.00 bz=-0.49 mt t=26.10 c" "set:temperature=26.1 -> 26.10 C" 20
    echo "tlv493d-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
tlv493d-clean:
    rm -rf {{ tlv493d_out }} {{ tlv493d_part_dir }}/target


# ---- TMP102 virtual part fixture (drivers/tmp102) ------------------------

tmp102_rel := "drivers/tmp102"
tmp102_dir := justfile_directory() / tmp102_rel
tmp102_out := build_dir / "tmp102"
tmp102_port := "127.0.0.1:5588"
tmp102_part_dir := tmp102_dir / "parts" / "tmp102"
tmp102_part_wasm := tmp102_dir / "parts" / "tmp102.wasm"

# The part's own tests: the register file, the left-justified bytes and the
# input, on the host.
tmp102-part-test:
    cargo test --manifest-path {{ tmp102_part_dir }}/Cargo.toml

# Build the virtual TMP102 -> drivers/tmp102/parts/tmp102.wasm.
tmp102-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ tmp102_part_dir }}/Cargo.toml"
    cp "{{ tmp102_part_dir }}/target/wasm32-unknown-unknown/release/tmp102.wasm" \
        "{{ tmp102_part_wasm }}"
    ls -l "{{ tmp102_part_wasm }}"

# The driver's own tests: the register pointer, the scale and the decode, on
# the host.
tmp102-driver-test:
    cargo test --manifest-path {{ tmp102_dir }}/tmp102-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
tmp102-check:
    just check {{ tmp102_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
tmp102-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ tmp102_out }}"
    just check "{{ tmp102_rel }}"
    just build "{{ tmp102_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ tmp102_out }}/tmp102.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/tmp102_test" \
        "{{ tmp102_out }}/tmp102.elf"
    ls -l "{{ tmp102_out }}/tmp102.bin" "{{ tmp102_out }}/tmp102.elf"

# Run the firmware with the virtual sensor on the control channel.
tmp102-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ tmp102_out }}/tmp102.bin --cosim-board {{ tmp102_dir }}/board.json --control-tcp {{ tmp102_port }} --elf {{ tmp102_out }}/tmp102.elf --timeout {{ timeout }} --log-file {{ tmp102_out }}/emu.log"

# One control line, e.g. "part temp input set:temperature=26.5".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
tmp102-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ tmp102_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/tmp102/drivers/tmp102.kdl.
tmp102-driver-record:
    just espforge "driver {{ tmp102_rel }} add --local {{ tmp102_rel }}/drivers/tmp102.kdl"

# Happy path: boot, 25 C from a fresh part, then a warmer and a colder part.
tmp102-sensor: tmp102-image tmp102-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ tmp102_out }}/sensor.stdout"
    log="{{ tmp102_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just tmp102-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: tmp102 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x48 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: tmp102 t=25.00 c" "a fresh part reads 25.00 C" 20
    just tmp102-part-input "part temp input set:temperature=26.5" >/dev/null
    wait_for "TEST: tmp102 t=26.50 c" "set:temperature=26.5 -> 26.50 C" 20
    just tmp102-part-input "part temp input set:temperature=-5" >/dev/null
    wait_for "TEST: tmp102 t=-5.00 c" "set:temperature=-5 -> -5.00 C" 20
    echo "tmp102-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
tmp102-clean:
    rm -rf {{ tmp102_out }} {{ tmp102_part_dir }}/target


# ---- HT16K33 virtual part fixture (drivers/ht16k33) ----------------------

ht16k33_rel := "drivers/ht16k33"
ht16k33_dir := justfile_directory() / ht16k33_rel
ht16k33_out := build_dir / "ht16k33"
ht16k33_port := "127.0.0.1:5589"
ht16k33_part_dir := ht16k33_dir / "parts" / "ht16k33"
ht16k33_part_wasm := ht16k33_dir / "parts" / "ht16k33.wasm"

# The part's own tests: the commands, the RAM wrapping and the inputs, on the
# host.
ht16k33-part-test:
    cargo test --manifest-path {{ ht16k33_part_dir }}/Cargo.toml

# Build the virtual HT16K33 -> drivers/ht16k33/parts/ht16k33.wasm.
ht16k33-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ ht16k33_part_dir }}/Cargo.toml"
    cp "{{ ht16k33_part_dir }}/target/wasm32-unknown-unknown/release/ht16k33.wasm" \
        "{{ ht16k33_part_wasm }}"
    ls -l "{{ ht16k33_part_wasm }}"

# The driver's own tests: the setup commands, the frame write and the read-back,
# on the host.
ht16k33-driver-test:
    cargo test --manifest-path {{ ht16k33_dir }}/ht16k33-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
ht16k33-check:
    just check {{ ht16k33_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
ht16k33-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ht16k33_out }}"
    just check "{{ ht16k33_rel }}"
    just build "{{ ht16k33_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ ht16k33_out }}/ht16k33.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/ht16k33_test" \
        "{{ ht16k33_out }}/ht16k33.elf"
    ls -l "{{ ht16k33_out }}/ht16k33.bin" "{{ ht16k33_out }}/ht16k33.elf"

# Run the firmware with the virtual part on the control channel.
ht16k33-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ ht16k33_out }}/ht16k33.bin --cosim-board {{ ht16k33_dir }}/board.json --control-tcp {{ ht16k33_port }} --elf {{ ht16k33_out }}/ht16k33.elf --timeout {{ timeout }} --log-file {{ ht16k33_out }}/emu.log"

# One control line, e.g. "part display input set:brightness=9".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
ht16k33-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ ht16k33_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/ht16k33/drivers/ht16k33.kdl.
ht16k33-driver-record:
    just espforge "driver {{ ht16k33_rel }} add --local {{ ht16k33_rel }}/drivers/ht16k33.kdl"

# Happy path: boot, the diagonal frame written and read back every second.
ht16k33-sensor: ht16k33-image ht16k33-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ ht16k33_out }}/sensor.stdout"
    log="{{ ht16k33_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just ht16k33-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: ht16k33 boot" "firmware boots" 40
    wait_for_log "answers I2C 0x70 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: ht16k33 ram=01020408102040800102040810204080" "the frame reads back from the part's RAM" 20
    echo "ht16k33-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
ht16k33-clean:
    rm -rf {{ ht16k33_out }} {{ ht16k33_part_dir }}/target

# ---- MCP23xxx virtual part fixture (drivers/mcp23xxx) --------------------

mcp23xxx_rel := "drivers/mcp23xxx"
mcp23xxx_dir := justfile_directory() / mcp23xxx_rel
mcp23xxx_out := build_dir / "mcp23xxx"
mcp23xxx_port := "127.0.0.1:5590"
mcp23xxx_part_dir := mcp23xxx_dir / "parts" / "mcp23xxx"
mcp23xxx_part_wasm := mcp23xxx_dir / "parts" / "mcp23xxx.wasm"

# The part's own tests: the register file, the pins and the inputs, on the
# host.
mcp23xxx-part-test:
    cargo test --manifest-path {{ mcp23xxx_part_dir }}/Cargo.toml

# Build the virtual MCP23017 -> drivers/mcp23xxx/parts/mcp23xxx.wasm.
mcp23xxx-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ mcp23xxx_part_dir }}/Cargo.toml"
    cp "{{ mcp23xxx_part_dir }}/target/wasm32-unknown-unknown/release/mcp23xxx.wasm" \
        "{{ mcp23xxx_part_wasm }}"
    ls -l "{{ mcp23xxx_part_wasm }}"

# The driver's own tests: the constructor, the per-port writes and the read,
# on the host.
mcp23xxx-driver-test:
    cargo test --manifest-path {{ mcp23xxx_dir }}/mcp23xxx-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
mcp23xxx-check:
    just check {{ mcp23xxx_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
mcp23xxx-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ mcp23xxx_out }}"
    just check "{{ mcp23xxx_rel }}"
    just build "{{ mcp23xxx_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ mcp23xxx_out }}/mcp23xxx.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/mcp23xxx_test" \
        "{{ mcp23xxx_out }}/mcp23xxx.elf"
    ls -l "{{ mcp23xxx_out }}/mcp23xxx.bin" "{{ mcp23xxx_out }}/mcp23xxx.elf"

# Run the firmware with the virtual part on the control channel.
mcp23xxx-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ mcp23xxx_out }}/mcp23xxx.bin --cosim-board {{ mcp23xxx_dir }}/board.json --control-tcp {{ mcp23xxx_port }} --elf {{ mcp23xxx_out }}/mcp23xxx.elf --timeout {{ timeout }} --log-file {{ mcp23xxx_out }}/emu.log"

# One control line, e.g. "part io input set:levels=0x5A00".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
mcp23xxx-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ mcp23xxx_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/mcp23xxx/drivers/mcp23xxx.kdl.
mcp23xxx-driver-record:
    just espforge "driver {{ mcp23xxx_rel }} add --local {{ mcp23xxx_rel }}/drivers/mcp23xxx.kdl"

# Happy path: boot, port A driving its latch, then the part driving port B.
mcp23xxx-sensor: mcp23xxx-image mcp23xxx-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ mcp23xxx_out }}/sensor.stdout"
    log="{{ mcp23xxx_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just mcp23xxx-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: mcp23xxx boot" "firmware boots" 40
    wait_for_log "answers I2C 0x20 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: mcp23xxx a=aa b=00" "port A drives its outputs and port B reads its pins" 20
    just mcp23xxx-part-input "part io input set:levels=0x5A00" >/dev/null
    wait_for "TEST: mcp23xxx a=aa b=5a" "set:levels=0x5A00 -> port B reads 0x5a" 20
    echo "mcp23xxx-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
mcp23xxx-clean:
    rm -rf {{ mcp23xxx_out }} {{ mcp23xxx_part_dir }}/target

# ---- PCF857x virtual part fixture (drivers/pcf857x) ----------------------

pcf857x_rel := "drivers/pcf857x"
pcf857x_dir := justfile_directory() / pcf857x_rel
pcf857x_out := build_dir / "pcf857x"
pcf857x_port := "127.0.0.1:5591"
pcf857x_part_dir := pcf857x_dir / "parts" / "pcf857x"
pcf857x_part_wasm := pcf857x_dir / "parts" / "pcf857x.wasm"

# The part's own tests: the port, the pins and the world's drive, on the host.
pcf857x-part-test:
    cargo test --manifest-path {{ pcf857x_part_dir }}/Cargo.toml

# Build the virtual PCF8574 -> drivers/pcf857x/parts/pcf857x.wasm.
pcf857x-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ pcf857x_part_dir }}/Cargo.toml"
    cp "{{ pcf857x_part_dir }}/target/wasm32-unknown-unknown/release/pcf857x.wasm" \
        "{{ pcf857x_part_wasm }}"
    ls -l "{{ pcf857x_part_wasm }}"

# The driver's own tests: the port byte and the read, on the host.
pcf857x-driver-test:
    cargo test --manifest-path {{ pcf857x_dir }}/pcf857x-driver/Cargo.toml

# Schema, resource graph, manifest discovery and the driver's path dependency.
pcf857x-check:
    just check {{ pcf857x_rel }}

# Compile the generated project: the manifest's type, constructor and endpoint,
# and the crate path in the generated Cargo.toml.
pcf857x-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ pcf857x_out }}"
    just check "{{ pcf857x_rel }}"
    just build "{{ pcf857x_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ pcf857x_out }}/pcf857x.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/pcf857x_test" \
        "{{ pcf857x_out }}/pcf857x.elf"
    ls -l "{{ pcf857x_out }}/pcf857x.bin" "{{ pcf857x_out }}/pcf857x.elf"

# Run the firmware with the virtual part on the control channel.
pcf857x-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ pcf857x_out }}/pcf857x.bin --cosim-board {{ pcf857x_dir }}/board.json --control-tcp {{ pcf857x_port }} --elf {{ pcf857x_out }}/pcf857x.elf --timeout {{ timeout }} --log-file {{ pcf857x_out }}/emu.log"

# One control line, e.g. "part io input set:levels=0x0F".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
pcf857x-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ pcf857x_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/pcf857x/drivers/pcf857x.kdl.
pcf857x-driver-record:
    just espforge "driver {{ pcf857x_rel }} add --local {{ pcf857x_rel }}/drivers/pcf857x.kdl"

# Happy path: boot, the pins reading back the port, then the world pulling low.
pcf857x-sensor: pcf857x-image pcf857x-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ pcf857x_out }}/sensor.stdout"
    log="{{ pcf857x_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just pcf857x-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: pcf857x boot" "firmware boots" 40
    wait_for_log "answers I2C 0x20 on GPIO 6/5" "the part attached on the firmware's SDA/SCL" 10
    wait_for "TEST: pcf857x pins=aa" "the eight pins read back what was written" 20
    just pcf857x-part-input "part io input set:levels=0x0F" >/dev/null
    wait_for "TEST: pcf857x pins=0a" "set:levels=0x0F -> the pins read 0x0a" 20
    echo "pcf857x-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
pcf857x-clean:
    rm -rf {{ pcf857x_out }} {{ pcf857x_part_dir }}/target


# ---- MAX7219 virtual part fixture (drivers/max7219) ---------------------

max7219_rel := "drivers/max7219"
max7219_dir := justfile_directory() / max7219_rel
max7219_out := build_dir / "max7219"
max7219_port := "127.0.0.1:5592"
max7219_part_dir := max7219_dir / "parts" / "max7219"
max7219_part_wasm := max7219_dir / "parts" / "max7219.wasm"

# The part's own tests: the register file, the latch and the state, on the host.
max7219-part-test:
    cargo test --manifest-path {{ max7219_part_dir }}/Cargo.toml

# Build the virtual MAX7219 -> drivers/max7219/parts/max7219.wasm.
max7219-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ max7219_part_dir }}/Cargo.toml"
    cp "{{ max7219_part_dir }}/target/wasm32-unknown-unknown/release/max7219.wasm" \
        "{{ max7219_part_wasm }}"
    ls -l "{{ max7219_part_wasm }}"

# Schema, resource graph, manifest discovery and the crate dependency.
max7219-check:
    just check {{ max7219_rel }}

# Compile the generated project: the busless manifest's type and pin
# constructor, and the crates.io dependency in the generated Cargo.toml.
max7219-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ max7219_out }}"
    just check "{{ max7219_rel }}"
    just build "{{ max7219_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ max7219_out }}/max7219.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/max7219_test" \
        "{{ max7219_out }}/max7219.elf"
    ls -l "{{ max7219_out }}/max7219.bin" "{{ max7219_out }}/max7219.elf"

# Run the firmware with the virtual display on the control channel.
max7219-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ max7219_out }}/max7219.bin --cosim-board {{ max7219_dir }}/board.json --control-tcp {{ max7219_port }} --elf {{ max7219_out }}/max7219.elf --timeout {{ timeout }} --log-file {{ max7219_out }}/emu.log"

# One control line, e.g. "part matrix state".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
max7219-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ max7219_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing drivers/max7219/drivers/max7219.kdl.
max7219-driver-record:
    just espforge "driver {{ max7219_rel }} add --local {{ max7219_rel }}/drivers/max7219.kdl"

# Happy path: boot, attach, a frame every second, and the part's register file.
max7219-sensor: max7219-image max7219-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ max7219_out }}/sensor.stdout"
    log="{{ max7219_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just max7219-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: max7219 boot" "firmware boots" 40
    wait_for_log "part matrix (max7219) attached" "the part attached on the firmware's pins" 10
    wait_for "TEST: max7219 frame=aaaaaaaaaaaaaaaa" "a frame was written to the digits" 20
    state=$(just max7219-part-input "part matrix state")
    echo "$state"
    for want in "digits=aaaaaaaaaaaaaaaa" "intensity=0f" "power=on" "scan=7" "decode=00" "test=off"; do
        [[ "$state" == *"$want"* ]] || { echo "FAIL: the part did not show '$want'; got: $state" >&2; exit 1; }
    done
    echo "PASS: the part latched the frame and the driver's control registers"
    echo "max7219-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
max7219-clean:
    rm -rf {{ max7219_out }} {{ max7219_part_dir }}/target


# ---- 74HC595 virtual part fixture (drivers/nxp74hc595) -------------------

nxp74hc595_rel := "drivers/nxp74hc595"
nxp74hc595_dir := justfile_directory() / nxp74hc595_rel
nxp74hc595_out := build_dir / "nxp74hc595"
nxp74hc595_port := "127.0.0.1:5593"
nxp74hc595_part_dir := nxp74hc595_dir / "parts" / "nxp74hc595"
nxp74hc595_part_wasm := nxp74hc595_dir / "parts" / "nxp74hc595.wasm"

# The part's own tests: the shift register, the latch and the state, on the host.
nxp74hc595-part-test:
    cargo test --manifest-path {{ nxp74hc595_part_dir }}/Cargo.toml

# Build the virtual 74HC595 -> drivers/nxp74hc595/parts/nxp74hc595.wasm.
nxp74hc595-part:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --target wasm32-unknown-unknown \
        --manifest-path "{{ nxp74hc595_part_dir }}/Cargo.toml"
    cp "{{ nxp74hc595_part_dir }}/target/wasm32-unknown-unknown/release/nxp74hc595.wasm" \
        "{{ nxp74hc595_part_wasm }}"
    ls -l "{{ nxp74hc595_part_wasm }}"

# Schema, resource graph, manifest discovery and the crate dependency.
nxp74hc595-check:
    just check {{ nxp74hc595_rel }}

# Compile the generated project: the busless manifest's type and pin
# constructor, and the crates.io dependency in the generated Cargo.toml.
nxp74hc595-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ nxp74hc595_out }}"
    just check "{{ nxp74hc595_rel }}"
    just build "{{ nxp74hc595_rel }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ nxp74hc595_out }}/nxp74hc595.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/nxp74hc595_test" \
        "{{ nxp74hc595_out }}/nxp74hc595.elf"
    ls -l "{{ nxp74hc595_out }}/nxp74hc595.bin" "{{ nxp74hc595_out }}/nxp74hc595.elf"

# Run the firmware with the virtual register on the control channel.
nxp74hc595-run timeout="90s":
    just esp-emu "--chip esp32c3 --firmware {{ nxp74hc595_out }}/nxp74hc595.bin --cosim-board {{ nxp74hc595_dir }}/board.json --control-tcp {{ nxp74hc595_port }} --elf {{ nxp74hc595_out }}/nxp74hc595.elf --timeout {{ timeout }} --log-file {{ nxp74hc595_out }}/emu.log"

# One control line, e.g. "part shift state".
# `addr` stays host:port for readability; netcat-openbsd wants them separate.
nxp74hc595-part-input args:
    #!/usr/bin/env bash
    set -euo pipefail
    addr="{{ nxp74hc595_port }}"
    printf '%s\n' "{{ args }}" | nc -q1 "${addr%:*}" "${addr##*:}"

# Re-record the manifest hash after editing
# drivers/nxp74hc595/drivers/nxp74hc595.kdl.
nxp74hc595-driver-record:
    just espforge "driver {{ nxp74hc595_rel }} add --local {{ nxp74hc595_rel }}/drivers/nxp74hc595.kdl"

# Happy path: boot, attach, the startup pattern, the steady frame and the latch.
nxp74hc595-sensor: nxp74hc595-image nxp74hc595-part
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ nxp74hc595_out }}/sensor.stdout"
    log="{{ nxp74hc595_out }}/emu.log"
    : > "$out"
    : > "$log"
    setsid just nxp74hc595-run 90s > "$out" 2>&1 &
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
    wait_for "TEST: nxp74hc595 boot" "firmware boots" 40
    wait_for_log "part shift (nxp74hc595) attached" "the part attached on the firmware's pins" 10
    wait_for "TEST: nxp74hc595 load=5a" "the startup pattern was loaded" 20
    wait_for "TEST: nxp74hc595 load=aa" "the steady frame was loaded" 20
    state=$(just nxp74hc595-part-input "part shift state")
    echo "$state"
    for want in "latch=aa" "shift=aa" "output=enabled"; do
        [[ "$state" == *"$want"* ]] || { echo "FAIL: the part did not show '$want'; got: $state" >&2; exit 1; }
    done
    echo "PASS: the part latched the byte and enabled its outputs"
    echo "nxp74hc595-sensor PASS"

# Remove the disposable images; the fixture and the checked-in wasm stay.
nxp74hc595-clean:
    rm -rf {{ nxp74hc595_out }} {{ nxp74hc595_part_dir }}/target


# ---- examples/001-wifi-basic (networking basics) -------------------------

wifi_basic_dir := justfile_directory() / "examples" / "001-wifi-basic"
wifi_basic_out := build_dir / "wifi-basic"

# Schema, resource graph and the `<wifi>`/`<wifi_station>` pairing.
wifi-basic-check:
    just check {{ wifi_basic_dir }}

# Compile the example as a merged image -> build/wifi-basic/wifi-basic.bin.
wifi-basic-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ wifi_basic_out }}"
    just check "{{ wifi_basic_dir }}"
    just build "{{ wifi_basic_dir }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ wifi_basic_out }}/wifi-basic.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/wifi_basic" \
        "{{ wifi_basic_out }}/wifi-basic.elf"
    ls -l "{{ wifi_basic_out }}/wifi-basic.bin" "{{ wifi_basic_out }}/wifi-basic.elf"

# Run it against the emulator's soft AP. The SSID and password must match the
# firmware's `<wifi_station>` (see gentle-intro-wifi.md).
wifi-basic-run timeout="60s" ssid="MyNetwork" password="MyPassword":
    just esp-emu "--chip esp32c3 --firmware {{ wifi_basic_out }}/wifi-basic.bin --net user --wifi-ssid {{ ssid }} --wifi-password {{ password }} --elf {{ wifi_basic_out }}/wifi-basic.elf --timeout {{ timeout }} --log-file {{ wifi_basic_out }}/emu.log"

# T-gate: boot, associate, and prove DHCP handed out 192.168.4.2.
wifi-basic-test: wifi-basic-image
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ wifi_basic_out }}/test.stdout"
    : > "{{ wifi_basic_out }}/emu.log"
    : > "$out"
    setsid just wifi-basic-run 60s > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    got=0
    for _ in $(seq 1 60); do
        grep -q 'TEST: ip ' "$out" && { got=1; break; }
        sleep 1
    done
    (( got )) || { echo "FAIL: no address within 60s; see $out" >&2; exit 1; }
    grep -E 'TEST: (wifi boot|ip )' "$out"
    if ! grep -q 'TEST: ip 192.168.4.2$' "$out"; then
        echo "FAIL: the station did not get 192.168.4.2; see $out" >&2
        exit 1
    fi
    echo "PASS: associated and acquired 192.168.4.2"


# ---- examples/002-http-example (one GET to the internet) -----------------

http_example_dir := justfile_directory() / "examples" / "002-http-example"
http_example_out := build_dir / "http-example"

# Schema, resource graph and the `network="$home"` provider reference.
http-example-check:
    just check {{ http_example_dir }}

# Compile the example as a merged image -> build/http-example/http-example.bin.
http-example-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ http_example_out }}"
    just check "{{ http_example_dir }}"
    just build "{{ http_example_dir }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ http_example_out }}/http-example.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/http_example" \
        "{{ http_example_out }}/http-example.elf"
    ls -l "{{ http_example_out }}/http-example.bin" "{{ http_example_out }}/http-example.elf"

# Run it. `--net user` NATs the guest's TCP through host sockets and forwards
# DNS from the host's /etc/resolv.conf, so the device reaches the real web.
http-example-run timeout="90s" ssid="MyNetwork" password="MyPassword":
    just esp-emu "--chip esp32c3 --firmware {{ http_example_out }}/http-example.bin --net user --wifi-ssid {{ ssid }} --wifi-password {{ password }} --elf {{ http_example_out }}/http-example.elf --timeout {{ timeout }} --log-file {{ http_example_out }}/emu.log"

# Gate: association, a lease, and a successful GET of the public URL.
# Needs the host to have DNS and outbound HTTP; `http-example-local-test`
# below is the version that needs no internet at all.
http-example-test: http-example-image
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ http_example_out }}/test.stdout"
    : > "{{ http_example_out }}/emu.log"
    : > "$out"
    setsid just http-example-run 90s > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    got=0
    for _ in $(seq 1 90); do
        grep -qE 'TEST: http (ok|failed)' "$out" && { got=1; break; }
        sleep 1
    done
    grep -E 'TEST: (ip |http )' "$out" || true
    (( got )) || { echo "FAIL: no GET result within 90s; see $out" >&2; exit 1; }
    if ! grep -q 'TEST: http ok' "$out"; then
        echo "FAIL: the GET did not succeed; see $out and {{ http_example_out }}/emu.log" >&2
        exit 1
    fi
    echo "PASS: associated, leased an address and fetched the public URL"


# ---- examples/003-https-example (one GET over TLS 1.3) -------------------

https_example_dir := justfile_directory() / "examples" / "003-https-example"
https_example_out := build_dir / "https-example"

# Schema, resource graph, the `network="$home"` reference and the TLS heap.
https-example-check:
    just check {{ https_example_dir }}

# Compile the example as a merged image -> build/https-example/https-example.bin.
https-example-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ https_example_out }}"
    just check "{{ https_example_dir }}"
    just build "{{ https_example_dir }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ https_example_out }}/https-example.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/https_example" \
        "{{ https_example_out }}/https-example.elf"
    ls -l "{{ https_example_out }}/https-example.bin" "{{ https_example_out }}/https-example.elf"

# Run it. `--net user` NATs the guest's TCP through host sockets and forwards
# DNS from the host's /etc/resolv.conf, so the TLS handshake reaches the web.
https-example-run timeout="120s" ssid="MyNetwork" password="MyPassword":
    just esp-emu "--chip esp32c3 --firmware {{ https_example_out }}/https-example.bin --net user --wifi-ssid {{ ssid }} --wifi-password {{ password }} --elf {{ https_example_out }}/https-example.elf --timeout {{ timeout }} --log-file {{ https_example_out }}/emu.log"

# Gate: association, a lease, and a successful GET over TLS.
# Needs the host to have DNS and outbound TCP to 443.
https-example-test: https-example-image
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ https_example_out }}/test.stdout"
    : > "{{ https_example_out }}/emu.log"
    : > "$out"
    setsid just https-example-run 120s > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    got=0
    for _ in $(seq 1 120); do
        grep -qE 'TEST: https (ok|failed)' "$out" && { got=1; break; }
        sleep 1
    done
    grep -E 'TEST: (ip |https )' "$out" || true
    (( got )) || { echo "FAIL: no GET result within 120s; see $out" >&2; exit 1; }
    if ! grep -q 'TEST: https ok' "$out"; then
        echo "FAIL: the TLS GET did not succeed; see $out and {{ https_example_out }}/emu.log" >&2
        exit 1
    fi
    echo "PASS: associated, leased an address and fetched the public URL over TLS"


# ---- examples/005-ntp (one clock sync, then local reads) ------------------

ntp_example_dir := justfile_directory() / "examples" / "005-ntp"
ntp_example_out := build_dir / "ntp-example"

# Schema, resource graph and the `<ntp>` -> `<wifi_station>` reference.
ntp-example-check:
    just check {{ ntp_example_dir }}

# Compile the example as a merged image -> build/ntp-example/ntp-example.bin.
ntp-example-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ntp_example_out }}"
    just check "{{ ntp_example_dir }}"
    just build "{{ ntp_example_dir }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ ntp_example_out }}/ntp-example.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/ntp_example" \
        "{{ ntp_example_out }}/ntp-example.elf"
    ls -l "{{ ntp_example_out }}/ntp-example.bin" "{{ ntp_example_out }}/ntp-example.elf"

# Run it. `--net user` NATs the guest's UDP through host sockets and forwards
# DNS from the host's /etc/resolv.conf, so the SNTP query reaches the internet.
ntp-example-run timeout="90s" ssid="MyNetwork" password="MyPassword":
    just esp-emu "--chip esp32c3 --firmware {{ ntp_example_out }}/ntp-example.bin --net user --wifi-ssid {{ ssid }} --wifi-password {{ password }} --elf {{ ntp_example_out }}/ntp-example.elf --timeout {{ timeout }} --log-file {{ ntp_example_out }}/emu.log"

# Gate: association, a lease, and a successful SNTP sync.
# Needs the host to have DNS and outbound UDP to port 123.
ntp-example-test: ntp-example-image
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ ntp_example_out }}/test.stdout"
    : > "{{ ntp_example_out }}/emu.log"
    : > "$out"
    setsid just ntp-example-run 90s > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    got=0
    for _ in $(seq 1 90); do
        grep -qE 'TEST: ntp (ok|failed)' "$out" && { got=1; break; }
        sleep 1
    done
    grep -E 'TEST: (ip |ntp )' "$out" || true
    (( got )) || { echo "FAIL: no sync result within 90s; see $out" >&2; exit 1; }
    if ! grep -q 'TEST: ntp ok' "$out"; then
        echo "FAIL: the SNTP sync did not succeed; see $out and {{ ntp_example_out }}/emu.log" >&2
        exit 1
    fi
    echo "PASS: associated, leased an address and synchronised the clock"


# ---- examples/websockets (one echo over ws:// and one over wss://) -------

websockets_dir := justfile_directory() / "examples" / "websockets"
websockets_out := build_dir / "websockets"

# Schema, resource graph and the two `network="$home"` references.
websockets-check:
    just check {{ websockets_dir }}

# Compile the example as a merged image -> build/websockets/websockets.bin.
websockets-image:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ websockets_out }}"
    just check "{{ websockets_dir }}"
    just build "{{ websockets_dir }}"
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            "{{ websockets_out }}/websockets.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/websockets" \
        "{{ websockets_out }}/websockets.elf"
    ls -l "{{ websockets_out }}/websockets.bin" "{{ websockets_out }}/websockets.elf"

# Run it. `--net user` NATs the guest's TCP through host sockets and forwards
# DNS from the host's /etc/resolv.conf, so both echo servers are reachable.
websockets-run timeout="120s" ssid="MyNetwork" password="MyPassword":
    just esp-emu "--chip esp32c3 --firmware {{ websockets_out }}/websockets.bin --net user --wifi-ssid {{ ssid }} --wifi-password {{ password }} --elf {{ websockets_out }}/websockets.elf --timeout {{ timeout }} --log-file {{ websockets_out }}/emu.log"

# Gate: association, a lease, and one echo over each transport.
# Needs the host to have DNS and outbound TCP to 80 and 443.
websockets-test: websockets-image
    #!/usr/bin/env bash
    set -euo pipefail
    out="{{ websockets_out }}/test.stdout"
    : > "{{ websockets_out }}/emu.log"
    : > "$out"
    setsid just websockets-run 120s > "$out" 2>&1 &
    emu=$!
    trap 'kill -- -"$emu" 2>/dev/null || true; wait "$emu" 2>/dev/null || true' EXIT
    wait_for() {
        local i
        for i in $(seq 1 120); do
            grep -q "$1" "$out" && { echo "PASS: $2"; return 0; }
            sleep 1
        done
        echo "FAIL: $2 (no '$1'); see $out and {{ websockets_out }}/emu.log" >&2
        return 1
    }
    wait_for 'TEST: ws ok' 'plain ws:// echoed the text frame' || exit 1
    wait_for 'TEST: wss ok' 'secure wss:// echoed the text frame' || exit 1
    grep -E 'TEST: (ip |ws |wss |websockets )' "$out"
    echo "PASS: both WebSocket variants echoed a text frame"
