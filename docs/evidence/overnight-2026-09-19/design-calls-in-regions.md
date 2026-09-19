# First experiment: direct CALL8 edges inside one region

**Design only; not implemented.** Start on the untimed combined4 engine. Keep RETW, ordinary returns and indirect calls as exits. Do not change scheduling quanta, opcode coverage or cache ownership policy.

## Why this bounded first step

The combined3 diagnostic counted about 128.8 million LEFT exits: 45.6M direct calls, 8.2M indirect calls and 51.7M RETW exits. The direct-call bucket includes CALL0/4/8/12, not just CALL8. Using the earlier rough 38 ns/dispatch estimate, eliminating *every* direct-call dispatch would save at most about 1.73 seconds of the 42.59-second battery, or 4.1%. Eligible CALL8 sites are a subset, and window spills/reloads remain. This is an optimistic ceiling, not a speed prediction. All call/return families together represent about four seconds, not 82% of elapsed time. [Histogram and confirmation](https://github.com/aliceisjustplaying/esp32sim/blob/codex/experiment-catalog/docs/evidence/overnight-2026-09-19/README.md)

## Smallest mechanism

1. Let region discovery follow a **direct CALL8 target** when the target begins with ENTRY a1; leave other prologues untouched in the first screen. Use the current instruction, chunk and page limits. Missing code, probe boundaries or unsupported target prefixes must retain the current exit behavior.
2. Keep CALL8 terminal in the decoder and in `terminal_helper`. Add its callee successor explicitly; do **not** add a fall-through edge to the return address.
3. Emit the existing CALL8 legality check, PS.CALLINC update and encoded return address. After its existing retirement increment, use `region_edge(target, false)` when that callee head belongs to this region. Otherwise keep the current return to Rust. Do not call `leave()` here: it would retire the call again.
4. Reuse the current ENTRY implementation. RETW still spills the callee state and returns through the existing helper. There is no return-site lookup in this experiment.

Receipts: [successor discovery and chunk boundaries](https://github.com/aliceisjustplaying/esp32sim/blob/7edb3bec/xtensa-lx7/src/jit/wasm_region.rs#L81-L113), [CALL emission](https://github.com/aliceisjustplaying/esp32sim/blob/7edb3bec/xtensa-lx7/src/jit/wasm_emit.rs#L1081), [region edges](https://github.com/aliceisjustplaying/esp32sim/blob/7edb3bec/xtensa-lx7/src/jit/wasm_region.rs#L207).

## Invariants to preserve

- **Window ownership:** CALL8 writes the caller's A8 and CALLINC but does not rotate the window. ENTRY must commit dirty caller locals before rotating WINDOWBASE, update WINDOWSTART, reload the new visible registers and install the new stack pointer. Keeping the old register locals across ENTRY would corrupt the callee. The existing ENTRY emitter already performs this sequence. [ENTRY](https://github.com/aliceisjustplaying/esp32sim/blob/7edb3bec/xtensa-lx7/src/jit/wasm_emit.rs#L962)
- **WINDOWS proof:** the entry-time collision mask describes one register window. It is not valid after ENTRY. Preserve its reload and the existing post-ENTRY region guard, which exits for ordinary instruction-by-instruction overflow handling rather than raising an early trap. Conservative rejection is acceptable; removing that guard is not. [Post-ENTRY guard](https://github.com/aliceisjustplaying/esp32sim/blob/7edb3bec/xtensa-lx7/src/jit/wasm_emit.rs#L686)
- **Retirement and interrupts:** retire CALL8 exactly once. The callee edge must retain the current budget/DIRTY check; a cut after CALL8 leaves PC at ENTRY with the caller's window still selected. Preserve CCOMPARE bounds and protected special-register boundaries. Keep the initial experiment out of the priced frontier path: changing dispatch boundaries also changes when accumulated timing extras are settled.
- **Code and observability:** include callee pages in version validation. A store affecting those pages must force an exit before stale callee code runs. Callee probes/stubs must still prevent internal entry; observed execution still disables regions. Preserve the existing region lifetime/epoch rules.
- **Return handling:** leave RETW unchanged, including illegal/underflow cases. A later internal-RETW experiment would need to validate the *computed* return PC, rotate/reload the caller's window and renew its collision proof. A remembered call site alone is not sufficient. Do not bundle the inconclusive standalone EX109 optimization into this first step. [Helper exit semantics](https://github.com/aliceisjustplaying/esp32sim/blob/7edb3bec/xtensa-lx7/src/jit/wasm_emit.rs#L431)

## Acceptance and stop rule

Add directed comparisons for dirty caller arguments/stack pointer, pre-call window overflow, post-ENTRY collision, illegal ENTRY/WOE state, nested calls/recursion, cuts before CALL8/at ENTRY/after ENTRY, timer delivery, callee probes and self-modifying callee code. Keep unsupported calls and all returns on their existing paths. Run the existing differential suite plus exact-work TinyDraw and pocket-tank comparisons.

Count actual dispatches and retired work, not merely disappearing call tags: a failed internal edge can become a budget exit, and a partial callee prefix can introduce another exit. Shared-callee coverage may also increase code size or conservative admission failures. Screen one isolated patch before refining it; park it if the measured gain is flat or negative. No cross-module chaining, return stack or new invalidation scheme is needed for this experiment.
