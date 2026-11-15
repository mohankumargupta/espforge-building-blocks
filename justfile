set shell := ["bash", "-cu"]

espforge := justfile_directory() / "espforge" / "target" / "debug" / "espforge"
runtime := justfile_directory() / "espforge" / "espforge-runtime"
build_dir := justfile_directory() / "build"

ota_project := justfile_directory() / "espressifemulator" / "002-wifi-ota"
ota_dir := build_dir / "ota"
ota_src := build_dir / "ota-src"
ota_tool := ota_project / "scripts" / "image-info.py"

sht31_dir := justfile_directory() / "drivers" / "001-sht31"
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


# ---- 001-sht31 driver fixture (testsht31.md) ----------------------------

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
sht31-part args:
    printf '%s\n' "{{ args }}" | nc -q1 {{ sht31_port }}

# T1: scan the bus from the espforge i2c_scanner example, on the board's pins.
sht31-scan:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ sht31_out }}" "{{ sht31_src }}"
    rm -rf "{{ sht31_src }}/scanner"
    just espforge "new --example i2c_scanner --chip esp32c3 --dir build/sht31-src/scanner"
    sed -i -E 's/sda="[0-9]+" scl="[0-9]+"/sda="5" scl="6"/' \
        "{{ sht31_src }}/scanner/i2c_scanner.espf"
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
