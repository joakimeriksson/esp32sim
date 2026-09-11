#!/bin/sh
# xterm.js for the page's Terminal tab (MIT, xtermjs/xterm.js). Not committed: this fetches it,
# pinned by version and SHA-256, into web/vendor/xterm — the Pages workflow runs it too. Without
# it the page still works; the Terminal tab just says to run this.
set -e
cd "$(dirname "$0")/.."
V=5.5.0
D=web/vendor/xterm
mkdir -p "$D"
get() { curl -sL --retry 5 --retry-all-errors --retry-delay 5 "$1" -o "$2"; }
get "https://cdn.jsdelivr.net/npm/@xterm/xterm@$V/lib/xterm.js" "$D/xterm.js"
get "https://cdn.jsdelivr.net/npm/@xterm/xterm@$V/css/xterm.css" "$D/xterm.css"
get "https://raw.githubusercontent.com/xtermjs/xterm.js/$V/LICENSE" "$D/LICENSE"
cat > "$D/SHA256SUMS" <<SUMS
1f991ac3b4b283ebf96e60ae23a00a52765dd3a2e46fa6fdda9f1aab032f7495  $D/xterm.js
ba8e6985669488981ccf40c0cefe3aba80722cb6c92de7ad628b0bd717faf2b6  $D/xterm.css
b569f629d00f2626a8100df2a1798210535621e42164dfd426a6fe5aac7b0ccd  $D/LICENSE
SUMS
if command -v shasum >/dev/null 2>&1; then shasum -a 256 -c "$D/SHA256SUMS"; else sha256sum -c "$D/SHA256SUMS"; fi
