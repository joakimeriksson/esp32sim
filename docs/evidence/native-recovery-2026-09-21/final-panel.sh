#!/bin/bash
set -euo pipefail
cd "$HOME/bench/esp32sim/overnight-review"
for i in $(seq 1 240); do
 if test -f DIRECT_MAIN_BROWSER_DONE; then
  node screen-panel.mjs "$PWD/baseline/target/release/esp32sim" "$PWD/recovery/target/release/esp32sim" confirm-panel-recovery 2 > logs/confirm-panel-recovery.txt 2>&1
  touch PANEL_RECOVERY_DONE
  exit 0
 fi
 sleep 10
done
exit 1
