> Historical worker note, preserved before browser results were available. Predictions and queued status below are historical; see [closeout](../README.md) and [measured results](../results.json).

# Worker `host` (x5): DWARF-free artifact, dispatch micro-diet, TinyDraw call edges

Worktree `$X-host`, branch `x5/host`, base `x5/base` = 8cbe0da4.
Base artifact `wasm/base.wasm` sha256 `0a5e09eb1d1e`, differential base 81,626 cases
(`results/diff-base.log`). Node is never a benchmark here; all timing comes from the M3.

## host-s1 — production module without DWARF (EX120)

Commit `5c8d3237` "wasm: build the production module without DWARF (EX120)": two lines in
`tools/wasm-rustflags.sh` defaulting `CARGO_PROFILE_RELEASE_DEBUG=0` next to the RUSTFLAGS
default (historically overridable via `${CARGO_PROFILE_RELEASE_DEBUG-0}`: unset selected 0 but an empty override was preserved, unlike the RUSTFLAGS fallback). The review fix changes this to `${CARGO_PROFILE_RELEASE_DEBUG:-0}`, so unset and empty now select 0; nonempty overrides remain explicit. The measured artifact is built by
`JOBS=4 gate.sh host-s1 CARGO_PROFILE_RELEASE_DEBUG=0` (build.sh does not source that file).

Artifact `wasm/host-s1.wasm` sha256 `697b164f7870`.

### Section table (base vs host-s1)

| section | base bytes | host-s1 bytes | identical |
|---|---:|---:|---|
| 1 type | 464 | 464 | no |
| 2 import | 62 | 62 | yes |
| 3 function | 1,363 | 1,363 | no |
| 4 table | 5 | 5 | yes |
| 5 memory | 3 | 3 | yes |
| 6 global | 9 | 9 | yes |
| 7 export | 1,558 | 1,558 | yes |
| 9 element | 962 | 962 | no |
| 10 code | 1,824,791 | 1,824,791 | no |
| 11 data | 100,835 | 100,875 | no |
| custom `.debug_abbrev` | 10,254 | — | dropped |
| custom `.debug_info` | 3,656,893 | — | dropped |
| custom `.debug_ranges` | 837,254 | — | dropped |
| custom `.debug_str` | 3,647,079 | — | dropped |
| custom `.debug_line` | 1,902,030 | — | dropped |
| custom `name` | 140,546 | 139,987 | no (kept, `cpu-profile` still symbolizes) |
| custom `producers` | 77 | 77 | yes |
| custom `target_features` | 148 | 148 | yes |
| **module** | **12,124,399** | **2,070,349** | −10,054,050 (−82.9%) |

### Code identity: the code section is *not* identical, and here is exactly why

`compare.mjs` by function index is misleading here, because `-C debuginfo=0` changes every local
crate's v0 mangling disambiguator (`_RNvNtNtCs5F2i29bsihg_10xtensa_lx7…` vs `…Cs4dmMuLoS5VH_…`),
which moves symbols in the link order. Two controls settle it:

1. **Worktree control.** `wasm/host-rb.wasm` (sha256 `1b8695f00e6f`) is my worktree at the same
   commit with the stock recipe: **every section except `.debug_info`/`.debug_str` is byte
   identical to `base.wasm`, all 1,361 bodies identical** (`compare.mjs`: `changedBodies: 0`).
   So the build is reproducible across worktrees and the only path-dependent bytes are DWARF.
2. **Matched by name, not index** (`notes/host-sections.mjs`, `notes/host-operands.mjs`, run over
   host-rb vs host-s1, normalizing the `Cs<hash>_` disambiguator): the 1,361 bodies carry 1,322
   distinct normalized names per module (39 generic instantiations collide once the disambiguator
   is erased), 1,293 of those names appear in both, and of those **740 are byte-identical, 553
   differ, 0 change size**. The 29 names that match on neither side are v0 backreferences
   (`B4_`, `B13_`) whose indices shift when a disambiguator changes length — the same functions
   spelled differently. All 7,777 differing bytes sit in an immediate: **5,606 `i32.const`,
   1,932 load/store memory offsets, 239 `call` function indices, 0 opcode bytes**. Data section
   +40 bytes, element section renumbered.

So host-s1 is *not* a pure wire-byte change: it is the same instruction stream with different
symbol numbering and static-data addresses. Since that confound is small but real, I added a
control that has no confound at all:

## host-s1b — base.wasm with only the `.debug_*` sections dropped (control for s1)

`node notes/host-strip-dwarf.mjs wasm/base.wasm wasm/host-s1b.wasm` copies every section of the
measured base artifact byte for byte except the five `.debug_*` custom sections. Artifact sha256
`a0c612d25d9f`; 12,124,399 → 2,070,868 bytes (−10,053,510, −82.9%). `compare.mjs` base → s1b:
**0 changed bodies, sections 1-11 and `name`/`producers`/`target_features` all identical**. This
is the artifact the EX120 question is actually about: identical code, 10 MB fewer wire bytes.
No source change; not gated through `gate.sh` (the source is the base source, so the
differential suite is `results/diff-base.log`'s 81,626 cases); exactness re-checked on the
artifact itself.

Reading s1 and s1b together: s1b alone = wire bytes; s1 − s1b = symbol/address renumbering luck.

## host-s3 — dispatch micro-diet, profile first (EX168, final catalog mapping)

Commit `06414498`, artifact `wasm/host-s3.wasm` sha256 `c9c713c6c5d8`. Eight lines in
`xtensa-lx7/src/jit/wasm.rs`.

### The profile (this is what picked the change)

`build.sh host-prof --features cpu-profile`, `SECS=8 node --cpu-prof … exact.mjs`, split with
`notes/host-profsplit.mjs` (exact.mjs runs base first to make the 8 s reference, so the profile
holds two modules; the split takes the candidate window). Self time, candidate window 12.86 s:

| step | self | step | self |
|---|---:|---|---:|
| generated guest code | 45.65% | exec_insn | 1.71% |
| `run_inner` | 20.70% | h_exec | 1.09% |
| `run_block_inner` | 6.12% | **`loop_len_at_head`** | **0.97%** |
| `jit::run` + chain loop | 4.99% | `check_interrupts` | 0.55% |
| `find_block` | 2.52% | `after_round` | 0.48% |
| `run_unmodeled` | 2.42% | periph_write | 0.57% |
| `step_blocks` | 2.38% | gpio irq_sources | 0.50% |

A second build with temporary `#[inline(never)]` splits and the census atomics off
(`notes/host-attribution.patch`, artifact `host-prof2`, window 13.96 s) separates the two big
shells: `run_inner` 10.88%, `run_block_body` 10.22%, `run_block_inner` 5.72%, `jit::run` 5.39%,
`observe_execution` 1.04%, `check_interrupts` 0.65%, `fast_mem` 0.49%, `refresh_irq` 0.48%, and
**no `ccompare_limit` frame at all**.

### What went in (one removal)

`loop_len_at_head` scanned `instructions.zip(pcs).take(loop_prefix)` for the instruction ending at
LEND, loading a `BlockInsn` per step. `pcs[k + 1] == pcs[k] + len(k)` by construction
(`wasm.rs` `queue()` builds `pcs` with exactly that recurrence), so the same instruction is the one
whose *successor PC* is LEND: search `pcs[1..min(loop_prefix + 1, n)]`, then handle the
block-end case with the single last instruction. Same answer, no decoded-instruction loads. Reached about 42M times per 30 guest seconds
on the x2 census (EX168 s3: 192.9M of 234.8M own-module calls stop at the inline
`lcount == 0 || lbeg != pc` test before it).

Measured with the same cpu-profile recipe (`host-prof3`, window 12.82 s): `loop_len_at_head`
**0.124 s → 0.077 s (0.97% → 0.60%)**, everything else within noise (`run_inner` 20.70 → 20.88%,
generated 45.65 → 45.17%). So the effect is ≈0.37 pp of the profiled window, i.e. a small single: predict **+0.2 to +0.5%**, at the edge of what 4 pairs resolve.

### Candidates the profile rejected (all four suggestions in the task list)

- **`check_interrupts` inline pre-test** — 0.55-0.73% self, but this is EX168 s5 exactly, and the
  M3 measured it at **−1.40% (−1.59, −1.22)** (`docs/experiments.md` EX168 row). Repeating a
  measured negative unchanged is not a new experiment.
- **`run_decoded_once`'s three CCOMPARE distances with the `div_ceil` branch** — extracted into an
  `#[inline(never)] fn ccompare_limit` for the attribution build and it never appears in the
  profile at all (< 0.02 s, below one sample bucket). Nothing to remove.
- **`bus.fast_mem()` per dispatch** — `esp32s3/src/bus.rs:688` is one `Option` test plus two
  pointer loads and it inlines; the 0.49% in the split build is the call overhead I introduced,
  not a saving available.
- **`step_blocks` bloom/observe retests** — `step_blocks` 2.20-2.38% and `observe_execution` 1.04%
  are single mask tests per dispatch; the only removal would be a cached `stub|probe` bloom, i.e.
  new state for two loads and an OR. Not worth a slot.
- **Index the dispatched block once** (`run_block_body` fetches `cc.blocks[code]` for the slot, for
  `b` and again inside `loop_len`): built as a probe (`host-s3probe2`) and it made `run_inner`
  *bigger*, 5,554 → 5,616 bytes, so LLVM had already merged those indexes. Dropped, no candidate.
- **Memoizing the LEND answer in the `Block`** (first attempt): a `Cell<(u32, u32)>` field grows
  `Block`, and `run_inner` went 5,554 → 5,625 bytes (`host-s3probe`). The PC-array scan gets the
  same saving with no new state. Dropped.

## host-s2 — EX164 calls-s4 ported to x5/base, measured where calls dominate

Port of x3 `118e9e0f` (branch `x3/calls`, EX164 arm s4). `git diff 5167baad 118e9e0f -- xtensa-lx7`
applied with `git apply -3`: **`wasm_region.rs` and `wasm_instruction.rs` merged cleanly** against
EX173/EX178/EX155/EX169 (the anticipated conflicts did not materialize); the only conflict was in
`wasm_tests/regions.rs`, where x5 added `prev_page_store()` (EX180) on the same tail line, resolved
to `cases + 2 + prev_page_store() + call_edges()`.

In the candidate: two-phase region formation (base closure first, then direct CALL edges from the
leftover chunk/instruction/page budget, one callee at a time, all-or-nothing rollback,
`CALL_INSNS = 24`), the inline windowed RETW that resumes at one of `MAX_RETURNS = 8` static call
sites of the same function, and the 13 directed cases. Nothing else added, nothing dropped; the cap
and the rollback are unchanged.

Not ported: the census arm of that branch (`census::edges`, `note_exit`/`note_edge`, the finer
`ExitKind`, `callee_extent`, the `region_edge` `last_kind` hunk) — `jit/wasm.rs` (+146) and
`census.rs` (+98) of the 852-line patch, all `wasm-jit-profile`-only. That feature does not build on
this base anyway: `crate::census::note_code_page` is called from `xtensa-lx7/src/block.rs:308` and
`xtensa-lx7/src/exec.rs:236` and does not exist in `xtensa-lx7/src/census.rs` (pre-existing, not
from my change), so there are no x5 counters for this mechanism and the x3 counters stand.

**Isolation.** On `x5/host` the port sits on top of s3 (commit `2bdaa26a`). To keep the M3 number
interpretable, the queued artifact is built from `x5/host-s2-solo` = `x5/base` + the same commit
cherry-picked (`5d1bc7a7`, identical 3-file change), so host-s2 measures the call edges alone.

### Counter evidence on this base (pocket-tank, 10 guest s, `notes/host-jitstats.mjs`)

`web/wasm/jit.mjs` already counts every `host_jit_compile`, so this needs no source change;
identical `insns` / frame hash / console hash across the three runs is a further exactness check.

| counter (10 guest s) | base | host-s2 | host-s3 |
|---|---:|---:|---:|
| modules compiled | 4,458 | **4,414** (−1.0%) | 4,458 |
| generated bytes | 41,903,081 | **48,479,967 (+15.7%)** | 41,903,081 |
| live bytes at end | 31,035,270 | 35,874,290 (+15.6%) | 31,035,270 |
| peak generated bytes | 41,027,052 | 48,056,581 (+17.1%) | 41,027,052 |
| peak live modules | 4,364 | 4,374 | 4,364 |
| instructions / frames / frame hash | 3,518,255,932 / 237 / 984d88d7 | same | same |

The mechanism fires on x5/base: fewer, larger modules. x3 measured core-0 generated bytes +34%
(15.9 → 21.3 MB) and 217.40M hops (−3.0%) on its own base; here the whole-module growth is +15.7%,
the cost side of the same trade. host-s3 generates byte-identical code to base, as its mechanism
requires.

### Why this is worth M3 time again

EX164 s4 has one pocket-tank result (−1.81%, 2 pairs) and no TinyDraw result, while EX137 says 82%
of TinyDraw region exits are calls and returns. TinyDraw 3 pairs is the point of this candidate;
pocket-tank 4 pairs is the control that says whether code growth or hop removal dominates.

## Gates

| candidate | commit | artifact sha256 | differential | 6 s exact | 30 s exact | clippy |
|---|---|---|---|---|---|---|
| host-s1 | 5c8d3237 | 697b164f7870 | PASS 81,626 | ok | `EXACT ok` | clean |
| host-s1b | 8cbe0da4 (base source) | a0c612d25d9f | base's 81,626 | ok | `EXACT ok` | n/a |
| host-s3 | 06414498 | c9c713c6c5d8 | PASS 81,626 | ok | `EXACT ok` | clean |
| host-s2 | 5d1bc7a7 (= 2bdaa26a) | 813b79d4bd86 | PASS **81,639** (+13 directed) | ok | `EXACT ok` | clean |

`cargo test --release -p xtensa-lx7` runs 0 tests on this crate (its JIT tests are the wasm32
differential suite). Clippy: the brief's native command is clean on every head. On the wasm32
target, where this code actually compiles, `cargo clippy --target wasm32-unknown-unknown -p
xtensa-lx7 -- -D warnings` fails on a **pre-existing** `clippy::unnecessary_cast` at
`xtensa-lx7/src/jit/wasm_memory.rs:193`, a file no candidate of mine touches; with that one lint
allowed my code is clean.

Predictions: s1 +0 to +2% (the Sep 20 single-pair screen read +3.34% against a ±2.5% noise band);
s1b the same or slightly less, because it cannot win anything from renumbering; s3 +0.2 to +0.5%;
s2 TinyDraw +1 to +3%, pocket-tank −1 to +1%.

## Queued (AGENT=host)

| job | workload | pairs | artifact | commit |
|---|---|---:|---|---|
| host-s1 | pocket-tank | 4 | 697b164f7870 | 5c8d3237 |
| host-s1-td | tinydraw | 3 | 697b164f7870 | 5c8d3237 |
| host-s1b | pocket-tank | 4 | a0c612d25d9f | 8cbe0da4 + strip |
| host-s3 | pocket-tank | 4 | c9c713c6c5d8 | 06414498 |
| host-s3-td | tinydraw | 3 | c9c713c6c5d8 | 06414498 |
| host-s2-td | tinydraw | 3 | 813b79d4bd86 | 5d1bc7a7 |
| host-s2 | pocket-tank | 4 | 813b79d4bd86 | 5d1bc7a7 |

Not queued: the s3+s2 stack (`2bdaa26a`, sha `1a6a6f550019` when built; it passed PASS 81,639 and
`EXACT ok` at 6 s and 30 s before the file was rebuilt for the isolated arm). Rebuild from
`x5/host` if both singles win and the composition is wanted.

## Catalog rows (ready to paste)

| <a id="ex120"></a>EX120 | **Strip WASM debug sections**<br>nodebug; DWARF; wire bytes; startup | Queued (awaiting M3) | Base 8cbe0da4. `[profile.release] debug = 1` puts five `.debug_*` custom sections totaling 10,053,510 bytes into a 12,124,399-byte module whose code section is 1,824,791 bytes; V8 never executes them. **host-s1** (`5c8d3237`, `CARGO_PROFILE_RELEASE_DEBUG=0` defaulted in `tools/wasm-rustflags.sh`, which both `wasm-build.sh` and `wasm-jit-test.sh` source) is 2,070,349 bytes, −82.9%, and keeps the 139,987-byte `name` section that `cpu-profile` symbolizes with. It is **not** code-identical: `-C debuginfo=0` moves every local crate's v0 mangling disambiguator, so 553 of 1,293 name-matched bodies differ in 7,777 bytes — 5,606 `i32.const` immediates, 1,932 load/store offsets, 239 `call` indices, **0 opcode bytes, 0 size changes** — plus a 40-byte data section and a renumbered element section. Two controls prove the claim: a stock rebuild in another worktree is byte-identical to `base.wasm` in every section except `.debug_info`/`.debug_str` (0 changed bodies), and **host-s1b** (`a0c612d25d9f`), base.wasm with only the five `.debug_*` sections dropped, is identical to base in sections 1-11 and in `name`/`producers`/`target_features` (0 changed bodies) at 2,070,868 bytes. s1b therefore measures the wire bytes alone and s1 − s1b measures the renumbering. Both pass 81,626 differential cases and 30 s `EXACT ok`. No timing yet. | Not adopted; queued as host-s1 (pocket-tank 4, TinyDraw 3) and host-s1b (pocket-tank 4). Branch `x5/host`, note `/Users/alice/src/a/esp32sim-x5/notes/host.md`, scripts `notes/host-sections.mjs`, `notes/host-operands.mjs`, `notes/host-strip-dwarf.mjs`. Related: EX121, EX154. |

| EX168 | **Retained-loop length from the PC array**<br>loop_len; dispatch diet; LEND scan | Queued (awaiting M3) | Base 8cbe0da4, profile-led. Node `--cpu-prof` on a `cpu-profile` build (8 guest s, candidate window 12.86 s): generated code 45.65%, `run_inner` 20.70%, `run_block_inner` 6.12%, `jit::run` 4.99%, `find_block` 2.52%, `run_unmodeled` 2.42%, `step_blocks` 2.38%, `exec_insn` 1.71%, `h_exec` 1.09%, `loop_len_at_head` 0.97%, `check_interrupts` 0.55%; a second build with temporary `inline(never)` splits separates `run_inner` 10.88% from `run_block_body` 10.22%. **host-s3** (`06414498`, 8 lines): `loop_len_at_head` found the instruction ending at LEND by scanning decoded instructions; `pcs[k+1] == pcs[k] + len(k)` by construction in `queue()`, so it searches the PC array instead and loads no `BlockInsn` (~42M calls per 30 guest s, the 18% of own-module calls that pass the EX168 s3 inline refusal). Same recipe re-profiled: `loop_len_at_head` 0.124 s → 0.077 s (0.97% → 0.60%), everything else within noise, generated code byte-identical to base (4,458 modules / 41,903,081 bytes in both). Rejected on evidence: the `check_interrupts` pre-test (EX168 s5 measured −1.40%), the CCOMPARE `div_ceil` per dispatch (no profile frame at all), `bus.fast_mem()` (one test and two loads, inlined), the `step_blocks` bloom retest, indexing the dispatched block once (`run_inner` grew 5,554 → 5,616 bytes) and a per-`Block` LEND memo (grew `Block`, `run_inner` 5,554 → 5,625 bytes). 81,626 differential cases, 30 s `EXACT ok`. No timing yet. | Not adopted; queued as host-s3 (pocket-tank 4, TinyDraw 3). Branch `x5/host` (`06414498`), note `notes/host.md`, profile split `notes/host-profsplit.mjs`, attribution patch `notes/host-attribution.patch`. Related: EX168, EX156, EX152. |

**Append to EX164** (the row already exists; this is the x5 follow-up): s4 (`118e9e0f`) ported to x5/base as `5d1bc7a7` (same change as `2bdaa26a` on `x5/host`, isolated from EX168 for measurement); `wasm_region.rs` and `wasm_instruction.rs` three-way merged cleanly against EX173/EX178/EX155/EX169, the only conflict being EX180's `prev_page_store()` in the directed-test tail. The census arm was not ported (x5's `wasm-jit-profile` build is broken independently: `census::note_code_page` is missing but called from `block.rs:308` and `exec.rs:236`). On x5/base the mechanism fires: pocket-tank 10 guest s gives 4,414 compiled modules and 48,479,967 generated bytes against base's 4,458 and 41,903,081 (+15.7% code, −1.0% modules, `notes/host-jitstats.mjs`), with identical instruction total, frame count and frame hash. Differential suite 81,639 cases = base 81,626 + the 13 directed call-edge cases; 30 s `EXACT ok`. Queued as host-s2-td (TinyDraw 3 pairs, the workload EX137 says is 82% calls and returns) and host-s2 (pocket-tank 4 pairs, the control for code growth). No timing yet.

## Next steps and risk

1. **Decide EX120 from the pair s1/s1b, not from s1 alone.** If s1b ≈ 0 and s1 > 0, the win is
   symbol/address renumbering luck, not wire bytes, and adopting the recipe buys download size
   rather than speed. If both win, the recipe commit (`5c8d3237`) is a one-line adoption.
   Note it also strips DWARF from the differential-suite build (`tools/wasm-jit-test.sh` sources
   the same file), so a panic there keeps symbol names but loses line numbers.
2. **s2's risk is code size, and it is now quantified on this base**: +6.6 MB of generated code
   per 10 guest seconds of pocket-tank for the hops it removes. If TinyDraw wins and pocket-tank
   loses, the next knob is the cap (x3 suggested sweeping 8/16/48) — not attempted here because
   the brief fixed it at 24.
3. **s3 is small by construction** (0.35 pp of dispatch). EX168 s5 shows this size of change can
   land anywhere in ±1.4% from code layout alone, so treat a single pair as uninformative.
4. **Unfinished**: x5's `wasm-jit-profile` build does not compile, so nobody in this round can get
   region or hop counters. One missing function (`census::note_code_page`) blocks it.
