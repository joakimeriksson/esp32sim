# EX244: the USB-OTG device port, a USB host, timed SD (`p4/05-usb`)

Branch commit `ca2ee63a` (parts 1 to 5: the DWC2 core in `esp-periph`, the USB host software in
`esp-soc`, the P4's high-speed port, `--usb-host`, `--sd-timing`) against upstream `main`
`20117b08`, as [README.md](README.md) describes; cumulative. Results: [05-usb.json](05-usb.json).

The other chips run none of the new code: the USB core and the host are new modules the P4
alone instantiates, and the timed SD bus is behind a flag in cold functions. Every run of a
workload retired the same instructions and printed the same console on both builds.

The runs were made before the branch was reworded on 2026-10-22: identifiers, comments, log
strings and documentation of the USB host and its tests changed, which none of these workloads
runs, and the binary's layout moved with them. `ca2ee63a` retires the same instructions and
prints the same consoles (one run of each workload); the times were not measured again.

## run 1, against main, started 2026-10-22T21:05:21-0600

| workload | main median (range) | branch median (range) | change | branch slower | instructions | console SHA-256 | 1-min load before / after |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| s3-hello | 0.40 (0.39-0.40) | 0.40 (0.40-0.40) | +0.00% | 3/7 | 11,990,488 + 6,798,360 | `119a1216fb34…` | 20.0 / 18.8 |
| c3-hello | 3.93 (3.91-3.94) | 3.94 (3.93-3.95) | +0.25% | 6/7 | 4,800,000,000 | `3b9d8d3eb053…` | 18.8 / 9.5 |
| c6-hello | 2.84 (2.81-2.85) | 2.84 (2.81-2.85) | +0.00% | 2/7 | 4,800,000,000 | `bd69fbb9ac5c…` | 9.5 / 6.7 |
| pocket-tank | 14.40 (14.22-14.70) | 14.38 (14.22-14.51) | -0.14% | 2/7 | 1,113,634,993 + 1,490,529,616 | `62b7af05631b…` | 6.7 / 3.5 |

`c3-hello` is slower in 6 of 7 pairs, by +0.25% on the medians (3.94 s against 3.93 s, one
10 ms tick); the C3's 1-2% swings between builds of unchanged C3 code are in
[04-sdmmc.md](04-sdmmc.md). The rest are within 0.2%.

## run 2, against main `b97fdc7c`, after the rebase and the review fixes, started 2026-10-23T20:02:53-0600

Branch commit `7f5e3ee2` against upstream `main` `b97fdc7c`, the same method on that base (the review fixes are in the
branch). Results: [05-usb-b97fdc7c.json](05-usb-b97fdc7c.json). 1-minute load 5 to 13 on 12 cores, per workload in the table.

| workload | main median (range) | branch median (range) | change | branch slower | instructions | console SHA-256 | 1-min load before / after |
| --- | ---: | ---: | ---: | ---: | ---: | --- | --- |
| s3-hello | 0.40 (0.39-0.40) | 0.39 (0.39-0.40) | -2.50% | 3/7 | 11,990,488 + 6,798,360 | `119a1216fb34…` | 6.4 / 6.3 |
| c3-hello | 3.96 (3.96-3.97) | 3.92 (3.92-3.93) | -1.01% | 0/7 | 4,800,000,000 | `3b9d8d3eb053…` | 6.3 / 5.6 |
| c6-hello | 2.87 (2.86-2.87) | 2.84 (2.84-2.85) | -1.05% | 0/7 | 4,800,000,000 | `bd69fbb9ac5c…` | 5.6 / 5.1 |
| pocket-tank | 14.23 (14.06-15.22) | 14.24 (14.04-14.74) | +0.07% | 4/7 | 1,113,634,993 + 1,490,529,616 | `62b7af05631b…` | 5.1 / 12.6 |

No workload meets the flag (6 of 7 pairs slower, or disjoint ranges).
