idf_component_register(SRCS "fp_timing.c" "fp_blocks.S" INCLUDE_DIRS "."
  PRIV_REQUIRES esp_psram spi_flash xtensa esp_hw_support)
set_source_files_properties(fp_timing.c PROPERTIES
  COMPILE_OPTIONS "-fno-tree-vectorize;-fno-unroll-loops")
