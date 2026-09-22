> Historical worker note, preserved before browser results were available. Predictions and queued status below are historical; see [closeout](../README.md) and [measured results](../results.json).

# Worker `tails` — run the quantum's tail inside the region (family `tails`, EX182)

Worktree `/Users/alice/src/a/esp32sim-x5-tails`, branch `x5/tails`, base `x5/base` = 8cbe0da4.
EX id **EX182** (`edges` took EX181 first; both families touch `wasm_region.rs::generate`).
Three stacked candidates, all gated and queued for pocket-tank (4 pairs) and TinyDraw (3).

## Layout recap (`wasm_region.rs::generate`, base)

One WASM `loop` at `loop_depth`, `n` nested `block`s, a `br_table` on `NEXT`, chunk `k`'s code
after the `k`-th `end`. `DONE` is the region's running retired count, local 3 the budget (never
written), local 4 the entry chunk index, `STOP` unused by regions in the base (it only serves
EX156's guarded *block* body).

Credit-short edge in the base (`region_edge`): `DONE + len > budget` with `DIRTY` clear leaves
at the chunk head with `r = budget - DONE > 0` credits unspent. The wrapper chain then pays a
second generated call (own module, EX156 guarded body, `CODE_CUT`) for those r instructions,
and the next quantum a third one to finish the block and chain back into the region.

## Candidates

| name | commit | artifact sha256 | differential cases | 30 s pinned |
|------|--------|-----------------|--------------------|-------------|
| `tails-s1` | `d93694e3` | `634a74eab8cc16bb…` | 82,267 (base 81,626) | `EXACT ok` |
| `tails-s2` | `75426fec` | `ecc1cd0281b0cb65…` | 83,387 | `EXACT ok` |
| `tails-s3` | `0ff99486` | `5a447ffa2e3ff3b4…` | 83,387 | `EXACT ok` |

Every one also passed the 6 s exactness smoke inside `gate.sh`, `cargo test --release -p
xtensa-lx7` (39 native tests) and `cargo clippy -p xtensa-lx7 -p esp-soc --all-targets -D
warnings`. (Clippy for the wasm32 target reports three warnings that are already on `x5/base`:
`wasm_memory.rs:193` and two doc-list lines in the EX180 test comment.)

### tails-s1 — guarded chunk copies for credit-short edges

After the ordinary chunks, `generate` emits a second, *guarded* copy of every chunk longer than
one instruction (EX156's form: one entry label per index, one `STOP <= index` compare per
instruction, `accx_ok` false, no PIE coalescing). A chunk of one instruction gets none: `r < len
= 1` means `r = 0`. At a credit-short edge with `DIRTY` clear and `budget > DONE`, `region_edge`
sets `STOP = budget - DONE`, `NEXT = the copy's dispatch index`, and branches back into the
dispatch loop instead of spilling and returning. The copy's cut stores the first un-retired PC
and returns **`CODE_LEFT`** — a region must not return `CUT`, whose tag the caller reads as a
block index — with the exit site naming the *previous* instruction, so `bus.note_pc` and
`block::note_sequential` see exactly what an own-module cut shows them. Index 0 emits no cut
test: a copy is never entered with `STOP <= entry`.

Exactness: `DONE` is unbiased at a credit-short edge, so `DONE + static index` is the retired
count at every exit of the copy. Reaching the chunk end takes the ordinary edges (including the
LEND backedge through `region_edge`), so hardware loops, the ENTRY re-proof, helper fallbacks,
the DIRTY rule and PIE are the ordinary chunk's code, unchanged. `direct` fallthrough is
disabled inside a copy, which is not followed by the next chunk's code.

### tails-s2 — resume into the region (stacks on s1)

A dispatch with `entry != 0` went straight to `run_block_body`. Now the entry parameter is
packed `(dispatch index | index << 16)`: the prologue masks the low half into `NEXT`, moves the
high half into local 4, and sets `DONE = -index`, `STOP = budget + index`; each guarded copy
starts with a `br_table` on local 4 (zero for a credit-short edge, so s1's entries still land on
index 0 — `region_edge` clears local 4 before branching). One region call now covers the resume
*and* the rest of the quantum: when the credit outlasts the chunk the copy runs to the chunk end
and takes the ordinary edges into the region.

Admission in `run_inner` uses the cached `Hot` facts only: epoch current, `entry < hot.len`, no
probe in the bloom, pages current, and `lcount == 0 || lend - lo > span`. A hardware loop ending
inside the region falls through to the own module, as `r.loops` is not cached in `Hot`; so does a
`CODE_REJECT` (window or coprocessor proof). `Hot` gains `guard`, the dispatch index of chunk k's
copy, from `region::guard_index` — the one definition used by both `generate` and the dispatcher.

### tails-s3 — one spill per guarded copy (stacks on s2)

s1/s2 grew region bytes 5.5×, well past the ~2× the task expected, because each per-index cut
carried a full register spill (16 bytes × up to 16 registers). Each copy is now wrapped in a
block whose end spills and returns; a cut stores the PC and the packed result in `TMP` and
branches there. The intended change shares spill code. The census below shows small dispatch-count differences from s2, so it does not establish identical dispatch behavior.

## Census (jit-profile build, pocket-tank, 6 guest s, both cores, 2,034,559,010 instructions)

The `jit-profile` build does not compile on `x5/base`: `census::note_code_page` is called from
`exec.rs:236`, `block.rs:308` and `wasm_region.rs:198` but does not exist. Measured with a local
uncommitted two-line shim in `census.rs`, applied identically to base and candidates (it is
recorded in `wasm/*-prof.dirty.patch`) and reverted before every gate; the shim is profile-only
code and cannot reach a production artifact.

| counter (core 0 + core 1)        | base          | tails-s1      | tails-s2      | tails-s3      |
|----------------------------------|---------------|---------------|---------------|---------------|
| `run_calls` (wrapper entries)    | 28,764,666    | 28,747,945    | 28,732,346    | 28,732,183    |
| own module, whole                | 13,132,709    | 13,427,210    | 13,407,972    | 13,408,123    |
| own module, **tail cut**         | 15,217,673    | **2,471,440** | 2,429,067     | 2,429,072     |
| own module, **resumed**          | 23,033,690    | 20,528,885    | **9,521,924** | 9,521,950     |
| own-module calls, total          | 51,384,072    | 36,427,535    | 25,358,963    | 25,359,145    |
| wrapper chain hops               | 48,923,542    | 35,087,361    | 25,733,966    | 25,734,288    |
| region calls                     | 26,311,028    | 27,414,663    | 29,154,789    | 29,154,766    |
| region calls rejected            | 6,892         | 6,892         | 47,440        | 47,440        |
| instructions retired in regions  | 1,286,134,313 | 1,482,334,223 | 1,525,105,645 | 1,525,107,510 |
| **region generated bytes**       | 16,748,869    | 90,359,007    | 92,691,400    | **70,569,234**|
| region instructions compiled     | 68,692        | 122,745       | 127,364       | 126,991       |
| bytes per region instruction     | 244           | 736           | 728           | 556           |
| regions formed                   | 1,036         | 1,330         | 1,360         | 1,356         |

Reading: s1 removes 12,746,233 own-module tail calls per 6 guest s (−83.8%), i.e. ~63.7 M per
30 s, which matches the x2 dispatch census's 61 M credit-short hops per 30 s. s2 removes a
further 11,006,961 resumed own-module calls (−53.6% of s1's). Together: own-module calls
51.4 M → 25.4 M (−50.6%) and wrapper chain hops −47.4%, with 18.6% more guest instructions
retired inside regions. The base's `left_kinds[budget]` (14,652,198) is not a clean before/after
split, because a guarded cut is tagged `Budget` as well (s1: 15,770,516).

The byte growth is larger than the ~2× per instruction (244 → 556 in s3) because s1/s2 also
*form* larger and more regions (68,692 → 126,991 compiled region instructions): the changed
dispatch pattern makes more heads hot. s3 recovers 22.1 MB of the growth.

## Tests

`wasm_tests/regions.rs`, all inside the 83,387-case differential suite:

- Large 40-chunk region, direct calls: entry at every chunk × budgets 8,9,…,16,23,63,300,511 ×
  two store targets (1,040 cases). Budgets 9..15 cut the next chunk's guarded copy at every one
  of its indices. With the store outside the region's pages nothing forces an exit, so the new
  assertion is `done == budget` exactly — in the base the same call retires `8·⌊budget/8⌋`.
- s2 resume: every index of every chunk × budgets 1, 2, 7, 63 (1,120 cases), same `done ==
  budget` assertion plus the instruction-by-instruction interpreter oracle.
- `deferred_in_guarded_copy`: a credit-short edge enters chunk 1 two instructions short and the
  slow store there is refused under `defer_armed`; the refused instruction is counted and
  subtracted exactly as from an own-module body, its PC is the continuation, and the next
  dispatch (through EX172's interior alias) performs the store.
- Already-existing `region_program` shapes now route their credit-short edges and resumes
  through the guarded copies at ~900 random budgets each, against the interpreter oracle,
  including `noted` PC equality: hardware loops with LEND at a chunk end (`hwloop`, `dot`,
  `uniform`, `loop-entry` with LCOUNT 0/1/2 through the turn schedule), ENTRY chunks (`calls`,
  `entry-a4`, `loop-entry`, including the interior-entry rejection), internal conditional
  branches leaving a chunk early (`tile`, `hwloop`), helper fallbacks (`*-slow`, `*-off-end`,
  `*-readonly`), and self-modifying stores into a region page (`tile`, `*-self-modify`,
  `memmove` onto itself, `prev-page-store`).

## Prediction

s1: one generated call less per credit-short quantum on both cores; the risk is V8 compile time
for 5.4× region bytes. s2: removes most of the remaining resumed own-module calls, at the price
of ~10 extra WASM ops in every region prologue and 47 K wasted rejected calls per 6 s. s3: broadly similar
dispatch counts to s2 with 24% fewer region bytes; if s2 loses to compile time, s3 is the
variant to keep.

## Catalog rows (ready to paste; M3 numbers still to fill)

| <a id="ex182"></a>EX182 | **Run the quantum's tail inside the region**<br>tails; guarded chunk copies; resume into a region | *pending M3* | Base `x5/base` 8cbe0da4. s1 `d93694e3` emits a guarded copy (EX156's `br_table` entry and one `STOP` compare per instruction) of every region chunk longer than one instruction and jumps to it at a credit-short edge with `STOP = budget − DONE`, returning `CODE_LEFT` with a mid-chunk PC instead of leaving at the chunk head. s2 `75426fec` packs `(dispatch index \| index << 16)` into the region's entry parameter so a resume enters the guarded copy at that index and one call covers the resume and the rest of the quantum. s3 `0ff99486` shares one spill-and-return epilogue per copy. Unlike EX153 s2/s2b, which put a *checked* copy (per-instruction entry/budget/LEND tests) in regions and lost, and unlike EX156, which specialized own-module bodies only. Census per 6 guest s of pocket-tank: own-module calls 51.38 M → 36.43 M (s1) → 25.36 M (s2), of which tail cuts 15.22 M → 2.47 M and resumes 23.03 M → 9.52 M; wrapper chain hops 48.92 M → 25.73 M; instructions retired in regions 1.286 G → 1.525 G; region generated bytes 16.7 → 90.4 / 92.7 / 70.6 MB. Gates: 82,267 (s1) and 83,387 (s2, s3) differential cases, strict 30 s pinned pocket-tank exactness, native tests and clippy. | Not adopted yet; M3 pairs queued for pocket-tank (4) and TinyDraw (3) per candidate. [Mechanism, census and gates](tails.md) |
