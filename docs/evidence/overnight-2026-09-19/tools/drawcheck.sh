#!/bin/bash
# usage: drawcheck.sh URL OUT.png
R=/Users/alice/src/a/esp32sim/work/night-run
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" --headless=new --no-first-run --no-default-browser-check --disable-background-timer-throttling --disable-renderer-backgrounding --remote-debugging-address=127.0.0.1 --remote-debugging-port=9240 --user-data-dir="$R/pagecheck/profile" about:blank >/dev/null 2>&1 & C=$!
sleep 3; node $R/bin/drawcheck.mjs "$1" "$2" 9240; kill $C; sleep 1; rm -rf "$R/pagecheck/profile"
