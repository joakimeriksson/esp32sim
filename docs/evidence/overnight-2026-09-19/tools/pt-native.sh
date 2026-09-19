#!/bin/bash
# usage: pt-native.sh <vq> <seconds> [extra flags]   -> prints final stats lines
VQ=$1; S=$2; shift 2
ESP32SIM_VQ=$VQ /Users/sarah/src/a/esp32sim/work/night/target/release/esp32sim --boot rom --rom /Users/sarah/src/a/esp32sim/web/wasm/fw/esp32s3_rev0_rom.elf --bootloader /Users/sarah/src/a/esp32sim/web/wasm/fw/public/pocket-tank-bootloader.bin --ptable /Users/sarah/src/a/esp32sim/web/wasm/fw/public/pocket-tank-ptable.bin --app /Users/sarah/src/a/esp32sim/web/wasm/fw/public/pocket-tank.bin --flash-at 0x290000=/Users/sarah/src/a/esp32sim/web/wasm/fw/local/model_q4.bin --board waveshare-amoled18-v2 --flash-mb 16 --psram-mb 8 --max-seconds $S "$@" 2>&1 | tail -6
