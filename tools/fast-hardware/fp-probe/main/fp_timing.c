/* Small derivative of TinyDraw calibration/esp32s3-core-timing's warmed IRAM
 * issue-block experiment. All CCOUNT windows exclude interrupts and printing. */
#include <inttypes.h>
#include <stdint.h>
#include <stdio.h>
#include "esp_attr.h"
#include "esp_chip_info.h"
#include "esp_rom_sys.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"

#define DECLARE(name) extern uint32_t name(void)
DECLARE(fp_empty); DECLARE(fp_add_dep); DECLARE(fp_add_ind);
DECLARE(fp_mul_dep); DECLARE(fp_mul_ind); DECLARE(fp_madd_dep); DECLARE(fp_madd_ind);
DECLARE(fp_float_add_dep); DECLARE(fp_float_add_ind);
DECLARE(fp_mul_trunc_dep); DECLARE(fp_mul_trunc_ind);
DECLARE(fp_lsi_add_dep); DECLARE(fp_lsi_add_gap); DECLARE(fp_lsi_add_ind);
typedef uint32_t (*block_t)(void);
static volatile uint32_t sink;
static inline uint32_t ccount(void) { uint32_t n; asm volatile("rsr.ccount %0" : "=a"(n)); return n; }

static IRAM_ATTR __attribute__((noinline)) uint32_t bench(block_t block) {
  sink ^= block();
  uint32_t ps;
  asm volatile("rsil %0, 15" : "=a"(ps) :: "memory");
  uint32_t start = ccount();
  uint32_t sum = 0;
  for (uint32_t i = 0; i < 1024; ++i) sum ^= block();
  uint32_t delta = ccount() - start;
  asm volatile("wsr.ps %0\nrsync" :: "a"(ps) : "memory");
  sink = sum;
  return delta;
}

void app_main(void) {
  esp_chip_info_t chip;
  esp_chip_info(&chip);
  printf("FP_CONFIG cpu_hz=%u chip_revision=%u calls=1024 ops_per_call=256 iram=1\n",
         (unsigned)(esp_rom_get_cpu_ticks_per_us() * 1000000u), (unsigned)chip.revision);
  struct { const char* name; block_t block; } cells[] = {
    {"empty", fp_empty}, {"add_dep", fp_add_dep}, {"add_ind4", fp_add_ind},
    {"mul_dep", fp_mul_dep}, {"mul_ind4", fp_mul_ind},
    {"madd_dep", fp_madd_dep}, {"madd_ind4", fp_madd_ind},
    {"float_add_dep", fp_float_add_dep}, {"float_add_ind", fp_float_add_ind},
    {"mul_trunc_dep", fp_mul_trunc_dep}, {"mul_trunc_ind", fp_mul_trunc_ind},
    {"lsi_add_dep", fp_lsi_add_dep}, {"lsi_add_gap", fp_lsi_add_gap},
    {"lsi_add_ind", fp_lsi_add_ind},
  };
  for (unsigned c = 0; c < sizeof(cells) / sizeof(cells[0]); ++c) {
    uint32_t result = cells[c].block();
    printf("FP_RESULT cell=%s result_bits=%08" PRIx32 "\n", cells[c].name, result);
    for (unsigned sample = 0; sample < 9; ++sample) {
      uint32_t cycles = bench(cells[c].block);
      printf("FP_SAMPLE cell=%s sample=%u cycles=%" PRIu32 "\n", cells[c].name, sample, cycles);
      vTaskDelay(1);
    }
  }
  printf("FP_DONE cells=%u\n", (unsigned)(sizeof(cells) / sizeof(cells[0])));
  fflush(stdout);
}
