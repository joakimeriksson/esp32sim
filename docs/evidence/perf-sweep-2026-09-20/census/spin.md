DEAD: pocket-tank retires at most 0.017% of its 10.07 G instructions in blocks whose register state recurs (fixed-point candidates), far below the 5% kill line; both cores do real compute, not spin-waiting. No prototype built, no benchmark run.

# EX151 spin: exact fast-forward of fixed-point spin loops (census only)

Worktree `/Users/alice/src/a/esp32sim-exp-spin`, branch `exp/spin`, commit `d3b6ddaf` (census hack on top of
`exp/perf-push-base` cf1187a6). Native CLI, 30 guest seconds, same arguments as
`docs/evidence/overnight-2026-09-19/tools/pt-native.sh` (`--boot rom ... --flash-at 0x290000=model_q4.bin`).
All runs in `/Users/alice/src/a/esp32sim-exp/runs/spin-census/`.

## What I measured

1. Workload identity: `core0 4446180089 + core1 5627653686 insns` = 10,073,833,775, the pinned total
   (`stderr.txt` line 5, `spin-blocks.txt` line 5). The census observes the same run the browser benchmark pins.
2. Per-core block table (`--profile-blocks`, hacked to key by core, block PC and length; `stderr.txt`):
   - core 0: 38.8% in one block at `4200e9bd` len 15, about 50% with its budget-truncated variants. Disassembly
     (`disasm.txt` lines 23-41): a `loop a10` body that byte-swaps two `l16ui` values and does `s32i a8, a12, 0`:
     a pixel-format conversion with a store per iteration, not a poll.
   - core 1: 13.6% at `4200f626` (nibble dequantize, two `s8i` per iteration), 4.3% at `4201177d` and 2.8% at
     `420115a8` (`lsi`/`madd.s`/`ssi` float kernels), 4 x 3.4% len-32 blocks at `4200f6fc..4200f82d` (`disasm.txt`).
   - By 4 KB page: `4200f000` 42.6%, `4200e000` 23.0%, `42011000` 8.6%, `42018000` 7.9%, `__divsf3` 3.2%, `memcpy` 2.1%.
     All IRAM pages (FreeRTOS, spinlocks, tick) are under 0.7% each. No ROM delay function reaches the 0.22% cutoff of
     the top 20 (`stderr.txt` lines after `[profile-blocks]`), so CCOUNT busy-waits (`esp_rom_delay_us`) are < 0.22%.
3. Fixed-point detector (`ESP32SIM_SPIN_CENSUS=1`, `esp-soc/src/machine.rs` `spin_census`): after every block, hash
   (next PC, a0-a15, PS, WINDOWBASE); if the hash matches one of that core's last 16 block-end states, the block's
   instructions count as "recurring register state". This is an UPPER bound for exact fixed points (it does not
   check stores) and it also catches device-register polls that keep returning the same value. Result
   (`spin-blocks.txt` lines 6 and 32):
   - core 0: 879,292 of 4,447,119,872 (0.02%); core 1: 794,281 of 5,627,925,760 (0.01%).
   - Total 1,673,573 = 0.0166% of 10.07 G.
   - Of that, 613,466 (`4037b612`, both cores) is the `retw.n` after `waiti 0` (`disasm2.txt` lines 12-14), that is
     WAITI wake-ups, not spinning; the census agent's "waiting" rows show the same 390,251 / 223,215
     (`notes/census.md` lines 30 and 53). About 287 K on core 1 is ROM `memcpy` (`40056f44..40056f97`) repeating
     with equal registers: false positives with stores.
   - Genuine pure-RAM `while(!flag)` polls do exist: `42044558: memw; l8ui a8,a9,0; extui; beqz` (core 0,
     222,448 instructions) and the same shape at `40378ed5` (core 1, 149,672) (`disasm2.txt` lines 26-29 and
     33-36). Together 372 K instructions = 0.0037% of the run, startup core-sync only.
   - Device-register polling loops: none visible; anything that polled a constant done-bit would be inside the
     0.0166% bound.
4. The EX047 hint ("cores spin-wait on each other longer" at quantum 512, 10.07 G -> 10.83 G) is not explained by
   fixed-point spinning at quantum 64: there is essentially none to extend. Both cores sleep in WAITI when they
   have nothing to do (census.md line 76: 17.0 M + 35.1 M dispatches already run as EX133/EX144 virtual quanta).
   I did not investigate where the extra 0.76 G at q512 comes from.

Level 1 (in-quantum fast-forward) could save at most 0.017% of instructions; Level 2 (spinning core counts as
idle for EX133) has nothing to latch onto because a pocket-tank core that is not computing is already in WAITI.
Not built. No bench.sh run was made, so there are no timing numbers.

## Catalog row

| <a id="ex151"></a>EX151 | **Exact fast-forward of fixed-point spin loops; a spinning core treated as virtually idle**<br>spin census; RAM-poll fixed point; while(!flag) skip; virtually idle spinner | Dead for pocket-tank (census only, Sep 19) | Unlike EX007 no firmware patterns and no semantic change were proposed: only loop iterations that provably leave architectural state unchanged would be retired arithmetically, and such a core would count as idle for EX133/EX144. Census on the native CLI, pocket-tank 30 guest s, pinned total reproduced (4,446,180,089 + 5,627,653,686 = 10,073,833,775): instructions in blocks whose visible register state (next PC, a0-a15, PS, WINDOWBASE) recurs within 16 blocks, an upper bound that ignores stores and includes constant device polls, are 1,673,573 = 0.0166%; 613 K of that is the return after WAITI and about 287 K is ROM memcpy. Genuine pure-RAM `memw; l8ui; beqz` polls exist at `42044558` and `40378ed5` but retire 372 K instructions (0.0037%), at startup only. The hot code is compute with stores: core 0 about 50% in a pixel-conversion `loop` at `4200e9bd`, core 1 in dequantize and float kernels at `4200f626`, `4201177d`, `420115a8`. No ROM delay loop reaches 0.22%. The EX047 remark that cores "spin-wait on each other longer" at quantum 512 is not backed by any fixed-point spinning at quantum 64. | Not built, not benchmarked. Revisit only for a workload whose census shows a material recurring-state share (the `ESP32SIM_SPIN_CENSUS=1 --profile-blocks` hack on branch `exp/spin` d3b6ddaf measures it in about 4.5 minutes). Related: EX007, EX133, EX144, EX047, EX034, EX063. Receipt: `/Users/alice/src/a/esp32sim-exp/notes/spin.md`, runs `/Users/alice/src/a/esp32sim-exp/runs/spin-census/` |

## Next step

None for pocket-tank. The census tool is reusable: run it on any new both-cores-busy workload before reviving
this idea. Exactness risk was never exercised.
