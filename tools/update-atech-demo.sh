#!/bin/sh
# Rebuild the Atech demo images from the firmware repo and re-pin the goldens.
#
# The firmware is its own project (github.com/joakimeriksson/atech-firmware); this repo carries
# the images it produces, under web/wasm/fw/public. Keeping the two in step means three things
# that are easy to do two of: copy the images, pin the commit they came from, and regenerate the
# goldens they move. This does all three, then checks the new goldens actually hold.
#
#   tools/update-atech-demo.sh [path-to-atech-firmware] [app]
#
# Needs platformio for the build and the S3 mask ROM ELF for the goldens.
set -e
cd "$(dirname "$0")/.."
REPO=${1:-../atech-firmware}
APP=${2:-pocket-synth}
ROMS=${ESP32SIM_ROM_DIR:-$HOME/.espressif/tools/esp-rom-elfs/20241011}
[ -d "$REPO" ] || { echo "no firmware repo at $REPO" >&2; exit 1; }

# a pin is only worth having if it identifies the tree that was built, so check before building
git -C "$REPO" diff --quiet HEAD || {
  echo "$REPO has uncommitted changes: commit them first, or the pinned commit is a lie" >&2; exit 1; }
COMMIT=$(git -C "$REPO" rev-parse --short HEAD)

( cd "$REPO" && make dist APP="$APP" >/dev/null )

CHANGED=no
for f in bootloader:atech-bootloader ptable:atech-ptable firmware:atech-firmware; do
  src="$REPO/dist/$APP/${f%%:*}.bin"; dst="web/wasm/fw/public/${f##*:}.bin"
  if cmp -s "$src" "$dst"; then
    echo "  ${f##*:}.bin unchanged"
  else
    cp "$src" "$dst"; CHANGED=yes; echo "  ${f##*:}.bin updated"
  fi
done

# every manifest that loads these images names the commit they were built from
for m in web/wasm/fw/atech.json web/wasm/fw/atech-sid.json; do
  grep -q '"_source"' "$m" || { echo "$m has no _source to pin" >&2; exit 1; }
  perl -0pi -e "s{(\"_source\": \"https://github\.com/joakimeriksson/atech-firmware \@ )[0-9a-f]+}{\${1}$COMMIT}" "$m"
done
echo "  manifests pin $COMMIT"

[ "$CHANGED" = yes ] || { echo "images already current; nothing to regenerate"; exit 0; }

echo "regenerating the atech goldens..."
UPDATE_GOLDENS=1 ESP32SIM_ROM_DIR="$ROMS" \
  cargo test --release -p esp32sim --test goldens -- --include-ignored atech >/dev/null
echo "checking the new goldens hold..."
ESP32SIM_ROM_DIR="$ROMS" cargo test --release -p esp32sim --test goldens -- --include-ignored atech >/dev/null
git --no-pager diff --stat -- tests/golden web/wasm/fw | sed 's/^/  /'
echo "review the golden moves, then commit."
