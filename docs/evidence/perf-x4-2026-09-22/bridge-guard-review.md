# Bridge admission guard coverage

The [PR #131 review](https://github.com/joakimeriksson/esp32sim/pull/131#issuecomment-5778012407) found missing regression coverage for EX171's in-wrapper interpreted blocks. Tests based on `c3849da2` now cover instruction credit, CCOMPARE deadlines, pricing exclusion and memory/control instruction exclusion. The final test revision is `eee05f6b`, following `aaa39d2c` and `41e06d86`. This changes tests and a test-only cache accessor; production execution behavior is unchanged. No performance measurement was made.

At `eee05f6b`, `tools/wasm-jit-test.sh` passed **79,776 differential cases** with **82,400 compiled modules released** in Node.js v26.9.0. The [wrapper fixture](../../../xtensa-lx7/src/jit/wasm_tests/scheduler.rs) warms a compiled predecessor and installs a decoded successor. It checks a budget ending inside that successor, a timer deadline at the same position, enabled control pricing and successful unpriced bridging. Execution is compared with `step()`. The [class table and bridge oracle](../../../xtensa-lx7/src/jit/wasm_tests/control.rs) check all 19 current `word_access` operations plus WSR/XSR/RSIL and compare bridge execution with the decoded `run_block` path.

The following mutations were tested individually while constructing this follow-up. Restore each mutation before applying the next and run `tools/wasm-jit-test.sh` each time.

| Mutation | Observed failure |
| --- | --- |
| In `jit/wasm.rs`, replace `bridge_target(pc, bus.page_versions(), budget - sofar)` with `bridge_target(pc, bus.page_versions(), 0xffff)` | `bridge exceeded caller budget` |
| Same mutation, running fixture modes in `[1, 0, 2, 3]` order so CCOMPARE is checked first | Scheduler returned `None`; single-step oracle returned `Some(Interrupt(6))` |
| In `block.rs`, change `BRIDGE_CLASS` from 2 to 3 | `memory admitted: L32i` |
| In `jit/wasm.rs`, remove `&& !cpu.price_control` from bridge admission | Final isolated fixture: timing extras were 82 versus the oracle's 87, losing five cycles |

Paths in the table are relative to `xtensa-lx7/src`. The pricing test requires isolation: priced JX uses a helper, whose `jit_helped` flag independently blocks bridging. That initial fixture let the pricing mutation survive. Replacing JX with direct J allowed region formation and made the unmutated fixture fail too. The final fixture warms observed single blocks, then disables region attempts before checking wrapper admission. Its unmutated suite passes and removing the pricing guard fails as reported above. The cache accessor exists only under `wasm32` with `wasm-jit-tests`.

The existing diagnostic limitation remains: traps and SIMCALLs reached after chaining are reported using the dispatch-start PC, as explicitly documented by [`observe_execution`](../../../esp-soc/src/machine.rs). This review preserves that established machine contract; accurate fault-site reporting would require a separate cross-core trap-location change.

This receipt retains commands, revisions, counts and failure signatures. Raw build logs and local environment paths are omitted; no measured values were redacted. `node tools/check-evidence-privacy.mjs` passed across 1,165 tracked evidence files, including 15 gzip files.
