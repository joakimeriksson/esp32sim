#!/bin/sh
# pocket-tank: the benchmark workload for language-model inference on PIE (docs/speed-plan.md).
# mediacutlet/pocket-tank (MIT) runs a 4-bit transformer on the Waveshare ESP32-S3-Touch-AMOLED-1.8
# (board waveshare-amoled18-v2). Its four flash parts are not committed: this fetches them into
# web/wasm/fw/local/, pinned by SHA-256, where the pocket-tank manifest, tools/bench.py and the
# browser benchmark find them. Files already present with the right hash are kept.
#   bootloader, partition table, app: the project's browser installer, manifest version 8ea6436
#   model: the repository at 5cc33b1 (the installer serves identical bytes)
set -e
cd "$(dirname "$0")/.."
D=web/wasm/fw/local
mkdir -p "$D"
INSTALLER=https://stratobuilds.com/pocket-tank-installer/firmware
MODEL=https://raw.githubusercontent.com/mediacutlet/pocket-tank/5cc33b1adf4076315b805e94129b295570a45b39/model/out/model_q4.bin
if command -v shasum >/dev/null 2>&1; then sum() { shasum -a 256 "$1" | cut -d' ' -f1; }
else sum() { sha256sum "$1" | cut -d' ' -f1; }; fi
get() {   # URL FILE SHA256
  if [ -f "$D/$2" ] && [ "$(sum "$D/$2")" = "$3" ]; then echo "ok (present) $2"; return; fi
  curl -sfL --retry 5 --retry-all-errors --retry-delay 5 "$1" -o "$D/$2.part"
  got=$(sum "$D/$2.part")
  if [ "$got" != "$3" ]; then rm -f "$D/$2.part"; echo "SHA-256 mismatch for $2: got $got" >&2; exit 1; fi
  mv "$D/$2.part" "$D/$2"
  echo "ok $2"
}
get "$INSTALLER/bootloader.bin"      pt-bootloader.bin 9556d59f3a10f6b04fa6d75f3142a66034339afafffbd4bbc3d9a127eaa73859
get "$INSTALLER/partition-table.bin" pt-ptable.bin     7af5b28608c91167e00778b7f20622f3b47849efebeb3e64117162d73f3e0508
get "$INSTALLER/pocket_tank.bin"     pocket_tank.bin   443dd24aed14a509e6df597eb3a02635cc07e0fbee6f04db85a62f4d4dadb113
get "$MODEL"                         model_q4.bin      b200bf87c78dc85c4d727f68941ac0b83cced9827037cc169cd2122d71002c2a

# The asset map for tools/browser-benchmark (run-pairs.py --assets, serve.py): absolute paths,
# with the mask ROM from ESP32SIM_ROM_DIR, the page's copy, or the newest esp-rom-elfs install.
ROM=""
for c in "${ESP32SIM_ROM_DIR:+$ESP32SIM_ROM_DIR/esp32s3_rev0_rom.elf}" web/wasm/fw/esp32s3_rev0_rom.elf; do
  if [ -n "$c" ] && [ -f "$c" ]; then ROM="$c"; break; fi
done
[ -n "$ROM" ] || ROM=$(ls -d "$HOME"/.espressif/tools/esp-rom-elfs/*/esp32s3_rev0_rom.elf 2>/dev/null | sort | tail -1)
if [ -n "$ROM" ]; then
  ABS=$(pwd)
  case "$ROM" in /*) ;; *) ROM="$ABS/$ROM" ;; esac
  cat > "$D/pocket-tank-assets.json" <<JSON
{"workload": "pocket-tank", "rom": "$ROM", "bootloader": "$ABS/$D/pt-bootloader.bin", "ptable": "$ABS/$D/pt-ptable.bin", "app": "$ABS/$D/pocket_tank.bin", "model": "$ABS/$D/model_q4.bin"}
JSON
  echo "wrote $D/pocket-tank-assets.json for tools/browser-benchmark"
else
  echo "no esp32s3_rev0_rom.elf found: set ESP32SIM_ROM_DIR to also write the browser-benchmark asset map" >&2
fi
