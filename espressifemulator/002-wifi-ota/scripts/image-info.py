#!/usr/bin/env python3
"""Report an OTA fixture's size and CRC32 for testota.md.

`begin(size, crc32)` needs both before the download starts, and T6 needs the
image to fit the OTA slot. With `--patch`, rewrite the two constants in the v1
app so `just ota-build-v1` picks them up.

Exit codes: 0 fits, 1 does not fit the slot, 2 bad input.
"""

from __future__ import annotations

import argparse
import re
import sys
import zlib
from pathlib import Path

# ota_0/ota_1 size from <ota flash_size="4MB" /> (testota.md §2).
SLOT_SIZE = 0x1F0000
IMAGE_MAGIC = 0xE9

SIZE_LINE = re.compile(r"^const IMAGE_SIZE: u32 = .*;$", re.MULTILINE)
CRC_LINE = re.compile(r"^const IMAGE_CRC32: u32 = .*;$", re.MULTILINE)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "image", type=Path, help="OTA image, e.g. build/ota/blink.bin"
    )
    parser.add_argument(
        "--patch",
        type=Path,
        metavar="APP_RS",
        help="rewrite IMAGE_SIZE/IMAGE_CRC32 in this app.rs",
    )
    args = parser.parse_args()

    if not args.image.is_file():
        print(f"error: missing image: {args.image}", file=sys.stderr)
        return 2

    data = args.image.read_bytes()
    if not data:
        print(f"error: empty image: {args.image}", file=sys.stderr)
        return 2

    size = len(data)
    crc = zlib.crc32(data) & 0xFFFFFFFF
    fits = size < SLOT_SIZE

    print(f"image: {args.image}")
    print(f"size:  {size} (0x{size:x}) bytes")
    print(f"crc32: 0x{crc:08x}")
    print(f"slot:  {SLOT_SIZE} (0x{SLOT_SIZE:x}) bytes  [T6]")
    if fits:
        print("fits:  yes")
    else:
        print("fits:  NO - rebuild release or raise <ota flash_size>")

    if data[0] != IMAGE_MAGIC:
        print(
            f"warn:  first byte is 0x{data[0]:02x}, not 0xe9 - not an ESP image",
            file=sys.stderr,
        )

    print("rust:")
    print(f"const IMAGE_SIZE: u32 = {size};")
    print(f"const IMAGE_CRC32: u32 = 0x{crc:08x};")

    if args.patch:
        text = args.patch.read_text()
        patched, size_hits = SIZE_LINE.subn(
            f"const IMAGE_SIZE: u32 = {size};", text, count=1
        )
        patched, crc_hits = CRC_LINE.subn(
            f"const IMAGE_CRC32: u32 = 0x{crc:08x};", patched, count=1
        )
        if size_hits != 1 or crc_hits != 1:
            print(
                f"error: {args.patch} has no IMAGE_SIZE/IMAGE_CRC32 const to patch",
                file=sys.stderr,
            )
            return 2
        args.patch.write_text(patched)
        print(f"patched: {args.patch}")

    return 0 if fits else 1


if __name__ == "__main__":
    sys.exit(main())
