# Native regression fix and reproduction evidence

This follow-up removes native execution overhead introduced by PR48's correctness fixes: repeated helper-table construction, code-cache moves during compiled execution, and full interrupt-source scans after device ticks that changed no source. Each bus type reuses one static helper table instead of rebuilding it during execution; native execution copies its executable address before borrowing the CPU. Explicit notifications cover clocked devices, DMA completion, host input and both GPIO level transitions. GPIO output queues now visit only changed valid pins, preserving their order. Precise idle deadlines and the corrected instruction semantics remain in place.

The final confirmation compared complete CLI processes against main, using two warmup pairs followed by twelve measured pairs for each scenario. Every pair alternated which version ran first. All warmups and measured outputs are retained.

| Scenario | Main median seconds | Fixed median seconds | Main / fixed wall-time ratio |
| --- | ---: | ---: | ---: |
| Atech script1 | 2.593971 | 2.590839 | 1.001209 |
| SID jukebox | 2.412805 | 2.439853 | 0.988914 |
| Panel SID | 3.552067 | 3.578104 | 0.992723 |

A ratio above 1 means the fixed version is faster. These medians are within about 1.1% of main. All three scenarios met the predefined practical check: fixed median wall time no more than 2% above main, with at least ten of twelve pairs less than 5% slower. Each scenario had eleven such pairs. Individual slow pairs remain in the records; this check does not establish statistical or universal performance equivalence.

The timing runs disable console output and require exact validated per-core instruction counts. Main and the fixed version have known semantic count differences. Complete-process ratios therefore measure the same fixed scenario, without asserting identical instruction-throughput denominators. Separate native golden tests verify console, audio and instruction expectations; no golden values changed for this follow-up.

Final validation passed 198 native cases with zero failures or ignored cases and seven external-input cases filtered, 38,681 actual-WASM differential cases, native/WASM release Clippy, and five WASM firmware demos. The archive preserves earlier environmental permission failures and their successful final reruns.

Source main is `563fecd49e1532fb3e6e833e8f1e50e514756521`. Apply the archive's `gpio/selected.patch` to reviewed PR48 revision `335d0786333390cf40f50084b8ce7b151cf75089` to reproduce the selected 12-file change. The archive also includes the earlier screening candidates and their exact construction order.

The [native evidence archive](https://github.com/aliceisjustplaying/esp32sim/releases/download/native-finish-evidence-2026-09-05/native-finish-evidence-2026-09-05.tar.gz) contains all 312 process captures across five campaigns, candidate patches, compact summaries, validation logs and reproduction scripts. It is 185,671 bytes, SHA-256 `664ccd9474b8fb07b349fff5972dbb1f5071d81733238f524c91f0bb0cabdd0e`. Account identifiers are redacted; manifest hashes identify the distributed bytes and preserve original hashes separately. Binaries and full source/build trees are excluded.

Extract the archive and follow `native-finish-evidence/REPRODUCE.md`. The distributed runners use environment settings for the repository and ROM paths, with adaptations and hashes retained. Diagnostic CPU samples are labeled separately: five host seconds of sampling from intentionally interrupted processes, not completed timing comparisons. Rebuilt executables may have different hashes and constitute a new measurement.
