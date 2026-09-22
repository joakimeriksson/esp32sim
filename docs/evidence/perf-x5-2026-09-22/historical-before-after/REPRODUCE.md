# Reproduce the historical TinyDraw comparison

The compact verifier checks the preserved receipts without running firmware:

```sh
node docs/evidence/perf-x5-2026-09-22/historical-before-after/verify.mjs
```

To rerun timings, supply checkouts of the source revisions in [provenance.json](provenance.json), WASM artifacts built with its recorded settings and a TinyDraw asset map whose file hashes match the selected contracts. Firmware binaries and private raw captures are not included. Historical sources were rebuilt with rustc 1.98.1, `RUSTFLAGS=-Cllvm-args=-inline-threshold=2000`, release debug=0, strip=debuginfo and no wasm-opt. Artifact hashes identify the measured outputs; a rebuilt artifact with a different hash needs a fresh qualification contract before timing.

The retained browser runner accepts caller-supplied paths. Create a Python virtual environment with `uv venv "$VENV"`, then run:

```sh
uv run --python "$VENV/bin/python" --no-project python \
  "$EVIDENCE/harness/browser-harness/historical-pairs.py" "$NEW_OUTPUT" \
  --assets "$TINYDRAW_ASSET_MAP" --common-tree "$CURRENT_TREE" \
  --baseline-tree "$HISTORICAL_TREE" --baseline-wasm "$HISTORICAL_WASM" \
  --baseline-contract "$HISTORICAL_CONTRACT" \
  --candidate-tree "$CURRENT_TREE" --candidate-wasm "$CURRENT_WASM" \
  --candidate-contract "$CURRENT_CONTRACT" --chrome "$CHROME_BINARY" \
  --warmup-pairs 1 --pairs 4
```

The asset map has `workload: "tinydraw"` plus paths for `rom`, `bootloader`, `ptable`, `app` and `elf`. The retained contracts are `a2db66cf-tinydraw-contract.json`, `80c43cae-tinydraw-contract.json` and `current-tinydraw-contract.json` in the browser harness directory. The two-pair A/A job uses the current artifact/contract for both arms with `--warmup-pairs 0 --pairs 2`. The runner requires a fresh output directory and preserves browser profiles and raw logs there; those private files need curation before publication. Keep harness sources unchanged during a campaign because their hashes are captured per arm.

For a new artifact, the retained [Node pilot](harness/harness-v2/pilot.mjs) uses `--wasm`, `--workload tinydraw`, `--assets`, `--output` and explicit `--mode historical-interpreter` or `--mode current-jit`. It writes firmware results and the final published LCD frame as RGBA/RGB565. Its current-work-contract exit status can be nonzero for a historical source with different legitimate instruction/frame totals; inspect the firmware gates, image and capability metadata before qualifying that source. Never use this pilot's wall time as a browser measurement. The initial failed `31037b26` pilot used the earlier adapter without final-image capture; its no-frame timeout is preserved in [qualification.json](qualification.json).
