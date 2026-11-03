# IDF sd_card (SDMMC) with long sector transfers, for the ESP32-P4

`../sd_card-p4` (ESP-IDF v5.5.3's `storage/sd_card/sdmmc`, public domain or CC0) with one block
added after the card's properties are printed, and `esp_timer` added to `main`'s requirements.
The block moves raw sectors with `sdmmc_write_sectors` and `sdmmc_read_sectors`:

- 64 and 128 sectors (32 and 64 KiB) from 64-byte-aligned `MALLOC_CAP_DMA` buffers: CMD25 and
  CMD18 with the host's auto-stop, longer than the 16 KiB the driver's four DMA descriptors hold,
  so the driver refills them from its interrupt handler;
- 64 sectors into a buffer 4 bytes off, which the driver splits into single-block reads through
  a bounce buffer;
- 2 sectors from the card's last one, which the driver refuses before the card sees it
  (`ESP_ERR_INVALID_SIZE`, 0x104);
- and the card status (CMD13).

It prints each result and the time the first write and read took. Built with
`idf.py set-target esp32p4 build`, as `web/wasm/fw/public/p4-sdsectors-bootloader.bin` and
`p4-sdsectors.bin` (pinned and fetched by `tools/fetch-p4-firmware.sh`), with hello_world's
partition table (the same bytes). The
`sdcard-sectors-p4` golden runs it with a card that programs for 2 ms after each write
(`--sd-prg-us 2000`), so the write's time includes the driver polling CMD13 until the card is
ready:

    ../../target/release/esp32sim-p4 --flash-mb 2 \
      --bootloader build/bootloader/bootloader.bin --ptable build/partition_table/partition-table.bin \
      --app build/sd_card.bin --sd-rw card.img --sd-prg-us 2000 --max-seconds 1

See ../../docs/esp32p4.md, "SD card".
