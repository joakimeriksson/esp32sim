# Reproducing the M1 comparison

Use the artifact and asset identities in [provenance](provenance.json), the [retained source manifest](harness-provenance.json) and [conditions](conditions.json). This was a direct pre-x4 versus post-x5 runtime comparison on an in-use machine. Compilation overlapped part of Chrome; later snapshots showed zero compiler processes without proving the machine idle. Chrome started on battery. The selected Safari restart began on AC power at 28%; the earlier battery-powered Safari attempt was interrupted after two warm-up arms and excluded. Browser differences are not an isolated engine comparison.

Reconstruct the baseline by checking out public revision `0042d053` and applying [the preserved baseline patch](baseline-from-main.patch). The measured label `7828e683` is not guaranteed fetchable. The expected reconstructed tree is `c40ce2f3ef8e734b4f3c98bab0e4fae822512b58`. Build both revisions with inline threshold 2000, release DEBUG=0 and STRIP=debuginfo, without wasm-opt. Compare artifact hashes before timing. [Build identities](provenance.json)

Set `SCRATCH` to a new persistent directory, `TREE` to a compatible Git checkout and `ASSETS` to a caller-created asset map for one workload. Set `BEFORE` and `AFTER` to the recorded WASM files. Create a virtual environment if absent, then run one Chrome comparison:

```sh
export TMPDIR="$SCRATCH" TMP="$SCRATCH" TEMP="$SCRATCH"
[ -d "$SCRATCH/venv" ] || uv venv "$SCRATCH/venv"
uv run --offline --no-project --python "$SCRATCH/venv/bin/python" python harness/chrome/run-pairs.py "$SCRATCH/chrome-primary" \
  --baseline-tree "$TREE" --candidate-tree "$TREE" \
  --baseline-wasm "$BEFORE" --candidate-wasm "$AFTER" --assets "$ASSETS" --pairs 4
```

For the complete ordering, run Pocket warm-up (1 pair), Pocket A/A control (2 pairs, BEFORE for both arms), Pocket primary (4 pairs), then the same three jobs for TinyDraw. Keep all warm-up and control results separate from primary estimates.

The Safari server expects a caller-created campaign directory containing `artifacts/before/main.wasm`, `artifacts/after/main.wasm`, their adjacent `build.json` files and `assets-pocket.json` / `assets-tinydraw.json` maps. Each map contains `workload` plus asset-role paths: Pocket uses rom, bootloader, ptable, app and model; TinyDraw uses rom, bootloader, ptable, app and elf. Every input must match the recorded asset hashes.

```sh
uv run --offline --no-project --python "$SCRATCH/venv/bin/python" python harness/safari/server.py \
  --campaign-root "$CAMPAIGN" --tree "$TREE" --out "$SCRATCH/safari-results"
```

Open the locally printed URL in Safari and keep that page visible. The server rejects hidden-page arms and automatically performs all 28 arms. Do not publish its token URL or raw server log. The retained Safari entry point uses an ordinary browser page; the abandoned WebDriver attempt generated no timing arms. [Attempt and power history](conditions.json)
