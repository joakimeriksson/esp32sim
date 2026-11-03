# EX241: CLIC mode, Zicntr, harts and per-core hooks (`p4/02-core`)

Branch commit `c5260076` (parts 1 and 2: the CLIC mode, the Zicntr counters, hart IDs and LR/SC
reservations through the bus, `Soc::select_core` and `Soc::interrupt_taken`) against upstream
`main` `20117b08`, as [README.md](README.md) describes; the comparison is cumulative, part 1
included. Results: [02-core.json](02-core.json).

The hooks are `#[inline(always)]` empty `Soc` defaults, the reservation checks `Bus` defaults a
one-hart bus inlines away, and the core's CLIC and counter paths sit behind flags only
`Cpu::new_esp32p4` sets. Every run of a workload retired the same instructions and printed the
same console on both builds.

## run 1, started 2026-10-22T20:25:03-0600

| workload | main median (range) | branch median (range) | change | branch slower | instructions | console SHA-256 | 1-min load before / after |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| s3-hello | 0.40 (0.40-0.41) | 0.41 (0.40-0.41) | +2.50% | 3/7 | 11,990,488 + 6,798,360 | `119a1216fb34…` | 10.0 / 10.4 |
| c3-hello | 3.99 (3.98-4.17) | 4.01 (4.00-4.02) | +0.50% | 6/7 | 4,800,000,000 | `3b9d8d3eb053…` | 10.4 / 8.2 |
| c6-hello | 2.90 (2.88-2.93) | 2.91 (2.91-2.92) | +0.34% | 5/7 | 4,800,000,000 | `bd69fbb9ac5c…` | 8.2 / 8.1 |
| pocket-tank | 14.89 (14.69-15.61) | 14.79 (14.63-15.04) | -0.67% | 2/7 | 1,113,634,993 + 1,490,529,616 | `62b7af05631b…` | 8.1 / 7.7 |

**Flag: `c3-hello` is slower in 6 of 7 pairs**, by +0.50% on the medians (4.01 s against
3.99 s, a 10 ms timer). Part 1 alone measured +1.27% and +2.04% on the same workload
([01-rv32f.md](01-rv32f.md)), so this part does not add to it. S3 `hello` differs by one timer
tick (0.41 s against 0.40 s, slower 3/7). The C6 and Pocket Tank are within 0.7%.

## run 2, against main `b97fdc7c`, after the rebase and the review fixes, started 2026-10-23T19:44:05-0600

Branch commit `90baa069` against upstream `main` `b97fdc7c`, the same method on that base (the review fixes are in the
branch). Results: [02-core-b97fdc7c.json](02-core-b97fdc7c.json). 1-minute load 6 to 9 on 12 cores, per workload in the table.

| workload | main median (range) | branch median (range) | change | branch slower | instructions | console SHA-256 | 1-min load before / after |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| s3-hello | 0.39 (0.39-0.40) | 0.39 (0.39-0.40) | +0.00% | 2/7 | 11,990,488 + 6,798,360 | `119a1216fb34…` | 6.8 / 6.8 |
| c3-hello | 3.96 (3.96-3.97) | 3.97 (3.96-3.97) | +0.25% | 3/7 | 4,800,000,000 | `3b9d8d3eb053…` | 6.8 / 9.1 |
| c6-hello | 2.87 (2.86-2.88) | 2.85 (2.85-2.87) | -0.70% | 1/7 | 4,800,000,000 | `bd69fbb9ac5c…` | 9.1 / 7.5 |
| pocket-tank | 14.21 (14.20-14.24) | 14.22 (14.20-14.79) | +0.07% | 4/7 | 1,113,634,993 + 1,490,529,616 | `62b7af05631b…` | 7.5 / 5.9 |

No workload meets the flag (6 of 7 pairs slower, or disjoint ranges).
