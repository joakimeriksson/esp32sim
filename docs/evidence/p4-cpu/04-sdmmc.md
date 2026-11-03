# EX243: the SD/MMC host and an SD card (`p4/04-sdmmc`)

Branch commit `6d1f7adc` (parts 1 to 4: the DesignWare SD/MMC host in `esp-periph`, the SD card,
the P4's socket and GPIO layout, `--sd`, a browser load kind) against upstream `main`
`20117b08`, as [README.md](README.md) describes; cumulative. Runs 2 and 3 compare it with part 3
(`4394cd37`) directly. Results: [04-sdmmc.json](04-sdmmc.json).

The other chips run none of the new code: the host and card are new `esp-periph` modules the P4
alone instantiates, plus a default `SocBus` method and a load kind. Every run of a workload
retired the same instructions and printed the same console on both builds.

## run 1, against main, started 2026-10-22T20:52:24-0600

| workload | main median (range) | branch median (range) | change | branch slower | instructions | console SHA-256 | 1-min load before / after |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| s3-hello | 0.41 (0.40-0.42) | 0.40 (0.40-0.41) | -2.44% | 1/7 | 11,990,488 + 6,798,360 | `119a1216fb34…` | 17.6 / 16.6 |
| c3-hello | 3.99 (3.98-4.01) | 4.05 (4.04-4.05) | +1.50% | 7/7 | 4,800,000,000 | `3b9d8d3eb053…` | 16.6 / 8.3 |
| c6-hello | 2.89 (2.88-2.90) | 2.89 (2.88-2.89) | +0.00% | 2/7 | 4,800,000,000 | `bd69fbb9ac5c…` | 8.3 / 5.8 |
| pocket-tank | 14.63 (14.58-14.90) | 14.65 (14.60-15.04) | +0.14% | 4/7 | 1,113,634,993 + 1,490,529,616 | `62b7af05631b…` | 5.8 / 4.7 |

## run 2, against p4/03, started 2026-10-22T20:58:19-0600

| workload | p4/03 median (range) | branch median (range) | change | branch slower | instructions | console SHA-256 | 1-min load before / after |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| c6-hello | 2.89 (2.89-2.90) | 2.90 (2.88-2.91) | +0.35% | 3/7 | 4,800,000,000 | `bd69fbb9ac5c…` | 4.7 / 4.6 |

## run 3, against p4/03, started 2026-10-22T20:59:27-0600

| workload | p4/03 median (range) | branch median (range) | change | branch slower | instructions | console SHA-256 | 1-min load before / after |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| c3-hello | 3.95 (3.94-3.96) | 3.99 (3.98-4.00) | +1.01% | 7/7 | 4,800,000,000 | `3b9d8d3eb053…` | 4.2 / 4.6 |

**`c6-hello` is not slower.** An earlier draft of this part, on an older base and toolchain,
reported `c6-hello` 8-9% slower than part 3 and put it down to an inlining change. On this base
it is +0.00% against main (slower 2/7) and +0.35% against part 3 (slower 3/7).

**Flag: `c3-hello` is slower in 7 of 7 pairs**, +1.50% against main and +1.01% against part 3,
although this part adds nothing the C3 executes. Part 3 measured +0.00% against main
([03-chip.md](03-chip.md)) and part 1 +1.27% / +2.04% ([01-rv32f.md](01-rv32f.md)): the C3's
hello moves by 1-2% between builds of unchanged C3 code, which reads as code layout under fat
LTO. Not attributed further. The first workload of run 1 started right after a test build
(load 17).

## run 4, against main `b97fdc7c`, after the rebase and the review fixes, started 2026-10-23T19:56:37-0600

Branch commit `37f6d9f4` against upstream `main` `b97fdc7c`, the same method on that base (the review fixes are in the
branch). Results: [04-sdmmc-b97fdc7c.json](04-sdmmc-b97fdc7c.json). 1-minute load 5 to 8 on 12 cores, per workload in the table.

| workload | main median (range) | branch median (range) | change | branch slower | instructions | console SHA-256 | 1-min load before / after |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| s3-hello | 0.40 (0.39-0.40) | 0.39 (0.39-0.40) | -2.50% | 3/7 | 11,990,488 + 6,798,360 | `119a1216fb34…` | 7.7 / 7.4 |
| c3-hello | 3.96 (3.96-3.97) | 3.92 (3.92-3.93) | -1.01% | 0/7 | 4,800,000,000 | `3b9d8d3eb053…` | 7.4 / 6.4 |
| c6-hello | 2.87 (2.86-2.88) | 2.92 (2.92-2.93) | +1.74% | 7/7 | 4,800,000,000 | `bd69fbb9ac5c…` | 6.4 / 5.5 |
| pocket-tank | 14.22 (14.21-14.24) | 14.30 (14.25-14.60) | +0.56% | 7/7 | 1,113,634,993 + 1,490,529,616 | `62b7af05631b…` | 5.5 / 5.3 |

**Flag:** `c6-hello` +1.74% (7/7); `pocket-tank` +0.56% (7/7).
