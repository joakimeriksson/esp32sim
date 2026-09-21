QUEUED (prediction s1 +1.5%, s2 +2.5%, s3 +3.5%): interpreting tiny no-code blocks in place removes 19.9M / 30.6M / 40.0M of 183.8M machine dispatches (10.8% / 16.7% / 21.8%), bit-exact, no new compiled code.

# EX171 inchain: bridge tiny uncompiled blocks inside the EX153 chain loop

Worktree `/Users/alice/src/a/esp32sim-x2-inchain`, branch `x2/inchain`, on base 414c6e07. All counts: pocket-tank 30 guest s,
Node, `--features jit-profile` builds, runner `/Users/alice/src/a/esp32sim-x2-census/tools/census-run.mjs`. Raw outputs in
`/Users/alice/src/a/esp32sim-x2/runs/inchain/{census,s1,s2,s3}.txt` (grep `ex171`, `ex153`, `interp_dispatches`).

## Related experiments and what differs
EX044/EX159 compile singletons into wasm (negative/flat), EX111 interprets budget tails (no gain), EX152 run_many loops ALL
blocks through the full dispatcher inside Cpu (rejected, -0.64%/+1.4%), EX153 wrapper chaining (in base), EX108 untried.
EX171 adds no compiled code and no new dispatcher: it only interprets, inside the existing chain loop, the no-code block
that would otherwise end the chain, then keeps chaining.

## Step 1: census on base (commit ac21e8b5: census counters, artifact `inchain-census.wasm`, `runs/inchain/census.txt`)
Why EX153 chains end (`[ex171] chain_end`):

| reason | core 0 | core 1 |
|---|---|---|
| exit_cut (budget cut inside a block/region) | 50.60M | 47.94M |
| helped (interpreter helper ran) | 23.18M | 1.12M |
| **nocode (next block has no code)** | **9.11M** | **4.59M** |
| budget used up | 6.51M | 6.03M |
| trap / pre-trap | 0.73M | 0.04M |
| undecoded + must_start + cold + stale | 0.19M | 0.04M |

Of the nocode chain ends, by whitelist class (`[ex171] nocode_class`, fits = whole block fits the remaining budget):
class 1 (pure register/branch/call, no memory) 5.24M core 0 + 4.29M core 1; class 2 (returns, LOOP setup, ENTRY, exact RSR)
3.49M + 0.24M; class 3 (memory) 0.06M; class 0 (never) 0.29M + 0.06M. "Does not fit" is negligible (<25K per class).
What follows the interpreted tiny block (`[ex171] follow`): after a class-1 chain end, core 0 4.09M compiled vs 1.12M
interpreted, core 1 3.55M vs 0.72M: the bridge really extends chains, saving two dispatches in most cases.
Only 37% of core 0's 24.7M interp dispatches are reached from a chain end; the rest start a dispatch (after a helper,
trap or another interpreted block): `via=dispatch class=1 full` 6.94M and `class=2 full` 3.71M on core 0. That is what s3 targets.
Bridgeable blocks at chain ends were under 15M (9.5M s1, 13.3M s2) but each is worth up to two dispatches, so I continued.

## Step 2-3: what was built
- `bridge_class` / `bridge_block_class` (xtensa-lx7/src/block.rs): class per instruction, cached per decoded entry in
  `Entry.bridge` when the block gets no code. Class 1 = the emitter's inline non-memory ops (wasm_policy.rs `supported`) plus
  their pure unsupported siblings (Bnone/Bany/Ball/Bnall/Bf/Bt, boolean ops, Mul16, Clamps, Nsa, Movf/Movt). Class 2 = Ret/RetN/
  Retw/RetwN, Loop/Loopnez/Loopgtz, Entry, Rsr of `rsr_field` registers (EX135 exact mid-dispatch). Memory ops, L32e/S32e,
  S32c1i, Wsr/Xsr/Rsil, Waiti, Rf*, PIE, FP: never bridged.
- `block::bridge` + the `else` arm of `chain_target` in `jit::wasm::run` (xtensa-lx7/src/jit/wasm.rs chain loop): same
  per-instruction sequence as `run_decoded` (check_overflow, note_pc, exec_insn). Window-overflow pre-trap leaves as
  CODE_TRAP_PRE, an exec trap (div by zero, RETW underflow, ENTRY illegal) as CODE_TRAP with `cpu.jit_trap`, exactly the
  wrapper's own trap exits; a completed block is a CODE_END and the loop re-checks budget, observed, HELPED, bloom and
  page versions before the next hop. A block that does not fit the remaining credit ends the chain as today. Disabled under
  `price_control`. Bridged instructions are subtracted from `jit_instructions` (jit_insns stays 10,014,435,040).
- s3 (`BRIDGE_INTERP`, block.rs `run_decoded` loop): when an interpreted dispatch ran a whole class<=2 block with no trap,
  credit left, the CCOMPARE deadline not reached (`done < room`), no observer, no bloom at either PC, `!block_break()`,
  it continues with the next valid decoded block (compiled chain or interpreter) under a freshly derived deadline.
Exactness argument: a class<=2 instruction touches no memory and no interrupt, timer, waiting or device state, so what the
dispatcher re-derives between blocks (check_interrupts, waiting, refresh_irq, deferral, sw_reset, stubs/probes via
boundary_bloom) is unchanged: the same argument EX153 uses for helper-free END/LEFT exits, and h_exec already treats a
non-trapping return that way (wasm.rs h_exec comment). CCOUNT reads stay excluded (must_start_block).

## Step 4: counters (base -> s1 -> s2 -> s3)

| counter | base | s1 c759fa30 | s2 a45c7fbb | s3 10e38aa0 |
|---|---|---|---|---|
| core 0 wrapper calls (`run_calls`) | 90.32M | 85.67M | 82.09M | 82.09M |
| core 0 interp dispatches | 24.74M | 18.05M | 13.12M | 13.12M (7.77M continued in place) |
| core 0 bridged blocks | 0 | 6.69M | 11.62M | 11.62M |
| core 0 machine dispatches (calls + interp - continued) | 115.06M | 103.72M | 95.21M | 87.45M (-24.0%) |
| core 0 chained hops per wrapper call | 0.95 | 1.05 | 1.14 | 1.14 |
| core 1 wrapper calls | 59.75M | 56.20M | 55.27M | 55.27M |
| core 1 interp dispatches | 8.96M | 3.95M | 2.65M | 2.65M (1.59M continued) |
| core 1 bridged blocks | 0 | 5.01M | 6.31M | 6.31M |
| core 1 machine dispatches | 68.71M | 60.15M | 57.92M | 56.33M (-18.0%) |
| core 1 chained hops per wrapper call | 2.29 | 2.50 | 2.56 | 2.56 |

(s3's jit-profile counters were taken at 8837b8e7, before the probed-PC fix 10e38aa0, which only affects test-style bloom changes.)
Instruction total 10,073,833,775 and jit_insns identical in every run (`[census-total]`).

## Step 5: gates
`bin/exact.mjs` 30 s: EXACT ok for inchain-s1, inchain-s2, inchain-s3. `tools/wasm-jit-test.sh` at 10e38aa0 (superset config):
PASS 78,903 differential cases. It caught one real contract miss in s3 ("region ran through a probed head", regions.rs:46):
a dispatch starting at a probed PC must stay one block long; fixed in 10e38aa0 the way EX153 does it. Native
`cargo test -p xtensa-lx7 --release`: all ok.

## Queued candidates (all against plain base)

| job | artifact | commit | what it tests | counter evidence | predicted |
|---|---|---|---|---|---|
| inchain-s1 | wasm/inchain-s1.wasm | c759fa30 | chain-loop bridge, class 1 only | -19.9M dispatches (10.8%) | +1.5% |
| inchain-s2 | wasm/inchain-s2.wasm | a45c7fbb | + class 2 (returns, LOOP, ENTRY, exact RSR) | -30.6M (16.7%) | +2.5% |
| inchain-s3 | wasm/inchain-s3.wasm | 10e38aa0 | s2 + interpreted dispatch continues into next block (block.rs level) | -40.0M (21.8%) | +3.5% |

Prediction basis: step_blocks 11.3% + find_block 2.6% + a slice of run_inner over ~184M dispatches is roughly 0.08-0.1% per
million dispatches; the interpretation work itself is unchanged. EX152's flat result warns that the machine layer alone is
cheap, so s3's extra 9.4M may add little; s1/s2 also skip run_decoded's deadline scan, cache take/put and accounting twice per bridge.

## Catalog row (ready to paste)
| <a id="ex171"></a>EX171 | **Interpret tiny uncompiled blocks inside the wrapper chain**<br>inchain; bridge; no-code block | Queued, exact; timing pending | On x2/base 414c6e07, pocket-tank: 13.7M EX153 chains end at a block with no code (singletons and unsupported-op blocks). The chain loop now interprets such a block in place when it is pure register/branch work (s1 c759fa30), plus non-trapping returns, LOOP setup, ENTRY and EX135-exact RSR (s2 a45c7fbb), and an interpreted dispatch of such a block continues into the next block (s3 10e38aa0). Machine dispatches 183.8M -> 163.9M / 153.1M / 143.8M; interp dispatches core 0 24.7M -> 13.1M, core 1 9.0M -> 2.7M; same 10,073,833,775 instructions and console hash; 78,903 differential cases pass. Unlike EX044/EX159 no code is compiled; unlike EX152 only chain-breaking tiny blocks are bridged, inside the existing wrapper loop. | Timing queued on M3 (inchain-s1/s2/s3). Notes: x2 `notes/inchain.md`, counters `runs/inchain/`. |

## Next step and risk
- If s2/s3 win: the bigger chain breakers are `helped` (23.2M on core 0: slow-memory helpers, WSR) and class-3 blocks starting a
  dispatch (1.3M L32e/S32e window handlers ending in RFWO, class 0 today). Bridging memory ops needs a proof that the access is plain
  RAM (FastMem TLB hit) and RFWO/RFWU need a `check_interrupts` re-run because they clear PS.EXCM: doable at the block.rs level only.
- Risk to exactness: the class table is the contract. Any op added to class 1/2 that can touch PS.INTLEVEL/EXCM, INTENABLE, CCOUNT,
  memory or `waiting` breaks it silently on workloads the gate does not cover; price_control mode is excluded rather than proven.
