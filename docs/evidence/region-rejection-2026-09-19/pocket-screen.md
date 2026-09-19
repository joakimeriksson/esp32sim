# EX136 rejection shortcut: Pocket Tank screen

The shortcut showed no gain: **80.40428 → 81.13610 host seconds**, 0.91% slower in one baseline-then-candidate pair. Keep it parked; this is not evidence of a repeatable regression. [Measured results](pocket-screen.json).

Exact comparison: base `bf36479b5c9b905447a51fb97fd9fd09564efcc5`, candidate `5fd61b699bed18b43f412af52cde8277dfc867cf`. Reused default production artifacts: baseline SHA-256 `468a47ee89a6274cff24eb8f2669648d327b0f7d41e1fff7681ba1c071b7ce80`, candidate `420ddf351c6181b303c980da1d8afc77e9bdf3e3ebe274502e7c93fe157feb73`. Since the recorded build source revisions, Rust changes are confined to CLI test assertions; the WASM runtime sources match. [Baseline build](../review-2026-09-19/production-rebuild.json) · [Candidate build](build.json).

Both arms completed 30.000000229 modeled guest seconds and 10,073,833,775 instructions with 3,094 counted binary messages, console SHA-256 `9e8a66e483f741c80db1bd20c62bc43943697c2e6a0053a6af1f7d52af8c67dd` and zero JIT failures. Binary-message counts do not prove pixel equality. Browser, V8, firmware hashes and harness provenance are in the [result receipt](pocket-screen.json).

Team builds/tests finished before this pair. External activity remained: Raycast was 123.3% CPU in the [before sample](pocket-host-before.txt), alongside desktop/browser activity; see also the [after sample](pocket-host-after.txt). Do not call this uncontended. An earlier incomplete attempt overlapped another agent's build, was interrupted and contributes no timing result. One fresh pair was the requested bounded screen; no confirmation campaign followed.

Raw captures remain at `work/pr95-measurement-runs/screen`; reproduction uses `tools/browser-benchmark/run-pairs.py` with `--pairs 1`, the exact trees/artifacts above and `web/wasm/fw/local/pocket-tank-assets.json`, through `uv run --no-project --python .venv/bin/python`. All task benchmarks stopped after this pair. This extends existing EX136; the canonical catalog should retain the original correctness result and add this screen as unadopted.
