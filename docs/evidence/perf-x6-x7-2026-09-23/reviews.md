# Correctness reviews of the x6/x7 stack

Six adversarial correctness reviews ran on the integration tip `x7/integrate-atom` `d6b5da216c722a7aeb6784dabfd53bdc1f98211d` (workload, shell, coverage, tails, MMIO, atomics). Each reviewer was a separate model session (`openai-codex/gpt-6-astra`, medium reasoning) with a fixed brief: read the assigned commits and the related catalog rows, check claimed invariants against the interpreter (`exec.rs`), build a directed reproducer for any finding and run no benchmarks. Every review worked in a detached scratch worktree and left the reviewed branches untouched. The first shell review was stopped by a provider content filter; it was rerun with a neutral brief, and that second run is summarized here.

Totals: **5 blockers, 1 major, 2 minor**. Coverage had no finding. Every finding has a fix and a directed regression test; for B1, T1, T2, both MMIO findings and the atomics minor the test was recorded failing without the fix, and for W1/W2 the reviewer's reproducers failed before the fix. The fixes for adopted code ship in the final stack (tip `2f3ae625`); the MMIO major and the atomics minor apply to candidates that were measured and then not adopted (mmio-s2/s2b, atomics), so their fixes are recorded but not shipped.

| Review | Commits reviewed | Findings | Fix commit | Regression test |
| --- | --- | --- | --- | --- |
| Workload | `949e28fe..067d0e94` | W1 blocker, W2 blocker | `4d7b5eb5`, re-pin `1f4c247d` (slice A, shipped) | `motion_survives_measured_te_in_either_order`, `one_timestamp_is_one_sample_and_is_consumed_once` |
| Shell | `7157759c`, `598b68ff` | B1 blocker | `a3de7247` (slice B, shipped) | `shell_bus_replacement` (the reviewer's test) |
| Coverage | `f5ffafd5`, `1f827041`, `ff384f5e` | none | none needed | reviewer's 32 guarded-JX and two changed-loop cases passed; not committed |
| Tails | `5b229d60`, `3123f2cd`, `9ddfcd5b`, `0f19dfe7` | T1 blocker, T2 minor | `d62b364e`, `ae81cb73` (slice D, shipped) | `deferred_in_guarded_copy` (extended), `head_recovery_long_pie` |
| MMIO | `32cd6ed1`, `02b3133b`, `e7633eec` | blocker, major | blocker `2f3ae625` (slice G, shipped); major `ce4587f2` (mmio-s2/s2b path, not shipped) | `timed_spi2_descriptor_collection_dirties_interrupts`, `device_store_without_versions` |
| Atomics | `557a5c47`, `20924aaf`, `d6b5da21` | minor | `894042e4` (atomics, not shipped) | `comparator_checks_threadptr` |

## Workload

- **W1 (blocker):** calling `esp32sim_set_imu_motion(e, 1)` and then `esp32sim_set_measured_te(e, 1)` left QMI8658 STATUS0 at 0 instead of 3. The TE setter replaced the board, and the new board's motion state defaulted to mode 0. The pinned manifest sets motion only, so it never hit this. Fix: the TE setter carries the selected motion over to the new board.
- **W2 (blocker):** in mode 1 two reads 1 ms apart returned the same timestamp (2575) with different accelerometer bytes, because the script was evaluated at each read while the timestamp floored time to 4 ms, and every read set STATUS0=3. Fix: samples on a 4 ms cadence, timestamp and data from the same sample index, STATUS0 reports new data until the outputs are read. This changes guest work: the pin moves from 13,329,744,922 to 13,335,545,195 instructions ([fluidbox.md](fluidbox.md)).
- Found sound: mode-0 byte compatibility with the old register stub (512-byte read plus 20,000 mixed operations), burst latching, clock publication before I2C transactions, per-board ownership, deterministic arithmetic, timestamp wrap and setter validation.

## Shell

- **B1 (blocker):** the shell-s2 flash epoch is a per-bus counter that starts at 1 on every `SocBus`. A CPU whose region cache was warmed on bus A and then run against bus B with an equal epoch but a rewritten second chunk executed A's stale chunk: the interpreter added 7,500 to `a3`, the JIT 5,000. Advertising an empty stable range made the same test and the full suite pass. The scenario needs a CPU and its caches to outlive a bus swap; separately constructed machines have separate caches. Fix `a3de7247`: each bus starts its epoch at a base from a process-global counter shifted by 32 bits (not the bus address), so equal epochs also mean the same bus; the reviewer's `shell_bus_replacement` test (warm on bus A, run on bus B whose later chunk differs, equal ranges and change counts) is committed and must match the interpreter.
- Found sound within one bus lifetime: every `page_ver` writer moves the epoch when covered pages can change, the PSRAM boundary exclusion is exact, the retained-loop memo resets on reuse and records stay paired through reset.

## Coverage

No finding. The reviewer checked adopted-loop admission (the actual `(LEND, LBEG)` pair), backedge LEND/LCOUNT checks, chunk splitting at LEND, the formation-time JX literal as a prediction guarded by the live register, the tails `r.jx = chunk.jx` integration and the MUL16 and mask-branch lowerings. Extra scratch tests passed: 98,677 cases with 32 guarded-JX cases and 98,679 with two changed-loop runs (neither committed).

## Tails

- **T1 (blocker):** head recovery after a guarded-copy cut called `build(head)`. When the head was unfetchable while the cut PC was fetchable, `build` raised IFETCH_ERROR (EXCCAUSE, EPC1, PS.EXCM changed), `head_lookup` discarded the error and execution continued from the cut PC with the exception state already entered. Reproduced with a fault-injecting bus: `(PS, EXCCAUSE, EXCVADDR, EPC1, DEPC)` went from all zero to `(16, 2, 1077346313, 1077346317, 0)`. A stale `alias_head` hint could also outlive a decoder flush. Fix `d62b364e`: `decode_block` fetches the first word before any mutation and returns `None` on a fault; the hint is consumed on use and reset by `flush`.
- **T2 (minor):** a cut after instruction 24 of a 32-instruction chunk of four-byte PIE instructions lies 96 bytes past the head, beyond the 93-byte backward alias scan, so recovery decoded a new block at the cut. Fix `ae81cb73`: `head_lookup` searches the block it just built. Without the fix: `(alias_hits, builds)` delta `(0, 2)` instead of `(1, 1)`.
- Found sound: retired counts and packed resume entries, TAIL-versus-CUT exit tagging, loops and ENTRY in copies, DIRTY after helpers, regeneration epoch moves and selection bounds. Gap noted: production uses `SHORT_SAMPLE` 1,024, tests use 2, so production-threshold selection (including an empty selection) is covered by source reading only.

## MMIO

- **Blocker:** mmio-s1 compares only SPI2 and GDMA interrupt sources around a write. Timed SPI2 DMA submission reads descriptor words through the bus, and a descriptor's next-pointer word can be a device register (the USB Serial/JTAG FIFO) whose read raises another device's interrupt. The comparison then missed it. Fix `2f3ae625` (shipped with mmio-s1): timed descriptor collection sets `irq_dirty`. The test places the EOF descriptor's next word on the USB FIFO and asserts `irq_dirty`, `block_break` and CPU0 line 5 immediately after the SPI_USR write.
- **Major:** after a continuing region store, the emitter computed `versions == 0` but still loaded every `versions[page]` (WASM `i32.or` does not short-circuit). With no fast memory the pointer is null. Fix `ce4587f2`: a real `if/else` with no load in the null arm. The test runs a word-copy region with a null versions pointer and an out-of-memory page index; without the fix it trapped with `memory access out of bounds`. This emitter path exists only with mmio-s2/s2b, which were measured and not adopted ([EX190](../../experiments.md#ex190)); the fix is not in the final stack.
- Found sound: word-access equivalence with `exec_insn`, deferral before `note_pc`, exception conversion, result packing, pricing refunds and chain stops.

## Atomics

- **Minor:** the differential comparator `same()` did not compare THREADPTR, so a wrong `cpu.threadptr` at a budget cut right after WUR would pass. Fix: the comparison moved into `mismatch()`, which includes THREADPTR, with a self-test. Fix `894042e4`. The atomics candidates were not adopted (EX159, EX191, EX192), so this test fix is not shipped either.
- Found sound: L32E/S32E addressing, S32C1I compare-and-store order, fallbacks, store-version bookkeeping, RFWO/RFWU/RFE, CCOUNT/INTERRUPT placement at dispatch heads, ROTW and the added RSR fields. The reviewer's 1,152-case memory matrix passed (99,797 cases, not committed).

## Gates after the fixes

Final code stack, tip `2f3ae625` (artifact `c2144b73362f2aa43ea45bdc7f57efddc36a7d5b99e50397a83a8ca3ead672df`): `tools/wasm-jit-test.sh` 94,063 WASM differential cases; `cargo test --workspace --release` 457 passed, 0 failed, 22 ignored; native all-targets and wasm32 Clippy clean with warnings denied; 30 s Pocket exact (10,073,833,775 instructions, 737 frames, `71a5ccdb…`); 30 s fluidbox exact against base-motion2 (13,335,545,195 instructions, 744 frames, `9b196782…`).

Earlier, larger integration `x7/final` `091ce114` (with mmio-s2/s2b and the atomics, before B1): 98,655 cases (98,645 at the reviewed tip plus 10 new), 97,397 modules released; workspace 457 passed, 22 ignored; Clippy clean; both 30 s exact gates passed; artifact `5a118b41bf6338087f25a8fc12be01af404e983cc840cbb521a10ab7dbccf38e`.

## Receipts and limits

Reviewer notes, reproducer patches and logs stay in lab storage (hashes of the notes in [sources.json](sources.json): `notes/review-*.md`, `notes/fixes.md`). The tails reviewer recorded patch hashes: `head-fetch-regression.patch` `e7504da82ad62b3f8f94845cc166166b5cbc1ebd12634e6f2a09a599f3004fee`, `head-fetch-guard.patch` `73d1760b23ff55660ddd172396ee66238cd88f8f086cad857dae8bd8516618bd`; the atomics reviewer `directed-tests-final.patch` `0311271288c126fa7d3b0af154aa446361723a9848a3a1f186484fd7085a5446`. Reviews are scoped to their commits at one tip; they are not proofs of the whole stack and did not rerun author exactness or timing claims. The MMIO findings were source-traced; their reproducers were written with the fixes.
