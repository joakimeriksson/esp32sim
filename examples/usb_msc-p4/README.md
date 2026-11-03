# IDF tusb_msc (SD card) for the ESP32-P4

The ESP-IDF v5.5.3 `peripherals/usb/device/tusb_msc` example (Unlicense or CC0-1.0, as its
sources say) for the ESP32-P4 silicon rev < 3 (v1.x), unchanged except for `sdkconfig.defaults`,
which adds the target, the revision and `CONFIG_EXAMPLE_STORAGE_MEDIA_SDMMC=y`. Built with
`idf.py set-target esp32p4 build`; `dependencies.lock` pins the managed components it pulls in,
espressif/esp_tinyusb 2.4.0 and espressif/tinyusb 0.21.0~2. Its P4 defaults are the reference
board's: the card on slot 1 through the GPIO matrix at CLK 43, CMD 44, D0-D3 39-42, four lines,
powered by on-chip LDO channel 4; TinyUSB on the high-speed port (rhport 1) in DMA mode, an
8 KiB MSC buffer; the console on UART0.

The bootloader, partition table and app are `web/wasm/fw/public/p4-usbmsc*.bin`, pinned and
fetched by `tools/fetch-p4-firmware.sh` (licences in `p4-usbmsc-NOTICE.txt`, committed). They are the input of the `usbmsc-p4` golden, which runs
them on a FAT32 card image the test builds (`tests/sd_image.rs`) with a host on the USB port
(`host.txt`) and a few lines typed at the console (`console.txt`):

    ../../target/release/esp32sim-p4 --flash-mb 4 \
      --bootloader build/bootloader/bootloader.bin --ptable build/partition_table/partition-table.bin \
      --app build/tusb_msc.bin --sd-rw card.img --script console.txt --usb-host host.txt --max-seconds 2

The example presents the card over USB mass storage. When the host configures it, esp_tinyusb
takes the card from the application and exposes it; an eject (START STOP UNIT with LoEj) gives
it back, and the console's `expose` command takes it again. While the application owns the card
the host's TEST UNIT READY fails with MEDIUM NOT PRESENT (sense 2/3A/00). See
../../docs/esp32p4.md, "USB".

`faults.txt` (the `usbmsc-p4-faults` golden, `--usb-host faults.txt` without `--script`) is a host
meeting faults: a command that times out mid-transfer and the port reset Linux recovers with, a
port reset after an eject, which takes the card back from the application (SET_CONFIGURATION runs
`tud_mount_cb`), and VBUS lost while a file is written, which gives it to the application
(`tud_umount_cb`) and leaves the file at size 0.
