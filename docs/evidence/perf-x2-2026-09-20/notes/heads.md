QUEUED (prediction +1 to +3% on the M3, bytes -32 to -41%): 45% of all generated wasm is code for block heads that lie inside an already decoded block; the cause is exception returns (RFE) landing mid-block, plus mid-block exits after device or slow-memory accesses; entering the existing block's guarded body at the instruction index removes them, bit-exact.

# EX172 heads: stop interior PCs from growing their own blocks and regions

Base 414c6e07. Worktree `/Users/alice/src/a/esp32sim-x2-heads`, branch `x2/heads`. Commits: 14819b5a census (jit-profile only) + `tools/heads-run.mjs` (exact.mjs + `REPORT=<file>` + `jit.stats`), 36bf6a8e = tag `heads-s1`, 7f178131 = tag `heads-s2`, ff2184da directed test, 1d7a9179 = tag `heads-s3`. Raw output: `/Users/alice/src/a/esp32sim-x2/runs/heads/`.

## Step 1: census, where interior heads come from

Instrumented `build()` (xtensa-lx7/src/block.rs, `census::on_build`): a new head is "interior" when its PC is a non-first instruction of a block decoded earlier on the same core. The cause is the last dispatch's ending (trap, cut, or the last retired instruction and the path that retired it). Module bytes are attributed to the head that owns them (`census::module` in `prepare` and region formation, wasm.rs).

Base, 30 guest s (`runs/heads/census30.txt`; 8 s numbers in `census8.txt`):

| | all | interior heads | share |
|---|---|---|---|
| distinct heads | 16,820 | 3,627 | 22% |
| block modules / bytes | 4,484 / 38.8 MB | 808 / 13.5 MB | 35% of bytes |
| region modules / bytes | 1,645 / 45.1 MB | 461 / 24.4 MB | 54% of bytes |
| **total bytes** | 83.9 MB | **37.9 MB** | **45%** |
| kernel 4200f6fc..f8ba: heads / modules / bytes | 210 / 279 / 25.1 MB | 196 / 260 / 23.7 MB | 94% |

By cause (30 s, bytes of block + region modules owned by the interior head):

| hypothesis | result |
|---|---|
| (a) exception return mid-block | **Yes, the main cause.** `Rfe`: 1,601 heads, 326 modules, 15.9 MB directly. FreeRTOS level-1 interrupts return through RFE, not RFI. Each such head X then decodes 32 instructions, so X+32, X+64, ... become further misaligned heads by plain fallthrough until the loop's `bnez` (the rows `interp:Lsi:fallthrough`, `MulS`, `AddN`, `vmulas`, ...: about 11 MB more in the kernel in 8 s). In 8 s the kernel has 84 RFE heads (8.3 MB) and 112 fallthrough descendants (11.4 MB). |
| (b) budget cut whose resume is lost | **No.** `cut-resume-lost` never appeared as a cause. The resume slot is per core and survives the other core's turn. |
| (c) decoder length limit | **Not a cause by itself**, but it shapes everything: `MAX_LEN = 32` (block.rs:47), so the 146-instruction body is five blocks, not one. hotwat's "a 146-instruction block can never pass `budget >= len`" does not apply: the gate sees a 32-instruction chunk. One static duplicate remains: the loop is entered once through `J` at 4200f6ff and then through `bnez` at 4200f6fc, giving two aligned streams of five blocks each (16 interior heads, 1.0 MB). Left alone: static flow, 2x not 10x. |
| (d) CODE_CUT next-index resumes | **No.** They use the resume slot. |
| (e) new: mid-block exit after a device or slow-memory access | **Second cause.** A store that moves an interrupt line (`bus.block_break()`), a region `memory` exit or a deferred device access ends the dispatch mid-block and the next PC becomes a head: `interp:S32iN:fallthrough` 772 heads / 3.4 MB, `region:L32iN:fallthrough` 1.9 MB, `body:S32iN`/`body:L32iN` (deferred access, re-dispatched at the same PC) 1.7 MB. Hottest site: an IRAM register sequence at 40384358..403845xx on core 0 where almost every instruction owns a 25-40 KB block and a 45-70 KB region (`s1-census8b.txt`, `[heads] top` lines). |
| static branch targets inside an earlier fallthrough block | `interp:J`, `Bne`, `Bnez`, ...: about 1.3 MB. These are real loop heads and must keep their own block and region. |

Flip side (30 s, kernel only, `[heads] kernel exec`): region 1,399.6M instructions (70.4%), tail-cut block bodies 294.3M (14.8%), resumed guarded bodies 294.1M (14.8%), whole block bodies 0. A block body only runs when the region gate fails, so it is always a tail cut followed by a resume. The interior heads are therefore redundant copies: the resumed-at-index body already is the steady state.

## Step 2: fix

`find_block` (block.rs:340-360): on an entry-table miss, if the PC was produced by an aliasable event, `alias_lookup` (block.rs:310) finds a valid decoded block with code that contains the PC at an instruction boundary and returns `(entry, arena index, end)`, exactly what a budget-cut resume returns. `run_decoded` then enters the block module at `arena[k].off` through the EX156 guarded body, or interprets the tail if the block is not hot yet. No new decode, module or region.

- Map: 4,096-slot direct-mapped `pc -> (head, arena start, arena index)`, filled on the miss path by probing up to 93 candidate heads below the PC and walking instruction lengths. A hit needs the same head, the same arena start (the arena only grows between flushes, so that is the same build), current page versions and code; `flush()` clears the table.
- Policy (which arrivals alias): `BlockCache::alias_pc`, set by the RFE family in exec.rs:500-506 (s1); by a sequential mid-block exit from the interpreter, a block body or a region, `pc - last retired pc` in 2..3 (s2, `note_sequential`, block.rs:56, wasm.rs:597/708/798); by a deferred device access (s3, block.rs:426). It is cleared at every dispatch (block.rs:281). Static arrivals never alias, so loop heads inside a fallthrough block keep their own head and region. The policy only picks between two exact executions, so a wrong guess costs speed, never correctness.
- Off when `price_control` (pricing scoreboards reset at decoded block boundaries), `observed` or a probe bit matches the PC. `set_boundaries` already flushes on probe changes (core.rs:43). WASM only (`ALIAS = cfg!(target_arch = "wasm32")`); native is untouched.
- Why exact: blocks end at every instruction that can change interrupt, timer or window-exception state, `must_start_block` instructions are never interior, the CCOMPARE distance bounds every dispatch, and the block module re-proves loop end, window and coprocessor state on every entry (wasm_emit.rs:564-600). Region and EX153 chaining already rely on the same fact.

## Evidence (no timing)

30 guest s, production builds, Node (`runs/heads/*-exact30.json`, `jit.stats`):

| | base | s1 | s2 | s3 |
|---|---|---|---|---|
| modules compiled | 6,129 | 5,651 | 5,336 | 5,162 |
| generated bytes | 83,502,841 | 56,556,570 (-32.3%) | 52,286,375 (-37.4%) | 49,375,595 (-40.9%) |
| live bytes at end | 67,023,164 | 40,273,281 | 37,856,164 | 35,560,229 |
| peak modules | 4,826 | 4,646 | 4,482 | 4,344 |
| sync compile ms (Node, machine shared, noisy) | 1,015 | 852 | 519 | 425 |

8 guest s, jit-profile builds (`census8.txt`, `s1-census8.txt`, `s2-census8.txt`, `s3-census8.txt`) and V8 `--trace-wasm-compilation-times` (`tier-*.txt`, hotwat's method):

| | base | s1 | s2 | s3 |
|---|---|---|---|---|
| kernel heads / block modules / region modules / bytes | 210 / 144 / 99 / 21.1 MB | 30 / 30 / 5 / 1.9 MB | same | same |
| interior bytes, all code | 30.3 MB | 10.8 MB | 5.2 MB | 2.7 MB |
| sync compile ms | 1,136 | 440 | 429 | n/a |
| TurboFan functions / cumulative ms | 1,322 / 12,915 | 1,252 / 7,618 | 1,204 / 7,183 | n/a |
| Liftoff machine code | 160.4 MB | 117.1 MB | 108.6 MB | n/a |
| block builds, core 0 / core 1 | 86,779 / 10,766 | 83,708 / 9,925 | 52,135 / 9,878 | 50,362 / 9,866 |
| alias hits, core 0 / core 1 | 0 | 64,828 / 6,344 | 2,124,421 / 61,835 | 2,155,327 / 65,015 |
| `run_calls` core 0 / core 1 | 24,887,016 / 17,040,450 | 24,919,707 / 17,042,167 | 25,239,513 / 17,056,116 | 25,276,541 / 17,056,521 |
| region calls / retired, core 1 | 21,638,621 / 1,300.86M | 21,639,407 / 1,300.89M | 21,605,309 / 1,300.76M | n/a |
| region retired, core 0 | 452.9M | 452.6M | 443.5M | n/a |

Dispatch paths: s1 is flat (+0.13% core-0 `run_calls`, 71K alias lookups per 8 s). s2 and s3 add 1.4-1.6% core-0 `run_calls`, move about 9M of 1,112M core-0 instructions from regions to guarded bodies and pay one alias-table probe 2.2M times per 8 s.

Gates: `bin/exact.mjs` 30 s `EXACT ok` (instruction total 10,073,833,775, console and frames hashes) for heads-s1, heads-s2, heads-s3 (`runs/heads/exact-heads-s*.txt`). `tools/wasm-jit-test.sh` PASS 78,904 cases for s1 (ALIAS_SEQ off), s2 and s3, including the new `scheduler::interior_alias` (exception-return arrival at every interior index under budgets 1, 2, 3, 5 and 64 decodes nothing new; a static arrival still gets its own head; a changed instruction is never entered through a stale alias).

## Queued candidates (all vs plain base)

| job | artifact | commit | tests | counter evidence | predicted |
|---|---|---|---|---|---|
| heads-s1 | wasm/heads-s1.wasm (7a7e1c98ed21) | 36bf6a8e | alias exception-return PCs only | bytes -32%, kernel modules 243 -> 35, TurboFan 12.9 -> 7.6 s per 8 s, dispatch counts flat | +1 to +3% |
| heads-s2 | wasm/heads-s2.wasm (37b9d5aa44f0) | 7f178131 | s1 + sequential arrivals after a mid-block exit | bytes -37%, core-0 builds -40%, +1.4% core-0 run_calls | s1 +/- 0.5% |
| heads-s3 | wasm/heads-s3.wasm (af124c493c9a) | 1d7a9179 | s2 + deferred device access re-dispatch | bytes -41%, interior bytes 30.3 -> 2.7 MB per 8 s | about s2 |

Prediction honesty: TurboFan runs on background threads, so the 5+ s of compile saved per 8 guest s mostly frees other cores on a desktop. What the main thread gets: less synchronous `new WebAssembly.Module` work (8 s Node: 1,136 -> 440 ms; hotwat measured 635 ms per 30 s base run), fewer kernel executions in fresh Liftoff code (each of the 99 kernel regions had to warm up and tier up separately; now 5 do) and a smaller hot-code footprint. I expect +1 to +3% on the M3 and would not be surprised by +0.5%. s2 and s3 trade 4-7 MB more for 1.5% more core-0 dispatches, so they may tie or lose to s1 on the desktop. Hypothesis only: phones (EX166, 4,173 module installations) have fewer spare cores and less cache, so the footprint cut may matter more there.

## Catalog row

| <a id="ex172"></a>EX172 | **Alias interior PCs onto the existing decoded block; heads**<br>interior block heads; RFE return mid-block; misaligned block streams; duplicate regions; alias table | Queued; counter evidence only | 414c6e07 base, pocket-tank 30 s, Node. Census: 3,627 of 16,820 heads lie inside an earlier block and own 37.9 MB of 83.9 MB generated wasm (45%); the q4 matmul kernel alone 196 of 210 heads, 23.7 MB. Causes: exception return (RFE) mid-block 15.9 MB direct plus fallthrough descendants every 32 instructions; mid-block exits after device or slow-memory accesses about 7 MB; lost resume slots and CODE_CUT: none. Kernel executes 70.4% in regions, 14.8% tail-cut, 14.8% resumed, 0 whole. Fix: a lookup miss at an aliasable arrival enters the existing block's EX156 guarded body at the index (4,096-slot alias table, flush-cleared, page-version checked; static arrivals keep their heads; off under pricing, observers, probes; WASM only). s1 36bf6a8e RFE only: modules 6,129 -> 5,651, bytes 83.5 -> 56.6 MB, live 67.0 -> 40.3 MB, kernel modules 243 -> 35 (8 s), TurboFan 12.9 -> 7.6 s per 8 s, dispatch counts flat. s2 7f178131 + sequential mid-block exits: 52.3 MB, +1.4% core-0 run_calls. s3 1d7a9179 + deferred access: 49.4 MB. EXACT ok at 30 s for all three; 78,904 jit cases pass including a new directed alias test. Relates EX156 (reuses its indexed entry), EX106, EX107, EX113, EX045, EX119/EX127, EX166 (footprint hypothesis only); differs by removing redundant modules at the source. No timing yet. | Not adopted; wait for M3 pairs heads-s1, heads-s2, heads-s3. Notes `/Users/alice/src/a/esp32sim-x2/notes/heads.md`. |

## Next step and exactness risk

1. If the pairs confirm: adopt s1; take s2/s3 only if they do not lose to s1. Run the native test suite before a PR (the `Rfe` arms now write `alias_pc` on native too; it is never read there).
2. Remaining duplicates: the kernel's two static alignments (f6fc and f6ff streams, 1.0 MB) and the direct-mapped entry table's rebuild churn (core 0: 50K-87K builds for about 13K-16K heads per 8 s; EX045 territory, builds reuse code through `by_pc`, so this costs decode time, not modules).
3. Exactness risk: low. It rests on the invariant regions and chaining already use (no interrupt-visible state changes inside a block). Not covered by pocket-tank: probes and observers (aliasing is simply off), pricing (off), self-modifying code (covered by the directed test and the page-version check). The sequential test in s2 is a byte-distance heuristic on purpose: it only steers policy.
