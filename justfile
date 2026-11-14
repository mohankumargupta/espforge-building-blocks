set shell := ["bash", "-cu"]

espforge := justfile_directory() / "espforge" / "target" / "debug" / "espforge"
runtime := justfile_directory() / "espforge" / "espforge-runtime"
build_dir := justfile_directory() / "build"

ota_project := justfile_directory() / "espressifemulator" / "002-wifi-ota"
ota_dir := build_dir / "ota"
ota_src := build_dir / "ota-src"
ota_tool := ota_project / "scripts" / "image-info.py"

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
    just build example=blocks/002-wifi-ota
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            --partition-table partitions.csv --target-app-partition ota_0 \
            "{{ ota_dir }}/base.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/wifi_ota" \
        "{{ ota_dir }}/base.elf"
    python3 "{{ ota_tool }}" "{{ ota_dir }}/base.bin"

# OTA payload: the espforge blink example, app image only -> build/ota/blink.bin.
ota-blink:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ota_dir }}" "{{ ota_src }}"
    rm -rf "{{ ota_src }}/blink"
    just espforge "examples get 01.Basics/blink --out build/ota-src/blink"
    just build example=build/ota-src/blink
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
    just check example=espressifemulator/002-wifi-ota/v1
    just build example=espressifemulator/002-wifi-ota/v1
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --merge \
            --chip esp32c3 --flash-size 4mb \
            --partition-table partitions.csv --target-app-partition ota_0 \
            "{{ ota_dir }}/v1.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/wifi_ota_v1" \
        "{{ ota_dir }}/v1.elf"
    python3 "{{ ota_tool }}" "{{ ota_dir }}/v1.bin"

# v1 with constants matching the current blink payload (the T2/T3 fixture).
ota-v1: ota-blink ota-patch-v1 ota-build-v1

# T4 payload: blink + <ota /> + mark_app_valid() -> build/ota/blink.fw.bin.
ota-blink-confirm:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{ ota_dir }}"
    just build example=espressifemulator/002-wifi-ota/blink-confirm
    (cd "{{ build_dir }}/build" \
        && cargo espflash save-image --chip esp32c3 --flash-size 4mb \
            "{{ ota_dir }}/blink.fw.bin")
    cp "{{ build_dir }}/build/target/riscv32imc-unknown-none-elf/debug/blink_confirm" \
        "{{ ota_dir }}/blink-confirm.elf"
    python3 "{{ ota_tool }}" "{{ ota_dir }}/blink.fw.bin"

# Size, CRC32 and slot fit for an OTA image (T6). Prints the Rust constants.
ota-image-info image=(ota_dir / "blink.bin"):
    python3 "{{ ota_tool }}" "{{ image }}"

# Serve build/ota/ to the guest's gateway (192.168.4.1:8000 -> host loopback).
ota-serve port="8000":
    python3 -m http.server {{ port }} --bind 127.0.0.1 --directory "{{ ota_dir }}"

# T0/T1: boot block 002 as shipped and watch it associate.
ota-t0 timeout="45s":
    just esp-emu "--chip esp32c3 --firmware {{ ota_dir }}/base.bin --net user --wifi-ssid MyNetwork --wifi-password MyPassword --timeout {{ timeout }} --log-file {{ ota_dir }}/t0.log"

# T2-T5/T7: run the v1 updater with the control channel open.
ota-t2 timeout="120s":
    just esp-emu "--chip esp32c3 --firmware {{ ota_dir }}/v1.bin --net user --wifi-ssid MyNetwork --wifi-password MyPassword --control-tcp 127.0.0.1:5558 --elf {{ ota_dir }}/v1.elf --elf {{ ota_dir }}/blink.elf --timeout {{ timeout }} --log-file {{ ota_dir }}/t2.log"

# T3/T4: hard reset the running emulator (same as a DTR/RTS toggle).
ota-reset addr="127.0.0.1:5558":
    printf 'reset\n' | nc -q1 {{ addr }}

# Evidence for T2/T3/T5: dump the 32-byte otadata region.
ota-otadata addr="127.0.0.1:5558" out=(ota_dir / "otadata.bin"):
    #!/usr/bin/env bash
    set -euo pipefail
    printf 'read-region 0xd000 32 %s\n' "{{ out }}" | nc -q1 {{ addr }}
    od -An -tx1 -N32 "{{ out }}"

# T3b: work on a copy so --save-state never rewrites the pristine v1 image.
ota-state-copy:
    cp "{{ ota_dir }}/v1.bin" "{{ ota_dir }}/v1-state.bin"

# Remove the disposable fixtures (build/ is gitignored).
ota-clean:
    rm -rf "{{ ota_dir }}" "{{ ota_src }}"
