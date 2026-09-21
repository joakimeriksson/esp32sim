# Native ARM64 panel qualification, September 19

**Performance clearance remains open.** With timing disabled, final integration `da3b5752` took a median **3.91170 seconds** versus **3.67942 seconds** for current PR #89 `5a1bba82`: 6.31% longer across three alternating pairs. Every pair was slower, but external desktop activity prevents treating that percentage as a controlled regression estimate. [Measurements and identities](summary.json) · [host samples](host-activity.txt).

| Pair | Order | PR #89 seconds | Final seconds | Final longer |
| --- | --- | --- | --- | --- |
| 1 | baseline, final | 4.24314 | 4.47472 | 5.46% |
| 2 | final, baseline | 3.65573 | 3.91170 | 7.00% |
| 3 | baseline, final | 3.67942 | 3.89325 | 5.81% |

All six arms retired core 0 **260,026,792** plus core 1 **136,442,769** instructions: **396,469,561** total. Each built 21,269/10,172 blocks with zero cache flushes and 31,441 JIT compilations. Console SHA-256 was `a77aaabb68350611617f518acdf4687b919906b7439ef327d816239c62b683d9` throughout. Generated code grew from 25,275 to 25,602 KiB. The harness initially required the entire JIT report to match and stopped after pair 1 because of that size difference. It was corrected to compare counts; the remaining two pairs resumed without repeating or dropping pair 1. [Harness](run.mjs) · [results](summary.json).

The workload follows the panel SID scenario in [bench-goldens.sh](../../../tools/bench-goldens.sh): ROM boot, waveshare-lcd4b, 16 MiB flash, 8 MiB PSRAM, fixed firmware/data/touch script and seven modeled seconds. This qualification captures USB console output rather than suppressing it. Approximate timing flags are absent, runtime `ESP32SIM_*` overrides are removed and native JIT must compile code. No independent image/audio equality check was performed. Exact arguments and all input/binary hashes are in [summary.json](summary.json).

Baseline was freshly built with the default release profile, rustc 1.98.1, no explicit features or Rust flags: [build log](baseline-build.log). The candidate is the release executable built and exercised by the final workspace/golden validation at `da3b5752`; it was copied before further builds and its SHA-256 checked with the validation agent. Both binaries are Mach-O arm64. `git diff da3b5752 85715360` was empty. Source changes span the combined timing/review stack, so no single mechanism is attributed.

All team builds/tests finished before timing. Host samples show Raycast at 61–126% CPU and a Codex renderer at 262.6% before pair 2's candidate. No unrelated process was stopped. Short runs and the slower first pair leave host noise and warm-up uncertainty. No extra campaign followed; the benchmark slot was released immediately.

The handed-off 10–12% result compared **original** PR #91 with **original** PR #89. Its reports reside in blocked system temporary paths and were not read or bypassed. This fresh comparison uses **current** PR #89 and the final integration; it cannot establish recovery from that older result. Record this dated qualification under existing EX027 (native regression/recovery), related to EX133/EX138–EX147, preserving earlier outcomes. No optimization was adopted from this run.

Local raw stdout/stderr and full process captures: `work/native-panel-qualification-0919/results`. Run the preserved harness with two exact binaries and a fresh output directory; it executes three alternating pairs and stops on changed work or console output.
