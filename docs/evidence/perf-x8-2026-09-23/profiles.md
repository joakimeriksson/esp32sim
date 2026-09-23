# x8 CPU profiles: where fluidbox time went at each stage

Chrome 153 CPU profiles on the M3 Pro, 30 guest seconds each, one arm per profile (`bin/m3-prof.py` in the lab; the q256 profiles use its `TREE` variant). Seconds are sampled self time, so shares matter more than totals: the profiler adds overhead, and the wall time under the profiler is not a timing result. The `.cpuprofile` captures are not committed; [profiles.json](profiles.json) holds these numbers, and [sources.json](sources.json) holds the capture hashes under `lab/results/prof/`. [curate.py](curate.py) (`profile_shares`) derives the numbers: Rust names are demangled and shortened to their last path component, and a generated module is named by its head PC.

Two build kinds are profiled. **cpu-profile** builds (`--features cpu-profile`) keep generated module names (`xtensa_<pc>`) and mark some dispatch functions non-inline, so the split between `step_blocks`, `run_block_inner` and `jit::run` is inflated there while their total is not. **Production** artifacts (the timed build) keep their name section, so Rust attribution is real but inlined: in base8, `step_blocks` absorbs the inlined dispatch chain, and generated modules are unnamed.

## Categories (fluidbox unless noted)

| Profile | Stack | Build | Quantum | Artifact | Busy s | Generated | Rust | JS |
| --- | --- | --- | ---: | --- | ---: | ---: | ---: | ---: |
| `base8-fl` | step 0 (base8) | cpu-profile | 64 | `d0623d55` | 36.37 | 17.87 (49%) | 18.23 (50%) | 0.24 |
| `base8-prod-fl` | step 0 (base8) | production | 64 | `c2144b73` | 35.98 | 18.13 (50%) | 17.61 (49%) | 0.24 |
| `base8-pocket` (Pocket) | step 0 (base8) | cpu-profile | 64 | `d0623d55` | 23.24 | 11.18 (48%) | 11.65 (50%) | 0.39 |
| `int8-c-fl` | int-c | cpu-profile | 64 | `5895cb23` | 33.13 | 17.34 (52%) | 15.53 (47%) | 0.25 |
| `int8-c-prod-fl` | int-c | production | 64 | `24933057` | 32.53 | 17.07 (52%) | 15.19 (47%) | 0.26 |
| `w3all-cp-fl` | w3-all | cpu-profile | 64 | `7fca9d13` | 30.94 | 16.01 (52%) | 14.71 (48%) | 0.20 |
| `w3all-prod-fl` | w3-all | production | 64 | `79e0b66a` | 29.98 | 15.44 (52%) | 14.29 (48%) | 0.23 |
| `w3all-q256-cp` | w3-all, quantum 256 | cpu-profile | 256 | `7fca9d13` | 20.80 | 11.66 (56%) | 8.94 (43%) | 0.19 |
| `w3all-q256-prod` | w3-all, quantum 256 | production | 256 | `79e0b66a` | 20.00 | 11.12 (56%) | 8.65 (43%) | 0.22 |

Busy = profiled time minus idle. The q256 rows run the same artifacts as the w3-all rows with `esp32sim_set_quantum 256` through the workload exports. That is a different work contract (13,334,523,179 instructions instead of 13,335,545,195), so compare q64 and q256 rows by share, not as equal work. The step-0 note read the `base8-fl` capture as generated code about 53% and the Rust shell about 48% of a busy time of about 33.6 s. This summarizer counts 36.37 s busy for the same capture (39.11 s profiled minus 2.74 s idle), which gives 49% and 50%. The seconds agree (generated 17.87 s, `run_inner` 6.21 s); only the busy denominator differs.

## Top Rust functions (self seconds)

| Profile | 1 | 2 | 3 | 4 | 5 | 6 |
| --- | --- | --- | --- | --- | --- | --- |
| `base8-fl` | `run_inner` 6.21 | `run_block_inner` 4.33 | `jit::run` 1.80 | `run_unmodeled` 1.24 | `step_blocks` 1.19 | `exec_insn` 0.64 |
| `base8-prod-fl` | `step_blocks` 6.93 | `run_inner` 6.29 | `run_unmodeled` 1.15 | `exec_insn` 0.63 | `alias_lookup` 0.48 | `periph_write` 0.26 |
| `base8-pocket` | `run_inner` 4.26 | `run_block_inner` 2.50 | `jit::run` 0.97 | `run_unmodeled` 0.90 | `step_blocks` 0.66 | `exec_insn` 0.36 |
| `int8-c-fl` | `run_block_inner` 5.29 | `run_inner` 3.67 | `step_blocks` 1.24 | `run_unmodeled` 1.22 | `jit::run` 1.09 | `exec_insn` 0.59 |
| `int8-c-prod-fl` | `run_block_inner` 6.20 | `run_inner` 3.47 | `step_blocks` 1.25 | `run_unmodeled` 1.21 | `exec_insn` 0.63 | `tick` 0.26 |
| `w3all-cp-fl` | `run_inner` 3.36 | `run_unmodeled` 2.08 | `resume` 2.03 | `run_block_inner` 1.89 | `jit::run` 1.08 | `run_memo_hit` 0.83 |
| `w3all-prod-fl` | `step_blocks` 3.33 | `run_inner` 3.26 | `run_unmodeled` 2.02 | `resume` 2.00 | `run_memo_hit` 0.84 | `exec_insn` 0.61 |
| `w3all-q256-cp` | `run_inner` 2.44 | `run_block_inner` 1.32 | `jit::run` 0.91 | `run_unmodeled` 0.66 | `resume` 0.59 | `exec_insn` 0.49 |
| `w3all-q256-prod` | `run_inner` 2.39 | `step_blocks` 2.32 | `run_unmodeled` 0.71 | `resume` 0.58 | `exec_insn` 0.56 | `periph_write` 0.27 |

`alias_lookup` falls from 0.43–0.48 s (base8) out of the top ten after alias-s1 ([EX172](../../experiments.md#ex172)); `run_block_inner` in int-c holds the resume-memo path (lane-s1, [EX195](../../experiments.md#ex195)), which memo-s2/lane-s2b then split into `resume` and `run_memo_hit`. In `int8-c-prod-fl` the Rust shell (`run_block_inner`, `run_inner`, `step_blocks`, `run_unmodeled`) is 12.1 s against base8's 14.4 s (`step_blocks`, `run_inner`, `run_unmodeled`; `run_block_inner` inlined), which is what the second wave targeted (lab task `w2.md`).

## Top generated modules (cpu-profile builds, self seconds, by head PC)

| Profile | 1 | 2 | 3 | 4 | 5 | 6 |
| --- | --- | --- | --- | --- | --- | --- |
| `base8-fl` | `4200c490` render 3.40 | `4200cf79` density 2.07 | `4200ce9e` 1.37 | `4200d204` 0.96 | `40002274` ROM `__call___divsf3` 0.54 | `400570e8` memset 0.45 |
| `base8-pocket` | `4200f75f` 1.18 | `4200f6ff` 1.07 | `4200e9bd` 0.87 | `420180c5` 0.56 | `4200f626` 0.50 | `4200f69c` 0.25 |
| `int8-c-fl` | `4200cf79` 3.53 | `4200c490` 3.50 | `4200d0e4` 1.52 | `4200c2c2` 0.50 | `40379c30` 0.47 | `4200d5ec` 0.28 |
| `w3all-cp-fl` | `4200c490` 3.29 | `4200cf79` 3.15 | `4200d0e4` 1.40 | `4200c2c2` 0.44 | `40379c30` 0.36 | `40002274` 0.28 |
| `w3all-q256-cp` | `4200ce01` 2.73 | `4200c490` 2.14 | `4200d0e4` 1.17 | `40379c30` 0.38 | `4200c2c2` 0.29 | `4200d5ec` 0.19 |

In int-c the density region `4200cf79` grows (2.07 → 3.53 s) because rename-s1 inlines `sqrtf`/`__divsf3` into it, the ROM trampoline region `40002274` that ran `__divsf3` shrinks (0.54 → 0.21 s) and the memset module `400570e8` falls from 0.45 to 0.18 s (store-s1 bulk runs, loop-s1 resumes). Under q256 the density loop's top module is headed at `4200ce01` instead of `4200cf79`; the cause was not investigated.

## Receipts

Artifacts: `c2144b73` = base8 (production, `x8/base` 85c639db); `d0623d55` = base8 cpu-profile build; `24933057` = int-c production (`ad7c9853`); `5895cb23` = its cpu-profile build; `79e0b66a` = w3-all production (`x8/trial3` 3b696862); `7fca9d13` = its cpu-profile build. Every profiled arm completed with the pinned instructions and 744 frames (`profiles.json`). Profile directories in the lab: `results/prof/{base8-fl, base8-prod-fl, base8-pocket, int8-c-fl, int8-c-prod-fl, w3all-cp-fl, w3all-prod-fl, w3all-q256-cp, w3all-q256-prod}`.
