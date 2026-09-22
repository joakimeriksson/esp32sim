# Firefox reproduction

Use the same frozen artifacts and input hashes as the existing [campaign reproduction guide](REPRODUCE.md). The Firefox adapter uses an ordinary visible browser page; it does not use WebDriver or change browser automation permissions. It starts a fresh page and worker per arm, while the browser process and caches may persist. [Source identities](firefox-harness-provenance.json)

Supply a persistent scratch directory `SCRATCH`, a compatible Git checkout `TREE` and a campaign directory `CAMPAIGN` with the recorded `artifacts/{before,after}/main.wasm`, adjacent build metadata and `assets-{pocket,tinydraw}.json` maps. Create the virtual environment if absent.

```sh
export TMPDIR="$SCRATCH" TMP="$SCRATCH" TEMP="$SCRATCH"
[ -d "$SCRATCH/venv" ] || uv venv "$SCRATCH/venv"
uv run --offline --no-project --python "$SCRATCH/venv/bin/python" python harness/firefox/server.py \
  --campaign-root "$CAMPAIGN" --tree "$TREE" --out "$SCRATCH/firefox-results" --port 8794
```

Open the locally printed URL in Firefox and keep the page visible. Hidden-page arms fail. The server performs one warm-up pair, two A/A pairs and four before/after pairs for Pocket, then TinyDraw: six jobs and 28 arms. Retain every arm and keep warm-ups/controls separate from primary comparisons. Do not publish the token URL or raw server log.

The initial attempt's adapter metadata rejection is preserved separately from the selected restarted campaign. Firefox ran after Chrome and Safari; power/load snapshots and capture-method differences prevent an isolated engine comparison. [Conditions and excluded attempts](conditions.json)
