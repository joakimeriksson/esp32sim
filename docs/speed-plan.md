# Plan: making the emulator faster

Every number here was measured in this repo with `tools/bench.py` (interleaved rounds, best +
median wall time, guest instruction counts cross-checked) or `sample(1)` against a normal run.
The negative results are listed too, so nobody re-spends the time.

**Status:** Phase 1 (block interpreter) and the first cut of Phase 2 (AArch64 JIT) have landed; Phase 0's NEON work and the JIT's inline memory path are open. The wasm backend compiles bounded regions and the PIE instructions TinyDraw's tile kernels use (#63). pocket-tank now measures language-model inference, the PIE path that remains slow; its baseline is below.

## Where we are (M-series Mac, `lto = "fat"`, `tools/bench.py`)

| workload | before blocks (`288a91e`) | block interpreter | **JIT** | vs real time now |
| --- | --- | --- | --- | --- |
| energy panel (LVGL, mostly idle) | 77 Minsn/s | 104 | **154** | ~4× |
| panel + WiFi + HTTPS (20 s) | 9.1 s wall | 8.6 s | **4.6 s** | 4.3× |
| SID player (panel, tune playing) | 93 Minsn/s | 133 | **241** | 2.7× |
| autopling detector (PIE-heavy) | 63 Minsn/s | 78 | **102** | ~real time |
| Atech synth (5 s scenario) | 113 Minsn/s | 153 | **233** | — |
| Atech ST7735 full redraw | needs ~240 Minsn/s | | | at the threshold |

Profile of the SID workload with the JIT (shares of total run time):

| ~36 % | generated code (ALU, branches, register access, inline TLB loads/stores, prologue/epilogue) |
| ~29 % | block dispatch: `run_block` inlined into `step_blocks` — cache lookup and validation, resume check, budget and timer bound, the call itself |
| ~9 %  | slow-path memory helpers (peripheral registers, TLB misses) |
| ~6 %  | interpreter fallbacks (calls/returns/`entry`, special registers, FPU/PIE/MAC16) |
| ~4 %  | peripheral models |
| ~22 % | unattributed leaf frames (inlined code without symbols) |

The dispatch share is now the striking number: every block returns to Rust, which re-checks
interrupts, re-validates the cache entry, re-derives the timer bound and re-enters through a
12-register prologue. **Direct block chaining** — jumping from one compiled block to the
next when the successor is compiled and no check is due, with the checks folded into the
generated code — is the classic answer and the next structural step.

Before blocks the per-instruction scaffolding was ~35 % and **no single piece of it was
removable** — each ablated to ≈0 %. Executing blocks reclaimed it; the JIT then removed the
dispatch and operand unpacking. What remains is memory access.

## Language-model inference on PIE: pocket-tank

`tools/fetch-pocket-tank.sh` (mediacutlet/pocket-tank, MIT; a 4-bit transformer on the Waveshare
AMOLED-1.8 board) is the workload for this path. Measured on 2026-09-12 on an M-series Mac; commands in `tools/browser-benchmark/README.md`.

| pocket-tank, main `de7c6d7` | Minsn/s | real time |
| --- | --- | --- |
| native, `tools/bench.py`, 20 guest s, best of 5 | 170.7 | 0.47 |
| Node, `tools/wasm-test.mjs` loop, 30 guest s, median of 3 | 105.2 | 0.31 |
| headless Chrome 152, `run-pairs.py`, 30 guest s, one screening pair | 107.5 | 0.32 |
| the same wasm with `-C target-feature=+simd128` | Node 108.8 against 108.7 for its plain build; Chrome 0.32 | no measurable change |

Every run executes exactly 10,073,833,775 instructions with 15 model decisions, so the total is
pinned in `workloads.json`. The native row's 20 guest seconds include the boot, which runs denser
than the steady state: 363 M instructions per emulated second there against 336 M averaged over 30
seconds, which is why its Minsn/s and real-time columns relate differently from the others. The interactive page in a visible Chrome tab ran at 0.23 real time:
drawing and pacing there cost extra on top of the headless harness.

The guest asks for 336 M instructions per emulated second across both cores, more than the
silicon does: SPI2 transfers finish instantly and flash-cache and PSRAM reads cost nothing extra, so
the firmware renders at 62 fps instead of 25–30 and decodes 24 tokens/s instead of 12.

Where the time goes (instruction shares from `--profile-blocks`; host shares from `sample(1)` and
a Node `--cpu-prof` of the wasm build):

| guest instructions | share |
| --- | --- |
| two 4 KB pages of the 4-bit matmul kernel | 65 % |
| `__divsf3` / `memcpy` | 3 % / 2 % |

| wasm host time | share |
| --- | --- |
| PIE interpreter (`pie::exec`, operand extraction, `Ops::get`, `ld`) | 29 % |
| scheduler loop between blocks (`run_unmodeled`) | 27 % |
| all generated code | 19 % |
| bus reads, mostly PIE weight loads | 8 % |
| single-instruction interpreter | 6 % |
| display transaction | 0.5 % |

The kernel's instructions are `ee.vld.128.ip`, `ee.vmulas.s8.accx[.ld.ip]` and `ee.zero.accx`. The
wasm backend emits only the load (and only through the fast mapping); the AArch64 backend emits no
PIE; every PIE instruction re-extracts its operands from the word and loads word by word through
the bus.

Plan, in order: packed PIE operands and bulk loads in the interpreter (all hosts), then those
instructions in the wasm backend, then NEON in the AArch64 backend or direct block chaining,
whichever the next profile shows larger.

### Packed PIE operands and bulk loads

The first step of that plan: `pie::pack` extracts the hot instructions' operands once at decode,
`exec_packed` runs them from the packed fields with byte-array dot products, and 128-bit loads
use `Bus::read_bulk`. The table executor stays the reference (randomized equivalence tests), and
pocket-tank still executes exactly 10,073,833,775 instructions with the same 15 decisions.

| pocket-tank | main | packed PIE | change |
| --- | --- | --- | --- |
| native, `tools/bench.py`, 20 guest s, best of 5 | 169.5 Minsn/s, 0.47 real time | 258.8 Minsn/s, 0.71 | 1.53× |
| Node, 30 guest s, median of 3 | 107.2 Minsn/s, 0.32 | 163.9 Minsn/s, 0.49 | 1.53× |
| headless Chrome 152, `run-pairs.py`, 3 matched pairs, median wall | 90.4 s, 0.33 | 60.3 s, 0.50 | 33.3 % less wall time |
| the packed build with `+simd128`, Node | | 164.8 Minsn/s | +0.5 %, within noise |

Wasm host time afterwards: the scheduler loop between blocks 37 %, generated code 21 %, the PIE
path about 18 % (packed execution 9 %, bulk loads 4.5 %, instructions still on the table 2.8 %,
word reads 1.9 %), and `exec_insn` dispatch into it 8 %. The kernel's instructions still leave
generated code for a helper call, and a helper ends a region at its next chunk head, so emitting
them in the wasm backend should shrink the dispatch share as well as the PIE share.

### The dot product in the wasm backend

The wasm backend now emits pocket-tank's kernel: signed 8- and 16-bit multiply-accumulate into
ACCX with and without its load, the ACCX reset, and the RUR of ACCX that follows each dot
product. RUR mattered as much as the vector code: a block with any instruction the backend
does not emit stays interpreted, so without it none of the kernel's blocks compiled and the
emission gained nothing. With it, the model core runs 92 % of its instructions compiled, up
from 70 %.

| pocket-tank, 30 guest s | packed PIE | + wasm dot product | change |
| --- | --- | --- | --- |
| Node, median of 3 | 162.8 Minsn/s, 0.49 | 199.5 Minsn/s, 0.59 | 1.23× |
| headless Chrome 152, 3 matched pairs, median wall | 63.3 s, 0.47 | 50.2 s, 0.60 | 20.7 % less wall time |

Against main that is 1.88× under Node and 0.33 to 0.60 real time in Chrome, with the pinned
instruction total unchanged. Still interpreted on the model core: signed division (3.6 % of its
instructions) and a dequantisation block needing SAR-byte writes, saturating subtract and
byte shifts (2.8 %).

### Division in wasm, a direct PIE helper natively

Two follow-ups. The wasm backend emits QUOU, QUOS, REMU and REMS inline, re-executing only a zero
divisor or QUOS of INT_MIN by -1 in the interpreter. Native blocks call `pie::exec` through their
own helper instead of `exec_insn`'s dispatch. Timed against the previous step on a quiet machine:

| pocket-tank | before | after | change |
| --- | --- | --- | --- |
| native, `bench.py` 5 interleaved rounds of 20 guest s, median | 28.65 s, 0.70 | 27.76 s, 0.72 | 1.03× |
| Node, 3 rounds of 30 guest s, median | 205.1 Minsn/s, 0.61 | 210.8 Minsn/s, 0.63 | 1.03× |
| headless Chrome 153, 3 matched pairs, median wall | 51.2 s, 0.59 | 48.5 s, 0.62 | 5.3 % less wall time (pairs 1.4, 5.3, 2.2 %) |

Both are small because both costs were small: division was 3.6 % of the model core's instructions
and the dispatch a few percent of native time. Where the work stands against main:

| pocket-tank, real time | main | now | change |
| --- | --- | --- | --- |
| native, M-series Mac | 0.47 | 0.72 | 1.53× |
| Node | 0.31 | 0.63 | 2.0× |
| headless Chrome | 0.32 | 0.62 | 1.9× |

### Guest work: the panel's transfers take no time

Host speed is half of real time; the other half is how much the guest does per emulated second.
SPI2 transfers complete at the next device tick, so pocket-tank flushes a 368x448 frame in no
emulated time and renders at 62.5 fps, where the board manages 25 to 30. An experiment (branch
`exp/qspi-transfer-time`, not merged) holds completion for the transfer's clock time at the
SH8601 driver's settings, 40 MHz with quad data. Natively over 30 guest seconds:

| pocket-tank, packed PIE | transfers instant | 40 MHz quad transfer time |
| --- | --- | --- |
| guest instructions per emulated second | 335.8 M (core0 148 M) | 287.5 M (core0 100 M) |
| render loop | 62.5 fps, flush 6.1 ms | 41 fps, flush 19.7 ms |
| model | 24.3 tok/s | 24.3 tok/s |
| real time on the same host | 0.77 | 0.87 |

Modelling transfer time is worth about 14 % on this board on any host. The remaining gap to the
board's frame rate and its 12 tok/s is memory latency (flash cache, PSRAM), which the fast paths
do not charge.

Where the next factors are: the dot-product instructions in the wasm backend, estimated at 1.2–1.4×,
measured 1.23× under Node (above); not yet measured, cheaper scheduler rounds or direct block chaining about
1.2×. With transfer time, headless Chrome would go from 0.50 to roughly 0.85–1.0 real time and the
native build past real time.

## Phase 0 — small, independent, do anytime

- **NEON for PIE** (`pie.rs`): every `ee.*` op runs as a scalar loop over `u128` lanes with
  shift/mask extraction and per-lane saturation; the detector spends ~20 % of its time there.
  `core::arch::aarch64` intrinsics behind a scalar fallback (the fallback stays the reference
  and the unit-test oracle). Expected +10–15 % on the detector only. ~2–3 days.
- **Shrink `CacheEntry`** (32 B × 64 K = 2 MiB copied by value on every hit). Measured: size
  32 K/64 K/128 K makes no throughput difference, so halving the table is free memory; packing
  `Insn` below 20 B may help cache locality but must be measured, not assumed.
- **Nothing else at this level.** Already measured as free or rejected: `ccompare` loop,
  per-instruction debug hooks (now bloom-gated), `target-cpu=native`, bigger icache, raw
  pointers in TLB entries, 2048-cycle idle steps (changed the Atech WAV — the regression bar
  is bit-identical output).

## Phase 1 — basic-block interpreter — DONE

**Measured: 1.24–1.42× across the workloads, every regression output bit-identical**, landed
in `xtensa-lx7/src/block.rs`. It is host-API-free like the rest of the core, so it carries over
to the WASM build unchanged. Two things the implementation taught that the design below did not
anticipate: IRAM/DRAM aliasing put `.dram0.data` in the same 4 KiB version page as the hottest
ISR code (version pages are now 256 bytes), and a block cut by the scheduling quantum has to
resume in place rather than start a new block at the cut, or the cache fills with fragments.

Design as built:

- **Block = straight-line run of decoded instructions** ending at a control transfer (`j`,
  `jx`, `call*`, `ret*`, branches, `loop*`), a `waiti`/`rsil`/`syscall`-class instruction, or
  a block-size cap (~32). Stored as a pre-resolved array: handler + unpacked operands per
  instruction (no re-decode, no operand extraction at run time).
- **Once per block instead of once per instruction**: the interrupt check, the decode-cache
  probe, `ccount` advance (add `block.len`), `insn_count`, and the stub/probe bloom test.
  The window-overflow check stays per-instruction where a `max_ar` demands it — it is the one
  check that measurably matters (removing it broke the guest, ablation B).
- **Timer precision is preserved** by bounding a block's length to the cycles remaining until
  the next `cycles_until_timer()` deadline — same trick the lazy device tick already uses, so
  `ccompare`/systimer alarms still land on the exact instruction.
- **Invalidation is already built**: a block caches the `page_ver` values of the (at most two)
  256-byte pages it spans; validation is two indexed loads. MMU remaps bump all flash/PSRAM
  versions. Self-modifying code, the SPI flash
  controller and the image loaders all bump versions today (`note_written`), and MMU changes
  invalidate the TLB. Zero-overhead loops (`lbeg`/`lend`/`lcount`) fall out naturally: the
  loop body is a block, the loop-back edge re-enters it.
- **Interpreter first, no codegen.** The block cache, discovery, guards and invalidation are
  exactly the infrastructure a later JIT reuses; only the "execute" half differs.
- Acceptance: Atech WAV bit-identical, SID capture sample-identical after alignment, full
  regression sweep, and `tools/bench.py` on the three standard workloads.

Landed at SID 133 Minsn/s, detector 78, panel 104 — the estimate held for SID and the panel;
the detector gained less because its time is in PIE lanes and loads/stores, not scaffolding.

## Phase 2 — JIT — first cut DONE (AArch64)

**Measured on top of the block interpreter: 1.23–1.46×; cumulative since `288a91e`: SID 2.07×,
Atech 1.86×, panel 1.66×, detector 1.60× — every output bit-identical with `--no-jit`.**
`xtensa-lx7/src/jit/`: a hand-written encoder (`a64.rs`, checked against clang) and a block
compiler (`mod.rs`) that inlines ALU/shift/move/compare ops and all branches, calls bus helpers
for loads and stores, and calls `exec_insn` for everything else. Native went first after all:
it is the machine the work is measured on, and the block model, invalidation and the helper
protocol are what a wasm backend will reuse.

The inline TLB fast path followed (SID 200 → 241, panel 132 → 154, Atech 212 → 233; the
detector is unchanged because its memory traffic is inside PIE instructions, which are still
fallbacks): generated code probes the shared `TlbEntry` table directly and calls the helper
only on a miss, a non-writable page, or a store whose version bump would touch a page edge.

Next inside the JIT, in order of measured value:
0. **Direct block chaining** — see the profile above; ~29 % of time is between blocks.
1. **Register caching within a block** — every guest register access is index arithmetic plus
   a memory access; keeping the most-used registers in host registers between instructions
   removes most of it. Needs spills before helpers and at exits.
2. **Inline the common fallbacks**: `call8`/`entry`/`retw` are the most frequent helper calls.
3. **PIE in generated code or NEON in the interpreter** — the detector's remaining cost.

## Phase 2b — the wasm backend (in progress)

One block IR, two backends — the native one exists; for the browser build:

- **landed checkpoint**: the first receipt-priced SRAM block emitter now has a shared-memory
  browser handoff. The worker caches generated modules and commits only complete 64-instruction
  single-core scheduler quanta; unsupported instructions, timer crossings and failed guards fall
  back. Shared-memory modules check PC once at entry and then fall through the straight-line
  sequence. CI executes the real wasm ABI under Node. This is integration coverage, not a
  whole-firmware speed result.

- **wasm backend**: emit a wasm module per batch of hot blocks. Measured with a spike:
  compile+instantiate costs **~0.3 µs per block** when batched (64+ blocks/module) and
  **2.5 ns per call** into a generated function under V8 — so translation pays for itself
  within a handful of executions. Generated code is straight-line ALU + word memory, the
  shape where wasm runs at 52–68 % of native; `return_call` (shipped tail calls) gives the
  dispatch chain Rust cannot express, and SIMD128 maps onto the PIE lanes.
- **aarch64 backend**: a few hundred lines of direct emission (no Cranelift — a large
  dependency, and useless for the wasm side). Native SID is already at real time, which is
  why the browser backend goes first: at ~0.45× real time today it is where a JIT actually
  changes what is possible.
- New floor after a JIT: device ticks, bounds-checked memory, and the window-overflow check.

Effort: ~a month total, roughly half per backend once the IR exists.

## Phase 3 — perception, not emulation

The browser session runs ~0.82× real time while headless runs 0.99×, because 460 KB frames are
pushed 50×/s whether or not anything changed. Push at 25 Hz or only dirty rows (the RGB engine
can track lines). Half a day, and it is the cheapest "the SID page stopped stuttering" per
hour spent of anything on this list.

## Rejected, with the numbers

- **One host thread per core**: core 1 is 7–8 % of executed instructions on the workloads that
  matter, so the ceiling is ~1.1× — and it forfeits deterministic, bit-identical output, which
  is the regression bar. Not worth it.
- **Skipping guest busy-loops by pattern**: the mbedTLS accelerator polls are real firmware
  behaviour; a faster interpreter runs them faster, and special-casing them risks the
  plausible-wrong-answer failures the crypto section of decisions.md documents.
- Everything in the Phase 0 "already measured" list above.

## Method rules (learned the expensive way)

1. Benchmark **interleaved** (`tools/bench.py`), never A-then-B — background load drifts ~10 %
   over minutes and sequential comparisons harvested it as fake wins twice this session.
2. `--profile` reports **guest** PCs and disables idle-skipping; for emulator-side cost use
   `sample <pid>` and confirm with an ablation build.
3. `pgrep esp32sim` before benchmarking — leftover runs at 100 % CPU look like regressions.
4. The bar for landing anything: Atech WAV and TFT bit-identical, panel PNG identical,
   decoder-vs-objdump, hello_world, autopling, WPA2 join, HTTPS fetch, unit tests.
