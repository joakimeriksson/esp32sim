> Historical worker note, preserved before browser results were available. Predictions and queued status below are historical; see [closeout](../README.md) and [measured results](../results.json).

# helpers: fewer interpreter helper round trips

Worker `helpers`, worktree `$X-helpers`, branch `x5/helpers`, base `x5/base` = `8cbe0da4`.
Base differential suite: 81,626 cases (`$X/results/diff-base.log`).

A helper round trip is `spill` → `call_indirect h_exec` → `defer_instruction` → `note_pc` → the `exec_insn` match →
price/`block_break` → return. Reference volumes are the x3 exit-kind census
(`$X3/notes/calls.md` §1, 30 guest s of pocket-tank, both cores): **34.65M RETW**,
**14.03M WSR/XSR/RSIL** (13.84M with `jit_helped` set), **26.00M memory** slow-path helpers.

All three candidates delete helper *calls* without moving a single block or chunk boundary: every instruction
involved stays `ends_block` and `terminal_helper`, so admission, region formation, the retained-loop prefix rule and
the retired-offset reconstruction are untouched. The one dispatcher-visible effect the helper had — `jit_helped`,
which stops the EX153 chain (`wasm.rs:406`, read at `wasm.rs:516`) — is reproduced by one `i32.store8` from
generated code for exactly the opcodes `h_exec` sets it for, and deliberately *not* for returns.

## Counter evidence (measured, not assumed)

`jit-profile` build of the s1+s2 head, 10 guest s of pocket-tank under Node
(`census` helper counters; Node wall time is never a benchmark). To build it the lab-local no-op
`census::note_code_page` was needed because the `jit-profile` feature does not compile on `x5/base`
(`cannot find function note_code_page in module crate::census`, `exec.rs:236`, `block.rs:308`,
`wasm_region.rs:196`); that stub was reverted and never committed.

| helper call site after s1+s2 | core 0 | core 1 |
|---|---:|---:|
| `Rsil`, `Wsr(230)`, `Xsr(230)` | **0** | **0** |
| `Retw` + `RetwN` (guard failures: window underflow) | 88,155 | 1,328 |
| `RetN` (not yet inline at that build) | 85,164 | 40,288 |
| `Wsr(12)` SCOMPARE1 | 411,632 | 22,415 |
| `Wsr(177)` EPC1 / `Wsr(209)` EXCSAVE1 / `Wsr(224)` CPENABLE / `Wsr(228)`/`Xsr(228)` INTENABLE | 74,174 | 32,764 |
| `Wsr` of LBEG/LEND/SAR/BR/ACCHI/M0–M3 (not addressed) | 170,368 | 80,568 |
| scalar memory (peripheral pages `0x6000`–`0x600c`) | ~5.0M | ~0.16M |

So s1 removed every RSIL/WSR PS/XSR PS helper call in the real workload, and s2 leaves only ~90K RETW helper calls
per 10 guest s — the window-underflow exceptions, which must stay in the interpreter. Region exit kinds confirm the
shape is unchanged: `left_kinds[sr] = 0` on both cores (a PS terminal exits `CODE_END`, exactly where the helper
path landed) while `left_kinds[retw]` is still 2.72M/3.12M, i.e. RETW exits a region in the same place, now without
the round trip.

The same census on the s3 build leaves, of the rows above, only `Retw`/`RetwN` (88,155 + 1,328, the underflows) and
the unaddressed LBEG/LEND/SAR/BR/ACCHI/M0–M3 context-switch writes: `Wsr(12)`, `Wsr(177)`, `Wsr(209)`, `Wsr(224)`,
`Wsr(228)`/`Xsr(228)` and `RetN` are all gone, i.e. s3 removes a further ~666K helper calls per 10 guest s.

---

## helpers-s1 — RSIL and WSR/XSR PS as compiled terminals

- commit `50608d74`, artifact sha256 `eaee713c0083`, **83,247** differential cases, `EXACT ok` (30 guest s).
- Files: `wasm_policy.rs` (`ps_terminal`/`supported_insn`), `wasm_instruction.rs` (`emit_ps_terminal`). +137 lines.

Mechanism. `Rsil`, `Wsr PS` and `Xsr PS` are emitted inline at the end of the body instead of calling `h_exec`:
`ps = (ps & !INTLEVEL_MASK) | (imm & 0xf)` with the old PS into `AR[t]` (RSIL, `exec.rs:504`),
`ps = AR[t] & 0x0007_ff3f` (`Cpu::write_sr`, `exec.rs:147`), and the exchange (XSR). Only SR 230. The emission
carries `exec_insn`'s epilogue (a hardware loop ending on the terminal takes its backedge) and returns
`CODE_END`/`CODE_LEFT` where the helper path landed. `defer_instruction` is `false` for these opcodes (no
`word_access`, not PIE/MAC16) and `control_price` is 0 with `transfers` false, so priced builds need no special case.

Catalog relation. EX135 introduced these terminal helpers and the constraints around them (no retained loop prefix
after a terminal `WSR LBEG/LEND/LCOUNT`), all unchanged. Materially different: the helper *call* is gone, not the
boundary; the chain stop EX153 depends on is one store from generated code.

Directed tests (+1,621 cases): `control.rs::ps_terminals` sweeps the PS write mask, the old value returned in
`AR[t]`, `LEND` on the terminal's own fall-through in both the guarded and the checked body, and entry/budget cuts
landing on the instruction; every case asserts `supported_insn`, so a silent fallback cannot pass the oracle.
`scheduler.rs::ps_terminal_chain` runs an encoded `rsil a2,0` through the real block scheduler with a pending
level-1 interrupt masked by `PS.INTLEVEL=15` and asserts that the wrapper chain stops at the terminal (`done == 2`,
not 4) and that the interrupt lands on the next dispatch exactly as `crate::step` delivers it.

Prediction: ~14M helper round trips per 30 guest s, no dispatch hops removed (the chain still stops there).
Small: 0.3–0.8% on pocket-tank, less on TinyDraw.

---

## helpers-s2 — guarded inline RETW / RETW.N

- commit `530e34e2` on `x5/base` (branch `x5/helpers-s2only`; identical change to `8490fed0` on `x5/helpers`),
  artifact sha256 `762d27f7af6f`, **84,218** differential cases, `EXACT ok` (30 guest s).
- Files: `wasm_policy.rs` (`supported_opcode`), `wasm_instruction.rs` (`emit_retw`). +121 lines.

Mechanism (`exec.rs:535-546`). Guards `PS.WOE`, `a0 >> 30 != 0` and `windowstart & bit((windowbase - n) & 15) != 0`;
any failure calls the same helper as today, so ILLEGAL and window-underflow exception state comes from `exec_insn`
unchanged. The common path computes `ret = (a0 & 0x3fff_ffff) | (pc & 0xc000_0000)`, **spills the caller's window
first**, then clears `windowstart` bit `windowbase`, sets `windowbase = newbase` and `PS.CALLINC = n`, stores the PC
and returns `CODE_LEFT` through `ret_value` (no second spill through the rotated window). A return is a taken
transfer, so no hardware-loop backedge is emitted, and — matching `h_exec`, which excludes returns from
`jit_helped` — the chain continues across it. Priced builds keep the helper: EX138 charges a taken return the
fetch-alignment cycle of its target, which needs the target's bytes and is precomputed only for static targets
(`BlockInsn::straddle`).

Catalog relation. EX109 (Sep 19, one pair each, inside the base-to-base spread), EX159 (single-pair sweep) and
calls-s3/EX164 (bundled with region call edges, never independently queued). Materially different: the base carries
EX153 wrapper chaining and EX172 interior aliasing, so an inline return now feeds a chain hop instead of a
dispatch; and this is four pocket-tank pairs plus three TinyDraw pairs on one clean artifact, measured both
standalone and stacked on s1.

Directed tests (+2,592 cases): `control.rs::windowed_return` sweeps both encodings against every call increment 0–3,
a `windowstart` where only the returned-into frame is live, `windowstart` 0 (underflow) and `0xffff`, `PS.WOE` clear
and `WOE|EXCM`, three window bases, `LEND` on the return, and entry/budget cuts, asserting `supported_insn` each
time. The pre-existing `terminal_helpers` sweep (11,664 cases, re-run under `PRICED`) and
`scheduler::wrapper_chain`'s RETW section — which asserts that a successful return continues the chain and an
underflow does not — cover the emission end to end through the scheduler.

Prediction: the largest of the three, ~34.65M helper round trips per 30 guest s. 1–2% on pocket-tank.

---

## helpers-s1s2 — the stack

- commit `8490fed0` (s2 on top of s1), artifact sha256 `b1077f5c03b1`, **85,839** differential cases,
  `EXACT ok` (30 guest s).

Independent mechanisms sharing only `supported_insn`; queued so the incremental effect of s2 on top of s1 is
readable as well as each against the base.

---

## helpers-s3 — the rest of the terminal-helper family (not the assigned mechanism; see below)

- commit `30b4bdd8` (on top of s1+s2), artifact sha256 `937a31b72d60`, **89,295** differential cases,
  `EXACT ok` (30 guest s).
- Files: `wasm_policy.rs` (`sr_terminal`, `wsr_mask`, `supported_opcode`), `wasm_instruction.rs`
  (`emit_sr_terminal`, `Ret | RetN`). +97/−48 lines.

**The assigned s3 cannot fire on this base.** `fallback(..., continue_block = true)` is emitted from exactly one
site, `wasm_emit.rs:898` (`g.fallback(bi, pc, next, last, !last)`), and only when `instruction::emit` returns false
for a *non-last* instruction. `admitted()` (`wasm_policy.rs`) requires every non-last instruction of a compiled
block or region chunk to satisfy `supported_insn`, and the only instruction that is `supported_insn` yet refused by
`instruction::emit` is `ENTRY aN, imm` with `N >= 4` — which `exec_insn` answers with ILLEGAL, so the helper returns
TRAP and the reload result is discarded. Scalar memory in particular never reaches that site: with `fast` the
emitter compiles it and `memory::emit` uses `continue_block = false` (`wasm_memory.rs:85`, a `CODE_CUT`/`CODE_LEFT`
exit with no reload at all), and without `fast` `supported_insn` is false, so `admitted()` refuses the whole block
and no code is generated. `fast = bus.fast_mem().is_some()` is true throughout the pinned runs: the S3 bus only
returns `None` with the approximate cache enabled (`esp32s3/src/bus.rs`, `fast_mem`), which `bin/exact.mjs` and the
M3 runner never switch on. So the minimal-reload change would have removed dead code, not work — no candidate
queued for it, and no M3 time spent.

What is queued instead is the same family, chosen from the residual census above: the **write side of EX135's
`rsr_field` table**, plus RET/RET.N. `wsr_mask(n)` returns the mask `Cpu::write_sr` applies to any register
`rsr_field` already proves exact at any instruction of a dispatch (PS `0x0007_ff3f`, CPENABLE `0xff`, EXCCAUSE
`0x3f`, VECBASE `!0x3ff`, the rest unmasked); those WSR/XSR become the same compiled terminal as WSR PS, with the
same `jit_helped` store. PRID is excluded because `write_sr` drops its value rather than storing one. A terminal
CPENABLE or INTENABLE write is safe for the same reason PS is: the write is the block's last instruction, and the
next body re-proves its coprocessor guard while `jit_helped` forces the dispatcher to re-derive interrupt state.
RET/RET.N are `new_pc = ar!(0)` and nothing else (`exec.rs:534`), i.e. `JX` on A0; like RETW they keep the helper
under `PRICED` and do not set `jit_helped`.

Directed tests (+3,456 net cases): `control.rs::sr_terminals` sweeps sixteen inline registers and six that must keep
the helper (PRID, WINDOWBASE, WINDOWSTART, LCOUNT, SAR, CCOMPARE0) for both WSR and XSR, asserting
`supported_insn == expected` for each, over three PS/operand combinations and the full entry/budget/`LEND` matrix;
`terminal_helpers` now also asserts that every op in its list (CALL*, RET, RET.N, RETW, RETW.N) is emitted inline.

Measured effect (same 10 guest s census, on the s3 build): `Wsr(12)`, `Wsr(177)`, `Wsr(209)`, `Wsr(224)`,
`Wsr(228)`/`Xsr(228)` and `RetN` all drop to zero — **~666K helper calls per 10 guest s, ~2.0M per 30 s**, about 4% of
what s1+s2 removes. Most likely inside the pair-to-pair spread; it is queued because it is monotone (it can only
remove work) and free on top of s1s2, not because a measurable gain is expected. Both `helpers-s1s2` and
`helpers-s3` are measured against the base with the same pair count, so the quantity of interest is the difference
between them, not `helpers-s3` against the base alone.

Not addressed, and left for a future candidate: the FreeRTOS context save/restore writes `Wsr` LBEG/LEND/SAR/BR/
ACCHI/M0–M3 (251K per 10 guest s). LBEG/LEND/LCOUNT interact with the retained-loop-prefix accounting EX135 had to
fix, and SAR is read by generated shifts, so they need their own argument rather than `rsr_field`'s.

---

## Gate summary

| candidate | commit | artifact sha256 | differential cases | 30 s exactness | queued |
|---|---|---|---:|---|---|
| `helpers-s1` | `50608d74` | `eaee713c0083` | 83,247 | `EXACT ok` | pocket-tank 4, tinydraw 3 |
| `helpers-s2` | `530e34e2` | `762d27f7af6f` | 84,218 | `EXACT ok` | pocket-tank 4, tinydraw 3 |
| `helpers-s1s2` | `8490fed0` | `b1077f5c03b1` | 85,839 | `EXACT ok` | pocket-tank 4, tinydraw 3 |
| `helpers-s3` | `30b4bdd8` | `937a31b72d60` | 89,295 | `EXACT ok` | pocket-tank 4, tinydraw 3 |

All four artifacts were built by `bin/gate.sh` from a clean tree (every `.dirty.patch` is 0 bytes) and every case
count is above the base's 81,626 with no FAIL. Native `cargo test --release -p xtensa-lx7` passes on the head.
`cargo clippy -p xtensa-lx7 -p esp-soc --all-targets -- -D warnings` passes. Clippy for the `wasm32` target
(`-p esp32sim-wasm --features jit-tests`) reports three failures that are **pre-existing on `x5/base`** in files
this branch does not touch: `unnecessary_cast` at `wasm_memory.rs:193` and two `doc_lazy_continuation` at
`wasm_tests/regions.rs:602-603`.

---

## Catalog rows (ready to paste; the coordinator assigns the ids)

| <a id="exAAA"></a>EXAAA | **Compile the PS special-register terminals**<br>helpers-s1; RSIL; WSR PS; XSR PS; jit_helped store | *(pending M3)* | EX135 made RSIL/WSR PS/XSR PS terminal helpers; this emits them inline and keeps the block boundary. The only dispatcher-visible effect of `h_exec` for these opcodes is `cpu.jit_helped`, which stops the EX153 chain so interrupt masking is re-derived; generated code now writes that flag itself, so the dispatch sequence and the pinned interleaving are unchanged. A `jit-profile` census of the resulting build shows **zero** remaining `Rsil`/`Wsr(230)`/`Xsr(230)` helper calls in 10 guest s of pocket-tank (base reference: 14.03M WSR/XSR/RSIL helper stops per 30 s, EX164 census). 83,247 differential cases, strict 30 s exactness passes. Base `8cbe0da4`, commit `50608d74`, artifact `eaee713c0083`. | x5 round, branch `x5/helpers`; four pocket-tank pairs and three TinyDraw pairs queued. Not merged. [Note](../../esp32sim-x5/notes/helpers.md) |
| <a id="exBBB"></a>EXBBB | **Guarded inline windowed return**<br>helpers-s2; RETW; RETW.N; EX109 retry | *(pending M3)* | EX109/EX159/EX164 retry on a base that already has EX153 wrapper chaining and EX172 interior aliasing, so an inline return feeds a chain hop instead of a dispatch. PS.WOE, `a0 >> 30 != 0` and a live target frame are checked inline; every guard failure keeps `exec_insn`, so ILLEGAL and window-underflow state is unchanged. The caller's window is spilled before the rotation and returns still do not set `jit_helped`. Priced builds keep the helper (EX138's fetch-alignment cycle needs the target's bytes). Census after the change: ~90K RETW helper calls per 10 guest s remain, all window underflows, against 34.65M RETW exits per 30 s in the EX164 base census. 84,218 differential cases standalone (85,839 stacked on EXAAA), strict 30 s exactness passes. Commit `530e34e2` (artifact `762d27f7af6f`), stack commit `8490fed0` (artifact `b1077f5c03b1`). | x5 round; standalone and stacked-on-EXAAA artifacts each queued for four pocket-tank and three TinyDraw pairs. Not merged. [Note](../../esp32sim-x5/notes/helpers.md) |
| <a id="exCCC"></a>EXCCC | **Compile the remaining exact special-register writes and RET**<br>helpers-s3; wsr_mask; RET; minimal reload (not built) | *(pending M3)* | The write side of EX135's `rsr_field` table: every register that table proves exact mid-dispatch is a plain `Cpu` field written with a known mask, so WSR/XSR of it becomes the same compiled terminal as WSR PS (PRID excluded, its write is dropped). RET/RET.N are compiled as the indirect jump to A0 they are. Measured on a `jit-profile` build of the result: `Wsr(12)`, `Wsr(177)`, `Wsr(209)`, `Wsr(224)`, `Wsr(228)`/`Xsr(228)` and `RetN` all fall to zero, ~0.67M helper calls per 10 guest s of pocket-tank, ~4% of what EXAAA+EXBBB remove, so a measurable gain is not expected. **Negative structural result recorded here:** the assigned "minimal reload after a memory fallback" cannot fire — `fallback(continue_block = true)` is emitted only for a non-last instruction that `admitted()` has already proved `supported_insn`, which leaves only illegal `ENTRY aN` (always TRAP); compiled scalar memory uses `continue_block = false` and no reload, and uncompiled scalar memory makes the whole block inadmissible. 89,295 differential cases, strict 30 s exactness passes. Commit `30b4bdd8`, artifact `937a31b72d60`. | x5 round, stacked on EXAAA+EXBBB; queued for four pocket-tank and three TinyDraw pairs. Read against the EXBBB stack artifact, not the base. Not merged. [Note](../../esp32sim-x5/notes/helpers.md) |
