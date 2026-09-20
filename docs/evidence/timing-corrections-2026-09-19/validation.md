# Timing handoff completion

The reconciled handoff fixes passed combined validation at `da3b5752`. Its tree is identical to the published four-PR stack head `85715360`; later commits add documentation only. [Machine-readable receipts](validation.json).

| Scope | PR | Correction |
| --- | --- | --- |
| SPI, DMA and cache lifecycle | [#104](https://github.com/joakimeriksson/esp32sim/pull/104) | Correct lane/phase time, publish completion effects without restoring stale channel state, preserve STOP and cancel reset/rebind transfers. Keep DMA/host accesses unpriced and reset shared cache state at the proper lifecycle boundaries. |
| Instruction pricing | [#105](https://github.com/joakimeriksson/esp32sim/pull/105) | Price omitted load dependencies, preserve successful-store continuation state, align step/block execution and charge executed fetches with bounded multi-instruction batches. Cover dynamic indirect targets and keep default native JIT enabled. |
| Scheduler accounting | [#106](https://github.com/joakimeriksson/esp32sim/pull/106) | Separate instruction counts from cycle prices, including reset paths, prevent duplicate CPI charges and reject zero quantum. |
| Worker and timing API | [#107](https://github.com/joakimeriksson/esp32sim/pull/107) | Validate experiment lists before dispatch, reject invalid starts and enforce timing API prerequisites. Preserve workload identity and test actual built WASM exports in CI. |

Validation passed:

- 401 release workspace tests, including ignored tests except `external_`, with the installed ROM bundle.
- Strict workspace/all-target Clippy.
- Default production WASM build and actual-export timing API tests.
- Eight firmware manifests: hello, c3-hello, c6-hello, c6-energy-scan, c6-contiki, c6-contiki-net, c6-rpl-net and panel.
- 78,900 WASM differential cases with 76,388 modules released.
- Worker pacing/channel tests and 20 frontend tests.

The initial native run could not bind local sockets in the sandbox. The same tests passed with local socket permissions. Each PR's CI remains separate from these combined-tree results.

## Qualifications

The [native panel screen](../native-panel-qualification-2026-09-19/README.md) still showed slower execution in all three pairs against current #89, with matched guest work. Desktop activity limits precision; this is not native performance clearance. The [region rejection screen](../region-rejection-2026-09-19/pocket-screen.md) showed no gain and remains parked in #95. Neither experiment was adopted as a performance improvement.

Fetch pricing now bounds priced WASM calls to 64 instructions rather than one. Batching is retained, but performance neutrality is not established. Actual changes to executable bytes still use the normal block rebuild/reset path; the successful-store regression covers stores that preserve those bytes.

Hardware calibration was not rerun after these cycle-accounting corrections. Older hardware ratios do not certify the corrected model. The interrupted uncommitted pricing patch in system temporary storage was inaccessible under the tool policy and remains untouched; the tested implementation was reconciled from the available committed versions and current source.

Raw local logs are under `work/pr-sanity-0919/validation/`; their hashes and compact summaries are included in the machine-readable receipt.
