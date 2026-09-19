# EX136 rejection shortcut: Pocket Tank confirmation

The shortcut was slower in **all six** matched pairs across two three-pair campaigns: median **77.95516 → 79.59927 host seconds** (2.11% slower) and **78.81516 → 81.85644** (3.86% slower). With the [earlier one-pair screen](pocket-screen.md), that is seven pairs and no win. **Rejected; PR #95 closed.** The host was busy during both campaigns, so the direction is supported and the percentages are not controlled estimates. [Campaign 1](pocket-confirm-1.json) · [Campaign 2](pocket-confirm-2.json).

| Campaign | Pair | Order | Baseline seconds | Candidate seconds | Candidate longer |
| --- | --- | --- | --- | --- | --- |
| 1 | 1 | baseline, candidate | 79.49361 | 79.59927 | 0.13% |
| 1 | 2 | candidate, baseline | 77.12087 | 79.88880 | 3.59% |
| 1 | 3 | baseline, candidate | 77.95516 | 79.53141 | 2.02% |
| 2 | 1 | baseline, candidate | 78.68158 | 81.85644 | 4.04% |
| 2 | 2 | candidate, baseline | 78.81516 | 81.04359 | 2.83% |
| 2 | 3 | baseline, candidate | 79.00435 | 86.82952 | 9.90% |

Exact comparison: baseline `bf36479b5c9b905447a51fb97fd9fd09564efcc5` (merge base of PR #95 with its target), candidate `5fd61b699bed18b43f412af52cde8277dfc867cf` (PR head). `git diff --stat` between them is the eight added lines in `xtensa-lx7/src/jit/wasm.rs` and nothing else. Both campaigns used the same two binaries: baseline SHA-256 `08829fcfb3cd423cdd8943feebe7990d0a5818a66130c5d8521bde3ed6e01404`, candidate `ba19ee4035bc330762395506f4f12fe5553e6c98e3f032fc51b990d7ed73722c`. Clean trees (empty `source.patch` in every arm).

**These binaries differ from the earlier screen's.** `run-pairs.py` built both trees itself with `cargo build --release --target wasm32-unknown-unknown -p esp32sim-wasm`, rustc 1.98.1, no features, no Rust flags and **no wasm-opt pass**. The earlier screen reused production artifacts that had been through wasm-opt `-O3`. The shortcut lost under both build recipes. [Campaign 1 builds](pocket-confirm-1-build.json) · [Campaign 2 builds](pocket-confirm-2-build.json).

All twelve arms completed 30.000000229 modeled guest seconds and 10,073,833,775 instructions, passed the harness checks, reported zero JIT failures and 72,158,197 JIT bytes and produced console SHA-256 `9e8a66e483f741c80db1bd20c62bc43943697c2e6a0053a6af1f7d52af8c67dd`, the same hash as the earlier screen. Chrome 153.0.8010.53, V8 15.3.76.13. No pixel equality check was performed.

Do not call either campaign uncontended. Host: Apple M1 Pro, 10 cores, desktop in use.

- Campaign 1 (18:20–18:28): load average 6.37 before with Spotlight `mds_stores` at 32.4% CPU; 8.30 after with `fseventsd` 38.4% and `mediaanalysisd` 28.6%. [Before](pocket-confirm-1-host-before.txt) · [after](pocket-confirm-1-host-after.txt).
- Campaign 2 (19:36–19:45), after Dropbox sync was paused: one-minute load average **40.47** at start with `duetexpertd` at 51.5%, so earlier heavy activity had only just ended; 9.66 after with a browser renderer at 33.0% and `fseventsd` 27.1%. The 86.83 s candidate arm in pair 3 is probably host interference; without it the campaign 2 pairs were 2.8–4.0% slower. [Before](pocket-confirm-2-host-before.txt) · [after](pocket-confirm-2-host-after.txt).

Host samples were taken only before and after each campaign, not per arm. Order alternated within each campaign, so order alone does not explain a candidate that lost every pair.

No mechanism was profiled. The shortcut adds an epoch, budget and boundary test ahead of the existing hot-entry test on every dispatch that reaches it, plus a page-version scan when that test rejects; whether that cost, code layout or something else accounts for the loss was not investigated. A retry should differ materially (for example, showing by profile that rejected cached entries are frequent on the target workload) rather than rerun this patch.

Raw captures remain locally at `work/pr95-bench-runs/20260919-182021` and `work/pr95-bench-runs/20260919-193639` from the repository root. Reproduce from the candidate checkout with:

```sh
uv run --no-project --python .venv/bin/python tools/browser-benchmark/run-pairs.py <fresh-out-dir> \
  --baseline-tree <checkout of bf36479b> --candidate-tree <checkout of 5fd61b69> \
  --assets web/wasm/fw/local/pocket-tank-assets.json --pairs 3
```

This extends existing [EX136](../../experiments.md#ex136); earlier results stand as recorded.
