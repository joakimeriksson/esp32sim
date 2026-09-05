# Browser validation of the native regression fix

The native regression fix also reduced complete TinyDraw browser battery time in three alternating pairs against the reviewed PR48 revision. Every run passed all 36 required gates, with identical console output and 9,819,885,134 executed instructions.

| Pair | Reviewed PR48 seconds | Fixed PR48 seconds | Fixed time change |
| --- | ---: | ---: | ---: |
| 1 | 119.231805 | 116.797090 | −2.04% |
| 2 | 121.638490 | 117.070380 | −3.76% |
| 3 | 123.942265 | 118.170010 | −4.66% |
| Median | 121.638490 | 117.070380 | −3.76% |

Runs used fresh dedicated Chrome processes in reviewed/fixed, fixed/reviewed, reviewed/fixed order. Firmware, ROM and harness hashes matched; the emulator WASM hash changed as intended. The battery runs without real-time pacing or browser canvas display. These are complete workload wall times, including console delivery, and do not establish interactive latency or silicon cycle accuracy. RSS samples showed no consistent reduction. Peak registered generated-code bytes were 31,698,131 before and 31,637,357 after. The separate yellow antialiasing receipt remains unresolved and is distinct from the 36 passing boolean gates.

After applying the same native fix to the dependent queued-output PR49 and integer-operation PR52, one complete battery per PR passed the same gates, console and instruction checks. Those single captures verify behavior; the older 1.67% and 2.51% incremental speed measurements belong to the previous bases and were not repeated here. The refreshed top also passed 41,711 actual-WASM differential cases, with 39,391 compiled modules released, WASM release Clippy and five firmware demos.

A separate production-page capture on refreshed PR52 produced three complete visible strokes, each spanning all 121 expected columns. All 158 sampled inputs reached the worker and were followed by emulator run entry. Each contact had display publication during the drag (42, 46 and 44 frames). This checks delivery and visible drawing; controller consumption and optical input-to-display latency were not measured.

The measured sources are reviewed PR48 `335d0786333390cf40f50084b8ce7b151cf75089`, that revision plus `gpio/selected.patch` from the native archive, PR49 `0a517293a66ef374932c42a727bc31229f34a019` plus the same patch, and PR52 `1d7608fdd2dd8e5fcc0be76fdf279cfcc46d35c8` plus that patch. The fixed PR48 code is also recorded in commit `2a6387f6ed80ba7477863eec2ad5b560bbfe77be`. Subsequent evidence and merge commits do not change the measured code. The browser archive records each WASM hash and source construction; the [native evidence](../native-regression-fix-2026-09-05/README.md) records main-relative CLI comparisons separately.

The [browser evidence archive](https://github.com/aliceisjustplaying/esp32sim/releases/download/native-fix-browser-evidence-2026-09-05/native-fix-browser-evidence-2026-09-05.tar.gz) retains all eight battery captures, strict comparisons, the production-page capture, source patches, input hashes and reproduction tools. It is 382,058 bytes, SHA-256 `3f85ddd626dfb7397e54f5cacebcc54e4be223d2c4ee3eb077ad5da30e345864`.

Extract the archive and follow its `REPRODUCE.md`. Exact prebuilt TinyDraw firmware and ROM inputs are external; hashes and firmware acquisition/rebuild instructions are provided. A rebuild is a new measurement. Account identifiers are redacted, with original and distributed hashes retained. Browser profiles, binaries and full source/build trees are excluded.
