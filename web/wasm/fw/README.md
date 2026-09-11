# Firmware for the WebAssembly build

Put images here for `index.html?wasm&fw=<name>`, described by `<name>.json`:

```json
{ "board": "waveshare-lcd4b", "flash_mb": 16, "psram_mb": 8, "wifi": "ssid=esp32sim,psk=esp32sim-pass", "stubs": ["esp_wifi_start=0"],
  "files": { "rom": "esp32s3_rev0_rom.elf", "bootloader": "bootloader.bin", "ptable": "partition-table.bin",
             "app": "energy_panel.bin", "elf": ["energy_panel.elf"], "script": "sid.txt" },
  "flash_at": { "0x610000": "energydata.json" } }
```

`kind` names: `rom`, `bootloader`, `ptable`, `app`, `elf` (one or a list), `flash` (whole image),
`script`, `picture`. `flash_at` writes files into flash at hex offsets (a data partition's contents). `terminal: true` opens the page on the xterm.js terminal tab (UART0) instead of the plain console log. `stubs` are `NAME[=value]` function stubs — resolved through the ELF, or through a `symbols` map (`{"NAME": "0xaddr"}`) when no ELF is shipped; `wifi` an AP spec. `demos.json` lists the manifests the page offers as links. Everything else in this directory is git-ignored except `public/` (hello_world and the Atech firmware — our own code — and their manifests): the mask ROM
is Espressif's and firmware is whoever built it — host them only where you may.
