# Census: pocket-tank on base (exp/perf-push-base cf1187a6), shared measurements

Worktree `/Users/alice/src/a/esp32sim-exp-census` (branch exp/census, commit d4fc547a adds the counters, all behind `jit-profile`).
Raw output: `runs/census/jitprof.txt`; tables: `python3 runs/census/summarize-jitprof.py runs/census/jitprof.txt`.

## 1. jit-profile counters (Node, instrumented build: counts only, never timing)

Commands:
`bin/build.sh /Users/alice/src/a/esp32sim-exp-census census-jitprof --features jit-profile`
`ESP32SIM_ROM_DIR=/Users/alice/src/a/esp32sim/web/wasm/fw CENSUS_OUT=runs/census/jitprof.txt ESP32SIM_WASM=wasm/census-jitprof.wasm node tools/census-run.mjs pocket-tank`
Run retired insns=10073833775 (= pinned total), jit_insns=9,817,326,916 (97.5%), 6243 modules compiled, 728 ms compile (`[census-total]` line).

A "dispatch" = one `run_block` call (step_blocks -> cpu.run -> run_block_inner -> jit run or interpreter loop). Quanta = iterations/64.

**Headline: 443M dispatches for 157M quanta. Core 0: 3.01 dispatches per 64-insn quantum; core 1: 2.66.** Average 22.7 insns per dispatch.
The quantum edge itself manufactures dispatches: on core 1, 65% of region exits are budget exits (54.8M of 83.9M), 52.2M entry-0 block bodies end in CODE_CUT,
and 50.6M dispatches could not enter their region only because budget < chunk length (they run the single-block module instead).

Paths (where each dispatch went; "body" = the block's own module, not a region):
- region_hot = EX136 cached entry; region_slow = slow lookup entry; body_noregion = entry 0, no live region covers the PC;
  body_gatefail = region exists but the entry gate failed (see gate line); body_resumed = entry != 0 (resumed after a cut);
  interp_nocode = block has an op the emitter does not support, whole block interpreted; interp_cold = not hot yet (<32 hits).


### core 0: 209,073,394 dispatches, 4,447,119,872 iterations, 21.3 insns/dispatch, quanta(it/64)=69.5M, **dispatches per 64-insn quantum = 3.01**
| path | dispatches | % disp | insns | % insns | insns/dispatch |
|---|---:|---:|---:|---:|---:|
| irq | 65,048 | 0.0 | 65,048 | 0.0 | 1.0 |
| waiting | 390,251 | 0.2 | 390,251 | 0.0 | 1.0 |
| interp_nocode | 25,113,646 | 12.0 | 47,006,165 | 1.1 | 1.9 |
| interp_cold | 152,278 | 0.1 | 818,527 | 0.0 | 5.4 |
| region_hot | 53,371,542 | 25.5 | 1,272,665,834 | 28.6 | 23.8 |
| region_slow | 9,212,290 | 4.4 | 505,431,677 | 11.4 | 54.9 |
| body_noregion | 38,485,083 | 18.4 | 263,805,943 | 5.9 | 6.9 |
| body_gatefail | 28,309,112 | 13.5 | 216,185,994 | 4.9 | 7.6 |
| body_after_reject | 239,731 | 0.1 | 301,567 | 0.0 | 1.3 |
| body_resumed | 53,734,413 | 25.7 | 2,140,448,866 | 48.1 | 39.8 |
budget at dispatch: ==64: 54,405,303 (26.0%), 1-8: 25,279,440 (12.1%), 9-32: 44,494,543 (21.3%), 33-63: 67,893,297 (32.5%), >64 (virtual quanta): 17,000,811
insns retired per dispatch: 0-1: 30,561,947 (14.6%), 2-4: 53,286,643 (25.5%), 5-16: 54,807,521 (26.2%), 17-63: 42,534,388 (20.3%), ==64: 23,578,501 (11.3%), >64: 4,304,394
[census-gate] budget_lt_len=25700143 bloom=0 loop=2608969 stale_pages=0
[census-body-exits] entry0[end,left,trap,cut,pre,reject]=[7337432, 28619158, 174066, 30672763, 230507, 0] resumed=[8005987, 22587126, 62011, 23076099, 3190, 0]
core=0 [wasm-region] formed=1192 failed=5958 covered=11258 dropped=28 chunks=15834 instructions=99791 bytes=21717961 calls=62823563 rejected=239731 retired=1778220536 exits[end,left,trap,cut,pre]=[5997783, 56345048, 241001, 0, 0] left_kinds[call,callx,retw,ret,jx,sr,memory,edge,budget,dirty,other]=[8835726, 7343096, 8480779, 62355, 327, 0, 6293091, 1315485, 23805296, 0, 208893]
interp (total of listed top-40 45,572,874): nocode:RetwN 8.24M, nocode:L32e 4.84M, nocode:S32e 4.06M, nocode:J 3.75M, nocode:BeqzN 2.29M, nocode:BnezN 1.85M, nocode:Rsr 1.52M, nocode:S32c1i 1.45M, nocode:Sub 1.29M, nocode:S32iN 1.29M, nocode:And 1.28M, nocode:Srli 1.03M, nocode:Nsau 1.03M, nocode:Call8 0.96M
helper (total of listed top-40 46,883,926): RetwN 19.47M, Wsr 7.88M, L32iN 5.87M, S32iN 5.65M, Rsil 5.42M, L32i 1.06M, S32i 0.99M, RetN 0.26M, Retw 0.23M, L32r 0.04M, Xsr 0.00M, L8ui 0.00M, S16i 0.00M, Wfr 0.00M
pie_table (total of listed top-40 147,480): ee.vsubs.s8 0.05M, ee.andq 0.05M, ee.vsr.32 0.02M, ee.src.q 0.02M
pie_packed (total of listed top-40 140,497): ee.vld.128.ip 0.08M, ee.vmulas.s8.accx.ld.ip 0.04M, ee.vmulas.s8.accx 0.01M, ee.zero.accx 0.01M

### core 1: 234,207,869 dispatches, 5,627,925,760 iterations, 24.0 insns/dispatch, quanta(it/64)=87.9M, **dispatches per 64-insn quantum = 2.66**
| path | dispatches | % disp | insns | % insns | insns/dispatch |
|---|---:|---:|---:|---:|---:|
| irq | 30,131 | 0.0 | 30,131 | 0.0 | 1.0 |
| waiting | 223,215 | 0.1 | 223,215 | 0.0 | 1.0 |
| interp_nocode | 19,216,413 | 8.2 | 208,626,887 | 3.7 | 10.9 |
| interp_cold | 47,844 | 0.0 | 324,072 | 0.0 | 6.8 |
| region_hot | 71,546,063 | 30.5 | 3,757,381,235 | 66.8 | 52.5 |
| region_slow | 12,381,084 | 5.3 | 394,404,629 | 7.0 | 31.9 |
| body_noregion | 20,631,309 | 8.8 | 153,518,095 | 2.7 | 7.4 |
| body_gatefail | 57,591,458 | 24.6 | 555,035,655 | 9.9 | 9.6 |
| body_after_reject | 336 | 0.0 | 367 | 0.0 | 1.1 |
| body_resumed | 52,540,016 | 22.4 | 558,381,474 | 9.9 | 10.6 |
budget at dispatch: ==64: 53,973,758 (23.0%), 1-8: 35,083,109 (15.0%), 9-32: 39,822,821 (17.0%), 33-63: 70,214,966 (30.0%), >64 (virtual quanta): 35,113,215
insns retired per dispatch: 0-1: 21,476,140 (9.2%), 2-4: 45,153,856 (19.3%), 5-16: 59,672,048 (25.5%), 17-63: 97,822,746 (41.8%), ==64: 1,269,557 (0.5%), >64: 8,813,522
[census-gate] budget_lt_len=50550585 bloom=0 loop=7040873 stale_pages=0
[census-body-exits] entry0[end,left,trap,cut,pre,reject]=[1378923, 24653496, 9777, 52180192, 715, 0] resumed=[34309719, 17850627, 338, 379324, 8, 0]
core=1 [wasm-region] formed=442 failed=1832 covered=411 dropped=2 chunks=7102 instructions=78190 bytes=22369027 calls=83927483 rejected=336 retired=4151808650 exits[end,left,trap,cut,pre]=[277098, 83624439, 25610, 0, 0] left_kinds[call,callx,retw,ret,jx,sr,memory,edge,budget,dirty,other]=[193897, 12587228, 9007652, 30100, 0, 0, 214220, 6838814, 54752150, 0, 378]
interp (total of listed top-40 208,096,287): nocode:Pie 127.28M, nocode:L32iN 8.05M, nocode:Extui 6.85M, nocode:Wur 5.79M, nocode:AddN 5.04M, nocode:Slli 4.80M, nocode:Rur 4.47M, nocode:Addx2 4.47M, nocode:Quos 4.44M, nocode:L16ui 4.44M, nocode:FloatS 4.44M, nocode:L32r 3.45M, nocode:J 3.40M, nocode:Bbsi 2.40M
helper (total of listed top-40 15,284,892): RetwN 13.88M, Wsr 0.69M, L32i 0.24M, Rsil 0.16M, RetN 0.12M, L32iN 0.11M, S32iN 0.05M, S32i 0.03M, L32r 0.01M, Retw 0.00M, L8ui 0.00M, Xsr 0.00M, L16ui 0.00M, Ssi 0.00M
pie_table (total of listed top-40 69,161,040): ee.andq 23.05M, ee.vsubs.s8 23.05M, ee.src.q 11.53M, ee.vsr.32 11.53M
pie_packed (total of listed top-40 58,147,769): ee.vld.128.ip 34.59M, ee.vmulas.s8.accx.ld.ip 13.35M, ee.zero.accx 5.77M, ee.vmulas.s8.accx 4.45M

## 1b. CORRECTION TO THE BRIEF: the cores ping-pong only 48% of guest time (round census)

Counter added in `esp-soc/src/machine.rs` (census commit 6265a0c5, `round_census`), same Node run recipe, artifact `wasm/census-jitprof2.wasm`,
raw line `[census-rounds]` in `runs/census/jitprof2.txt` (same pinned total 10073833775, same dispatch counts as run 1):

| round kind | guest cycles (per-core timeline, 7.195G = 30 s x 240 MHz) | share |
|---|---:|---:|
| both cores busy, 64-insn ping-pong | 3,443,029,760 | 47.9% |
| virtual quanta, only core 1 busy (EX133/EX144) | 2,177,719,424 | 30.3% |
| virtual quanta, only core 0 busy | 956,503,296 | 13.3% |
| all idle (skipped in chunks) | 567,984,183 | 7.9% |
| one core busy but ordinary 64-insn round (VQ skipped/penalty/remainder) | 49,459,937 | 0.7% |

- Ordinary ping-pong rounds: **54,653,018** (not 157M). They carry 2 x 3.443G = 6.89G instructions = 68.4% of the 10.07G; virtual quanta carry 3.13G = 31.1%.
- `vq_stats[runs,quanta,device_stop,waiti]=[12350639, 48972230, 242365, 2782]`: 12.35M VQ runs cover 48.97M quanta, i.e. only **3.97 quanta (254 insns) per VQ run**;
  242K runs ended at a device register, 2.8K at WAITI; the rest ran to their `vq_quanta` bound (`machine.rs:658-669`: min of cap 1024, next device deadline, the sleeping peer's wake timer, script event, display push), so that bound is what keeps VQ runs short.
- Busy share per core: core 0 retires 4.447G of 7.195G cycles (61.8%), core 1 5.628G (78.2%).
- So "dispatches per 64-insn quantum" (3.01 / 2.66 above) is an average over both regimes; budget==64 dispatch counts (54.4M / 54.0M) match the ordinary round count, as they should.

Reading the raw lines:
- `census-gate`: why an entry-0 dispatch with a live region ran the body instead: budget < chunk len / boundary bloom / active hardware loop (LCOUNT != 0 with LEND inside the region and not a region loop).
- `census-body-exits`: exit code histogram of block-body calls, entry==0 vs resumed. CODE_CUT = budget cut mid-block; LEFT = branch/call/return out.
- `wasm-region`: region calls, rejects (generated entry check said no), retired, exit codes, and LEFT kinds. `budget` = continuation edge found too little allowance left.
- budgets > 64 are EX133/EX144 virtual quanta (the other core idle): 17.0M dispatches on core 0 and 35.1M on core 1 started with budget > 64, so both cores are NOT busy all the time.
- `helper` = h_exec calls from generated code (interpreter helper for one instruction), by op. RetwN dominates: 19.5M (core 0) + 13.9M (core 1). Wsr 7.9M + Rsil 5.4M on core 0 are FreeRTOS critical sections. L32iN/S32iN/L32i/S32i helpers = slow-memory fallbacks (12.6M core 0, 0.4M core 1).
- `interp` = instructions executed by the block interpreter, by op (prefix nocode: = block not compilable).
- `pie_table` = PIE ops through `pie::exec_table` (no packed fast path, no JIT); `pie_packed` = PIE ops through `exec_packed`. Both are counted only when the interpreter/helper runs them; PIE ops emitted as WASM SIMD are not counted.

Findings worth a look:
1. Core 1 still interprets one PIE kernel (the one at 4200fdb3, section 3): 127.3M interpreted PIE instructions, of which 69.2M go through `exec_table`
   (`ee.andq` 23.05M, `ee.vsubs.s8` 23.05M, `ee.src.q` 11.53M, `ee.vsr.32` 11.53M). Per `supported_insn` the ops that block compilation are
   **`ee.src.q`, `ee.vsr.32`, `ee.vsubs.s8` and `wur.sar_byte` (Wur:13)**; `ee.andq` is supported but rides along in the interpreted block (see 1c), as do 34.6M `ee.vld.128.ip`,
   13.3M `ee.vmulas.s8.accx.ld.ip` and the Rur/Quos/FloatS/L16ui scalar tail: 19.2M dispatches, 208.6M instructions (3.7% of core-1 instructions but the slowest ones).
2. Core 0 has 25.1M interpreted dispatches at 1.9 insns each (12% of its dispatches): window-overflow/underflow handlers
   (L32e 4.8M, S32e 4.1M, Rfwo/Rfwu 0.48M each), RetwN 8.2M, S32c1i 1.45M, Rsr 1.5M, Nsau 1.0M.
3. Core 0: 48% of instructions retire in resumed bodies (entry != 0) at 39.8 insns/dispatch: that is the retained hardware loop at 4200e9bd
   (27.3M calls, 123M backedges) running as a block module because a resumed dispatch can never enter a region.
4. Tiny dispatches: 40% (core 0) and 28.5% (core 1) of dispatches retire <= 4 instructions.

## 3. Per-core guest PC histogram (exact, not sampled)

Source: `[census-pc]` rows of `runs/census/jitprof.txt` = every dispatch's start PC with dispatches and instructions retired (top 3000 PCs per core = 99.7% / 100.0% of instructions).
No ELF exists for pocket-tank (only `web/wasm/fw/public/pocket-tank.bin`), so functions are named by their `entry` instruction:
image segments split with a 10-line Python ESP-image parser into `/tmp/census-dis/seg*.bin`, disassembled with
`xtensa-esp32s3-elf-objdump -b binary -m xtensa -D --adjust-vma=<seg addr>`; ROM names from `nm esp32s3_rev0_rom.elf`.
Tables: `python3 runs/census/funcs.py` (needs `/tmp/census-dis/dis_*.txt`).

core 0: top 20 guest functions by instructions (dispatch-start PC attributed; region calls can run into callees only via fallthrough, never across calls)
| function | insns | % core | dispatches | insns/dispatch |
|---|---:|---:|---:|---:|
| fn_4200e958 | 2,318,108,831 | 52.13 | 41,534,505 | 55.8 |
| fn_42017c3c | 981,885,053 | 22.08 | 41,077,610 | 23.9 |
| fn_42019c00 | 307,301,878 | 6.91 | 11,828,589 | 26.0 |
| __divsf3 | 182,774,165 | 4.11 | 8,435,005 | 21.7 |
| fn_4037fe3c | 66,262,933 | 1.49 | 14,190,137 | 4.7 |
| fn_40384358 | 49,936,495 | 1.12 | 9,708,025 | 5.1 |
| fn_40380038 | 36,586,764 | 0.82 | 5,631,178 | 6.5 |
| fn_42017b64 | 27,951,745 | 0.63 | 2,530,241 | 11.0 |
| _xtos_set_intlevel | 26,111,865 | 0.59 | 6,151,831 | 4.2 |
| fn_42019878 | 25,392,946 | 0.57 | 972,574 | 26.1 |
| fn_42042a34 | 19,507,126 | 0.44 | 1,076,112 | 18.1 |
| fn_4200f328 | 17,621,935 | 0.40 | 83,520 | 211.0 |
| fn_42024544 | 17,338,701 | 0.39 | 876,472 | 19.8 |
| memset | 17,255,109 | 0.39 | 526,234 | 32.8 |
| fn_4037b498 | 16,839,842 | 0.38 | 3,654,023 | 4.6 |
| memcpy | 15,554,063 | 0.35 | 1,420,272 | 11.0 |
| fn_42018388 | 14,881,050 | 0.33 | 1,302,971 | 11.4 |
| fn_4037a874 | 13,949,543 | 0.31 | 2,208,251 | 6.3 |
| __call___divsf3 | 12,184,911 | 0.27 | 6,174,447 | 2.0 |
| fn_4037ad90 | 11,678,148 | 0.26 | 2,596,097 | 4.5 |

core 1: top 20 guest functions by instructions (dispatch-start PC attributed; region calls can run into callees only via fallthrough, never across calls)
| function | insns | % core | dispatches | insns/dispatch |
|---|---:|---:|---:|---:|
| fn_4200f328 | 4,270,748,205 | 75.88 | 150,545,851 | 28.4 |
| fn_42011014 | 863,835,460 | 15.35 | 36,969,398 | 23.4 |
| memcpy | 199,653,348 | 3.55 | 16,707,350 | 12.0 |
| __divsf3 | 142,456,176 | 2.53 | 6,288,916 | 22.7 |
| fn_42010070 | 67,402,604 | 1.20 | 2,533,743 | 26.6 |
| memset | 30,536,027 | 0.54 | 1,799,173 | 17.0 |
| __call_memcpy | 18,790,352 | 0.33 | 9,506,298 | 2.0 |
| __call___divsf3 | 9,497,049 | 0.17 | 4,804,034 | 2.0 |
| fn_40388010 | 3,783,909 | 0.07 | 829,789 | 4.6 |
| fn_40378020 | 2,232,979 | 0.04 | 536,136 | 4.2 |
| fn_40380120 | 1,842,030 | 0.03 | 220,349 | 8.4 |
| fn_4037fe3c | 1,722,664 | 0.03 | 321,331 | 5.4 |
| fn_4038154c | 1,475,663 | 0.03 | 99,589 | 14.8 |
| fn_4038047c | 1,445,649 | 0.03 | 352,463 | 4.1 |
| __udivdi3 | 1,274,041 | 0.02 | 120,901 | 10.5 |
| fn_403766f0 | 1,260,719 | 0.02 | 83,309 | 15.1 |
| fn_40380038 | 1,175,088 | 0.02 | 159,669 | 7.4 |
| fn_40384828 | 1,137,686 | 0.02 | 410,143 | 2.8 |
| fn_403821c0 | 1,079,352 | 0.02 | 109,347 | 9.9 |
| ets_delay_us | 936,639 | 0.02 | 322,049 | 2.9 |

By address range: core 0 = flash app 85.7% of instructions (35.0 insns/dispatch), ROM 6.0% (9.6), IRAM (FreeRTOS, critical sections, window handlers) 8.0% of instructions
but **67.8M dispatches = 32% of core-0 dispatches at 5.2 insns/dispatch**. Core 1 = flash app 92.5%, ROM 7.2% (memcpy, __divsf3), IRAM 0.3%.

What the hot code is (disassembly in `/tmp/census-dis/dis_42000020.txt`):
- Core 0 `fn_4200e958` (52.1%): display blit. `loop a10` at 4200e9ba, body 4200e9bd..4200e9e7 = 15 insns x 32 iterations: two `l16ui`, RGB565 byte swap
  (extui/slli/or), one `s32i`, three pointer adds; outer `bne` at 4200e9f0. Every 32-pixel strip starts with `callx8 0x4037fbf4` (a11 = -1, a blocking take).
  The 14 PCs 4200e9c0..4200e9e7 at ~2.01M dispatches and ~68 insns each are budget-cut resumes landing on every offset of the loop body:
  this one loop is why 48% of core-0 instructions retire through `body_resumed` (a resumed dispatch cannot enter a region).
- Core 0 `fn_42017c3c` (22.1%): the tank renderer: single-precision float per-pixel math (lsi/mul.s/madd.s/trunc.s, `ole.s`/`bt`) then a `loop` of RGB565 blend + `s16i` (4201_81c5..820c).
- Core 1 `fn_4200f328` (75.9%): the 4-bit matmul. Hottest single block is scalar, not PIE: nibble unpack loop 4200f626..4200f644
  (l8ui, extui, addi -8, srli, 2x `s8i`, `beq`/`j`; 13 insns per byte) = 870.7M + 196.5M = **1.07G instructions = 19.0% of core 1** in 23.9M dispatches.
  The PIE dot-product blocks (4200f6fc..4200f888: `ee.vld.128.ip`, 3x `ee.vmulas.s8.accx.ld.ip`, `ee.vmulas.s8.accx`, `rur.accx_0`, `ee.zero.accx`) are compiled and take 2.06G (36.6% of core 1); the whole unpack range 4200f5f8..4200f647 is 1.13G.
  The second kernel at 4200fdb3..4200fe12 (`wur.sar_byte`, `ee.src.q`, `ee.andq`, `ee.vsubs.s8`, `ee.vsr.32` + the same vmulas ops + quos/float.s) is the
  interpreted one from section 1: 177.4M instructions start in 4200fdb0..4200fe14 (blocks 4200fdb3 99.0M, 4200fdef 35.9M).
- Core 1 `fn_42011014` (15.4%): scalar float loops (`lsi`/`madd.s`/`ssi` saxpy at 4201177d, 420115a8: attention/accumulate), `memcpy` 3.6%, `__divsf3` 2.5% (ROM soft-float divide, also 4.1% of core 0).

**Spin-wait / polling: none found.** Every block in the per-core top 25 (core 0: 71.6% of its instructions, core 1: 75.0%) is compute with stores or accumulators
(blit `s32i`, blend `s16i`, unpack `s8i`, PIE accumulate + `rur`, memcpy, float divide); no store-free self-loop appears.
Idle time is real idle, not spinning: the cores sit in WAITI and the scheduler skips it (7.9% all-idle, 43.6% single-core virtual quanta, section 1b);
`waiting`-state dispatches are 0.39M (core 0) + 0.22M (core 1) of 443M. Share of instructions attributable to spin-waiting: ~0%.
The closest thing to overhead-only guest code is core 0's IRAM/RTOS traffic: 355M instructions (8.0% of core 0, 3.5% of all) in very short dispatches,
with helper-executed `Wsr` 7.9M + `Rsil` 5.4M + `S32c1i` 1.45M (critical sections/spinlocks) and the window-overflow handlers.

## 1c. Why blocks run in the interpreter (nocode dispatches by the block's set of unsupported ops)

Counter in census commit ba15fe3a, artifact `wasm/census-jitprof3.wasm`, raw rows `[census-nocode-missing]` in `runs/census/jitprof3.txt` (pinned total again).
Key = ops of the block for which `emitter::supported_insn` is false; empty key = every op is supported but `compile` refuses blocks of one instruction (`xtensa-lx7/src/jit/wasm.rs:261`, `instructions.len() < 2`).

| core | unsupported ops in block | dispatches | insns |
|---|---|---:|---:|
| 0 | (none: single-instruction block; J 3.75M, BeqzN 2.29M, BnezN 1.85M ... per the interp op list) | 12,295,820 | 12,264,460 |
| 0 | RetwN (singleton) | 7,217,350 | 7,217,350 |
| 0 | RetwN+S32c1i | 1,077,819 | 5,113,960 |
| 0 | Bnone | 600,996 | 1,891,614 |
| 0 | Rotw | 549,086 | 845,639 |
| 0 | L32e+Rfwu (window underflow handler) | 541,968 | 4,844,620 |
| 0 | L32e+Rfwo+S32e (window overflow handler) | 538,167 | 4,844,790 |
| 0 | S32c1i | 436,125 | 1,130,192 |
| 0 | Bany | 290,259 | 966,983 |
| 0 | Rsr:234 (CCOUNT) 250,339; Rsr:226 (INTERRUPT) 172,952; context save/restore + interrupt exit blocks (Rsr LBEG/LEND/LCOUNT/SAR 0-3, Rsr BR/ACCLO/ACCHI/M0-3, Wsr, Rfe; SR numbers per `xtensa-lx7/src/state.rs:19-29`) ~0.6M | | |
| 1 | Pie:ee.src.q+ee.vsr.32+ee.vsubs.s8+Wur:13 | 7,944,285 | 172,873,388 |
| 1 | (none: single-instruction block) | 8,044,813 | 8,044,773 |
| 1 | RetwN (singleton) | 1,209,754 | 1,209,754 |
| 1 | Pie:ee.src.q+Wur:13 | 580,191 | 11,499,820 |
| 1 | Pie:ee.src.q+ee.vsr.32+ee.vsubs.s8 | 449,616 | 10,473,566 |
| 1 | Rsr:234 (CCOUNT) | 322,013 | 936,903 |

Single-instruction blocks + singleton RetwN = 19.5M of core 0's 25.1M interpreted dispatches and 9.3M of core 1's 19.2M. They cost a full dispatch each for one instruction.
