#!/bin/bash
set -euo pipefail
ROOT="$HOME/bench/esp32sim"
cd "$ROOT"
mkdir .campaign.lock
trap 'rc=$?; echo "$(date -u) Admission campaign exit=$rc"; rmdir "$ROOT/.campaign.lock"' EXIT
if pgrep -f 'run-pairs.py|capture-battery.mjs|cargo build|cargo test|exact.mjs|wasm-jit-test.mjs|Google Chrome.app/Contents/MacOS|Safari.app/Contents/MacOS' >/dev/null; then
  echo 'Another build, benchmark or interactive browser is running; refusing overlap.' >&2
  exit 1
fi
mkdir -p logs runs
export RUSTUP_TOOLCHAIN=1.98.1
RUSTC="$(rustup which rustc)"; export RUSTC
export DYLD_FALLBACK_LIBRARY_PATH="$(dirname "$(dirname "$RUSTC")")/lib"
CARGO="$(rustup which cargo)"
unset CARGO_ENCODED_RUSTFLAGS
export RUSTFLAGS='-Cllvm-args=-inline-threshold=4000'
name=combined-inl4000-admission-borrow
printf '%s\n' "$RUSTFLAGS" > "logs/$name.rustflags"
echo "$(date -u) Building and checking $name"
cd "$ROOT/admission-confirm"
"$CARGO" test --locked --offline -j4 -p esp-soc -p esp32s3 -p xtensa-lx7 > "$ROOT/logs/$name-native.log" 2>&1
"$CARGO" build --locked --offline -j4 --release --target wasm32-unknown-unknown -p esp32sim-wasm > "$ROOT/logs/$name-build.log" 2>&1
cp target/wasm32-unknown-unknown/release/esp32sim_wasm.wasm "$ROOT/wasm/$name.wasm"
shasum -a 256 "$ROOT/wasm/$name.wasm" > "$ROOT/logs/$name.sha256"
"$CARGO" build --locked --offline -j4 --release --target wasm32-unknown-unknown -p esp32sim-wasm --features jit-tests > "$ROOT/logs/$name-jit-build.log" 2>&1
node tools/wasm-jit-test.mjs > "$ROOT/logs/$name-jit-tests.log" 2>&1
cd "$ROOT"
TREE="$ROOT/admission-confirm" node exact.mjs "$ROOT/wasm/$name.wasm" > "logs/$name-exact.log" 2>&1
unset RUSTFLAGS
run_pair() {
  local run="$1" candidate="$2" tree="$3" pairs="$4" workload="$5"
  if pgrep -f 'Google Chrome.app/Contents/MacOS|Safari.app/Contents/MacOS' >/dev/null; then echo 'Interactive browser running; refusing benchmark'; exit 1; fi
  echo "$(date -u) Starting $run ($pairs pairs, $workload; baseline combined-inl4000)"
  uv run --offline --no-project --python "$ROOT/.venv/bin/python" python base/tools/browser-benchmark/run-pairs.py \
    "$ROOT/runs/$run" --baseline-tree "$ROOT/combined" --candidate-tree "$ROOT/$tree" \
    --baseline-wasm "$ROOT/wasm/combined-inl4000.wasm" --candidate-wasm "$ROOT/wasm/$candidate.wasm" \
    --assets "$ROOT/assets/$workload/assets.json" --pairs "$pairs" > "$ROOT/logs/$run.log" 2>&1
  node -e 'const d=require(process.argv[1]); console.log(JSON.stringify({run:process.argv[1],reduction:d.wallReductionPercent,pairs:d.pairsWallReductionPercent}));' "$ROOT/runs/$run/summary.json"
}
run_pair admission-control-aa combined-inl4000 combined 2 pocket-tank
run_pair "$name" "$name" admission-confirm 4 pocket-tank
if node -e 'const d=require(process.argv[1]); process.exit(d.wallReductionPercent>0 && d.pairsWallReductionPercent.every(v=>v>0)?0:1)' "$ROOT/runs/$name/summary.json"; then
  run_pair "$name-tinydraw" "$name" admission-confirm 3 tinydraw
else
  echo 'Admission did not improve consistently; skipping TinyDraw and retaining current artifact.'
fi
echo "$(date -u) Admission campaign complete"
