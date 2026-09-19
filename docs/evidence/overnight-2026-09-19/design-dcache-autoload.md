# ESP32-S3 data-cache autoload: observed off, not a justified default

**Research/design only; no model change.** The two emulated firmware boots do not support enabling automatic prefetch by default to explain pocket-tank's remaining timing gap.

## Observed registers

Native ROM boots, stopped after two guest seconds, using the frozen inputs and the existing timed-tree CLI:

| Register | Address | Pocket-tank | TinyDraw |
| --- | --- | ---: | ---: |
| DCACHE_CTRL | `0x600c4000` | `0x15` | `0x11` |
| DCACHE_AUTOLOAD_CTRL | `0x600c404c` | `0x08` | `0x08` |
| AUTOLOAD_SCT0_ADDR / SIZE | `0x600c4050` / `0x600c4054` | `0` / `0` | `0` / `0` |
| AUTOLOAD_SCT1_ADDR / SIZE | `0x600c4058` / `0x600c405c` | `0` / `0` | `0` / `0` |
| ICACHE_CTRL | `0x600c4060` | `0x0b` | `0x0b` |

`0x08` is DONE only: global autoload enable (bit 2) and both section enables (bits 0–1) are clear. Bit 2 of **DCACHE_CTRL** is a different field: cache size, not autoload. The emulator preserves written AUTOLOAD_CTRL bits and only ORs DONE on reads; it is not hiding an enable bit. Receipts: `autoload/pocket-peek.log`, `autoload/tinydraw-peek.log` and their `*-command.json` files beside this note. These are emulated snapshots, not physical-board readbacks or a claim about later firmware activity.

## What IDF actually does

The installed v6.1 checkout is `fff9895c82d744c7237be8847347bdd1b07c6643`. `cache_hal_init` snapshots the existing autoload state and passes it back when enabling/resuming caches; it does not unconditionally enable DCache autoload. The S3 octal-PSRAM setup explicitly calls `Cache_Resume_DCache(0)`. A source search found the ROM configuration/enable declarations, but no unconditional S3 C/C++ startup call to those autoload APIs. The observed firmware images identify themselves as IDF 5.4.1 (Pocket) and 6.0.2 (TinyDraw), so distinguish their observed state from the v6.1 source inspection.

Sources: [HAL preservation](https://github.com/espressif/esp-idf/blob/fff9895c82d744c7237be8847347bdd1b07c6643/components/hal/cache_hal.c#L48-L89), [S3 enable-bit test](https://github.com/espressif/esp-idf/blob/fff9895c82d744c7237be8847347bdd1b07c6643/components/hal/esp32s3/include/hal/cache_ll.h#L89), [octal PSRAM resume](https://github.com/espressif/esp-idf/blob/fff9895c82d744c7237be8847347bdd1b07c6643/components/esp_psram/esp32s3/esp_psram_impl_octal.c#L424).

AUTOLOAD_CTRL defines ascending/descending order in bit 4, miss/hit/both triggering in bits 6:5 (0/3 = miss, 1 = hit, 2 = both) and a two-bit size in bits 8:7. The ROM API encodes a step count as `count - 1`, allowing 1–4 blocks. Section registers describe virtual-address ranges. Current zero fields therefore encode ascending, miss-triggered, size code zero, but **none of that is active**. [Register definitions](https://github.com/espressif/esp-idf/blob/fff9895c82d744c7237be8847347bdd1b07c6643/components/soc/esp32s3/register/soc/extmem_reg.h#L281-L370), [ROM configuration types](https://github.com/espressif/esp-idf/blob/fff9895c82d744c7237be8847347bdd1b07c6643/components/esp_rom/esp32s3/include/esp32s3/rom/cache.h#L94-L155).

## Minimal conditional experiment, if hardware enables it

Honor the global/section enables, virtual range, direction and trigger mode. Only demand accesses trigger lookahead; prefetches must not recursively trigger more prefetches. A candidate rule requests the following configured number of lines, in the selected direction, through the same flash/PSRAM service resource. Queueing should not itself stall the CPU. Do not make a queued line an immediate cache hit: track completion, merge a demand for an in-flight line and charge any remaining wait. Account for bandwidth and any dirty eviction; stop at unmapped or non-memory boundaries.

That is a proposal, not fully established hardware behavior. The headers alone do not settle whether the count includes the triggering line, buffer allocation/replacement policy, demand priority or exact overlap. Confirm those with enabled/disabled hardware counter tests and several consumer delays before adopting a rule. First read these registers on the matching physical firmware. With the observed controls unchanged, the correct first implementation would remain **off**, so it would not explain the current Pocket gap by itself.
