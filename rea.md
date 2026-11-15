# REA: reverse-engineering esp-emu's I²C controller model

**Summary.** `repro.md` §5 ends at: the remaining fault is the pair `esp-hal`
1.2.2 ↔ `esp-emu` 1.0.0. This document takes the next step with binary reverse
engineering: it dissects the installed `esp-emu` binary (`~/.local/bin/esp-emu`)
and combines what the binary leaks with what the vendored `esp-hal` 1.2.2
source demands of the hardware, down to single register bits. It already
narrows the bug to one specific register bit and names the functions Ghidra has
to open to confirm it.

| | |
|---|---|
| Binary | `~/.local/bin/esp-emu`, 14 515 752 bytes |
| SHA-256 | `d228c4986140afe9fc13d75af515196f5c4d4b1134ed26b16fd84d7314a6f7a5` (`build/rea/esp-emu.sha256`) |
| Format | ELF64, **aarch64**, statically linked, **stripped**, non-PIE `EXEC`, entry `0x4437b8` |
| Provenance | Rust release build; CI path `/builds/security/esp-emulator` (GitLab, group `security`, project `esp-emulator`); rustc commit `0ef1bc386059596dada8fb65b50425aab2ea637a` |
| Contents | a Rust host with the wasmtime/cranelift stack (for co-simulated WASM parts) plus per-chip peripheral models; clap 4.6 CLI |
| Chip in scope | ESP32-C3 only (AGENTS.md); C3 `I2C0` register block base `0x60004000` |

Host tooling used so far: `objdump`/`readelf`/`strings` from binutils (the
board is aarch64, so native aarch64 disassembly works out of the box), plus two
small Python scans (§3). Ghidra is the tool for the next step (§5); everything
it needs to be pointed at is already mapped (§4).

---

## 1. What the binary leaks without a disassembler

Even stripped, the binary keeps (a) every Rust panic `Location`, which contains
the emulator's own source paths **and line numbers**, and (b) its diagnostic
strings. These give us the emulator's own module names and exact source lines
to hunt for in decompiled code.

### 1.1 Module names and diagnostics (strings, `strings -t x`, VAs)

| VA (`fileoff + 0x400000`) | Content | Meaning |
|---|---|---|
| `0xcaeb3c` | `src/cosim/i2c.rs` | the part-side (co-sim) I²C bridge |
| `0xcb241d` | `src/periph/serial/i2c.rs` | **the emulated I²C controller** — prime suspect |
| `0xcb9253` | `esp_emu::cosim::i2c` (type path, followed by `out`, `spi`, `uart`) | module/type names survive as type paths |
| `0xcbd4e6`… | `command list ended without STOP/END; completing best-effort`, `transfer started while the last one was still out; the last one is dropped`, `the device answering this port changed with a transfer in flight; it is dropped` | controller-model diagnostics: it *tolerates* HAL command lists and completes them best-effort |
| `0xca7f01` | `--i2c-dev` help: "Puts a register-file device … beside the built-in EEPROM at 0x50" | the no-board bus has a built-in EEPROM at 0x50 |
| `0xa1xxxx / 0xb0xxxx / 0xc0xxxx` | `__call_rom_i2c_writeReg`, `__call_i2cmst_reg_init`, … (three copies) | per-chip ROM-function callout tables (C3 / S3 / other); the host intercepts known ROM functions |

### 1.2 Panic `Location` entries (decoded from `.data.rel.ro`)

A Rust `&'static Location` is a `{ ptr, len, line, col }` struct in data. Each
one pins an exact line in the emulator's source and, more usefully, its
*address is referenced by the compiled code* that can panic there:

| Location VA | Source line | Referenced by |
|---|---|---|
| `0x11a2e38` | `src/cosim/i2c.rs:58:37` | (no adrp/add hit in the text scan; reachable via Ghidra data xrefs) |
| `0x11aa040` | `src/periph/serial/i2c.rs:975:40` | (no text xref; Ghidra data xrefs) |
| `0x11aa058` | `src/periph/serial/i2c.rs:983:53` | cluster B |
| `0x11aa070` | `src/periph/serial/i2c.rs:980:47` | (no text xref; Ghidra data xrefs) |
| `0x11aa088` | `src/periph/serial/i2c.rs:670:33` | (no text xref; Ghidra data xrefs) |
| `0x11aa0a0` | `src/periph/serial/i2c.rs:671:34` | cluster A (179 refs) |
| `0x11aa0b8` | `src/periph/serial/i2c.rs:558:10` | cluster B |
| `0x11b8fa0` | `src/periph/serial/i2c.rs:185:22` | single site `0x870d08` |
| `0x11b8fb8` | `src/periph/serial/i2c.rs:191:26` | — |

### 1.3 Code clusters (adrp/add xref scan over `objdump -d`)

| Cluster | Range | Refers to | Likely contents |
|---|---|---|---|
| A | `0x4d1200`–`0x514000` | Location `i2c.rs:671:34`, 179 hits | the register-file / memory-mapped read-write dispatch (a `Location` reused across an indexer or macro) |
| B | `0x6a55cc`–`0x6ab3f8` | Locations `i2c.rs:983:53` and `:558:10` | transaction/command processing near the STOP/END diagnostics |
| C | `0x870d08` (single) | Location `i2c.rs:185:22` | a small routine (init or reset path) |

These are instruction addresses of the referencing `add` instructions, found
without Ghidra (script in §3.2). Ghidra turns them into named functions in
minutes.

---

## 2. The contract esp-hal imposes (the other half of the pair)

The vendored HAL source is on disk and pins down *which register reads* produce
the two observed errors (`repro.md` §3):

`esp-hal-1.2.2/src/i2c/master/low_level/mod.rs`:

- `wait_for_completion_blocking` (l. 890) polls the `TxComplete | EndDetect`
  interrupt events, then `check_all_commands_done_blocking` (l. 977) →
  `all_commands_done` (l. 898, reads `comd_iter()` done bits) →
  `check_errors` (l. 1004).
- `check_errors` (l. 1004):
  1. if `INT_RAW.NACK` set → `AcknowledgeCheckFailed(estimate_ack_failed_reason(..))`
  2. else if `INT_RAW.TRANS_COMPLETE` set **and** `SR.RESP_REC` clear →
     `AcknowledgeCheckFailed(Data)` (l. 987–989)

`esp-metadata-generated-0.4.0/src/_generated_esp32c3.rs:163`:
`i2c_master.can_estimate_nack_reason => false` for ESP32-C3, so
`estimate_ack_failed_reason` always returns **`Unknown`** on C3
(`low_level/mod.rs:1904–1916`; the `FIFO_ST.TXFIFO_RADDR` estimation branch is
compiled out).

Register facts from the vendored PAC (`esp32c3-0.34.0/src/i2c0/`):

| Register | Offset from `0x60004000` | Bit | Field |
|---|---|---|---|
| `I2C_SR_REG` | `0x08` | 0 | `RESP_REC` |
| `I2C_FIFO_ST_REG` | `0x14` | — | `TXFIFO_RADDR` … |
| `I2C_INT_RAW_REG` | `0x20` | 3 | `END_DETECT` |
| | | 7 | `TRANS_COMPLETE` |
| | | 10 | `NACK` |

### 2.1 Deduction — the two observed errors are two different HAL branches

| Observed (`repro.md` §3) | HAL branch taken | What the emulator must have done |
|---|---|---|
| 0x44 empty write → `…Failed(Unknown)` | branch 1: `INT_RAW.NACK` set (estimator off on C3 → `Unknown`) | nothing answers; address phase NACKed. **Correct behaviour.** |
| 0x50 (and the SHT31 part) — every shape → `…Failed(Data)` | branch 2: `INT_RAW.NACK` clear, `INT_RAW.TRANS_COMPLETE` set, `SR.RESP_REC` **clear** at poll time | the transaction **ran to completion** (`TRANS_COMPLETE` fired; a timeout would have been `Error::Timeout` instead), but the model left `SR.RESP_REC` (SR reg `0x08`, bit 0) clear — or cleared it — before the HAL polled |

So the mystery "the transaction dies in the data phase" of `repro.md` §3 is
probably not a dead transaction at all: the model most likely drives the whole
transfer and raises `TRANS_COMPLETE`, and the failure is exactly the
`TRANS_COMPLETE && !RESP_REC` combination esp-hal treats as a data NACK. The
candidate defects in `esp_emu::periph::serial::i2c`:

- **(a)** the model never sets `SR.RESP_REC` at all;
- **(b)** it sets it but clears it on `END`/`STOP` processing, before the HAL's
  final `check_errors` poll (note cluster B sits right where the
  `command list ended without STOP/END; completing best-effort` diagnostics
  live);
- **(c)** it completes all commands (done bits) and fires `TRANS_COMPLETE`
  with different ordering than silicon, so the HAL samples `SR` mid-sequencing.

Which of the three it is, is exactly what the Ghidra step answers.

---

## 3. Method used so far (reproducible)

Artifacts live in `build/rea/` (gitignored, like the other fixtures).

### 3.1 Disassembly and strings

```sh
mkdir -p build/rea
strings -t x -n 4 ~/.local/bin/esp-emu > build/rea/emu-strings.txt
objdump -d --no-show-raw-insn ~/.local/bin/esp-emu > build/rea/emu.asm
sha256sum ~/.local/bin/esp-emu > build/rea/esp-emu.sha256
```

VA mapping (verified against `readelf -l`): the first `LOAD` covers file
offsets `0x0..0xd8b521` at vaddr `0x400000`, so **string VA = string file
offset + 0x400000**; the second `LOAD` maps `0xd8c3f8 → 0x119c3f8`
(`+0x410000`), which holds `.data.rel.ro` (the panic `Location`s).

### 3.2 Helper 1: decode panic `Location` structs

```python
import struct
data = open('/home/pi/.local/bin/esp-emu','rb').read()
def rd(va, n): return data[va-0x400000:va-0x400000+n]
for slot in (0x11a2e38, 0x11aa040, 0x11aa058, 0x11aa070,
             0x11aa088, 0x11aa0a0, 0x11aa0b8, 0x11b8fa0, 0x11b8fb8):
    ptr, ln, line, col = struct.unpack('<QQII', rd(slot, 24))
    s = data[ptr-0x400000:ptr-0x400000+ln].decode('utf8', 'replace')
    print(f'{slot:#x}: {s}:{line}:{col}')
```

### 3.3 Helper 2: adrp/add xref scan (find code that references a data VA)

aarch64 materialises addresses as `adrp xN, <page>` + `add xN, xN, #<off>`;
the pair must be reconstructed:

```python
import re
WANT = {(0x11aa000, '0xa0'): 'Location i2c.rs:671:34',   # extend as needed
        (0x11aa000, '0x58'): 'Location i2c.rs:983:53',
        (0x11aa000, '0x70'): 'Location i2c.rs:980:47',
        (0x11aa000, '0xb8'): 'Location i2c.rs:558:10',
        (0x11b8000, '0xa0'): 'Location i2c.rs:185:22'}
adrp = re.compile(r'^\s*([0-9a-f]+):\s+adrp\s+(x\d+), (0x[0-9a-f]+)')
add  = re.compile(r'^\s*([0-9a-f]+):\s+add\s+(x\d+), (x\d+), #(0x[0-9a-f]+)')
base, hits = {}, []
for line in open('build/rea/emu.asm'):
    m = adrp.match(line)
    if m: base[m.group(2)] = int(m.group(3), 16); continue
    m = add.match(line)
    if m and m.group(3) in base:
        key = (base[m.group(3)], m.group(4))
        if key in WANT: hits.append((int(m.group(1),16), WANT[key]))
for a, n in sorted(hits): print(f'{a:#x}  {n}')
```

---

## 4. What Ghidra must be pointed at

The disassembly (`build/rea/emu.asm`, ~2.2 M instructions) already bounds the
interesting code; Ghidra's job is function recovery, cross-references and
decompilation inside these bounds:

1. **Cluster B functions** (`0x6a55cc`–`0x6ab3f8`): the command-list /
   transaction-completion path. Questions to answer per function:
   - does anything write `SR.RESP_REC` (offset `0x08`, bit 0 of the modelled
     register set)?
   - where is `INT_RAW.TRANS_COMPLETE` (offset `0x20`, bit 7) set, relative to
     the `command list ended without STOP/END; completing best-effort`
     diagnostic?
2. **Cluster A functions** (`0x4d1200`–`0x514000`): the MMIO read/write
   dispatch. Confirm the controller model's register block layout matches
   `0x08`/`0x14`/`0x20` offsets, and whether `RESP_REC` is ever stored.
3. **Anchor by constants**: guest MMIO addresses `0x60004000` (C3 `I2C0` base),
   `0x60004008`, `0x60004014`, `0x60004020` (and the other chips' bases for the
   neighbouring copies of the model). Search Immediate values in Ghidra.
4. **Anchor by ROM callouts**: the `__call_rom_i2cmst_reg_init` etc. tables
   (§1.1) — the C3 copy is the one wired to the C3 chip config.
5. **Anchor by the cosim side** (`src/cosim/i2c.rs:58`): the part bridge
   receives `BusRequest{I2c{addr, write, read_len}}` and replies `ack: bool`
   (`repro.md` §1); check whether the *controller model* ever consumes the
   part's `ack` result into `SR`/`INT_RAW` or only into a log line — the
   co-sim log shows `ack:true` reaching the host while the controller still
   reports NACK, so the seam between the two is part of the answer.

### 4.1 Practical Ghidra recipe

Requires a JDK (21+) — `brew install --cask temurin@21`, then Ghidra from
`ghidra-sre.org` (aarch64 Linux build) or `brew install ghidra`. Then:

```sh
analyzeHeadless build/rea/rea-project rea \
    -import ~/.local/bin/esp-emu \
    -processor AARCH64:LE:64:v8A \
    -analysisTimeoutPerFile 1800
# then in the GUI (build/rea/rea-project):
#   - search Immediates for 0x60004000 / 0x60004008 / 0x60004020
#   - show Data Xrefs to 0x11aa0a0, 0x11aa058, 0x11b8fa0 (the Locations of §1.2)
#   - decompile the containing functions of clusters A/B/C (§1.3)
```

Ghidra's version tracker is overkill here; a straight code/diff read of the
decompiled functions against the expectations in §2 is enough. If Ghidra is
unavailable, radare2 (`r2 -A`) over the same clusters and a capstone script
extending §3.3 reach the same answers, just slower.

---

## 5. Done when

- The decompiled `esp_emu::periph/serial/i2c.rs` functions behind clusters A/B
  show which of (a)/(b)/(c) in §2.1 is real, with the setting/clearing of
  `SR.RESP_REC` and `INT_RAW.TRANS_COMPLETE` pinned to instructions.
- A one-paragraph verdict lands in `repro.md` §5 (replacing "What remains is
  the pair …") and §7 option 1 gets a concrete bug location.
- Rerun `just i2cscanner-test` — it prints `Found device at address 0x50` and
  exits 0 once the pairing is fixed (`repro.md` §8).

## 6. Evidence on disk

| Path | What |
|---|---|
| `build/rea/emu.asm` | full `objdump -d` of the binary (2 214 679 lines) |
| `build/rea/emu-strings.txt` | `strings -t x -n 4` with file offsets |
| `build/rea/esp-emu.sha256` | identity of the analysed binary |
| `repro.md` §3/§5 | the observed failure this work explains |
| `testsht31.md` §1a | the same gate from the SHT31 fixture's side |
| `~/.cargo/registry/src/…/esp-hal-1.2.2/src/i2c/master/low_level/mod.rs` | the HAL branch logic quoted in §2 |
| `~/.cargo/registry/src/…/esp32c3-0.34.0/src/i2c0/` | the register/bit map quoted in §2 |

Cross-check note: the emulator is developed publicly (the binary's banner
points at `github.com/espressif/esp-emulator`); once the binary-level verdict
is in, compare against the upstream `src/periph/serial/i2c.rs` at the tag the
banner reports (`v1.0.0`) — the binary is the CI-built release of that
project, so the two should agree; any divergence is itself a finding.
