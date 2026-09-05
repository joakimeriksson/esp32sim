# Browser correctness evidence, 2026-09-05

The retained logs report 186 passing release workspace tests, zero failures and seven filtered tests, 38,681 passing actual-WASM differential cases, and passing hello, C3 hello, C6 hello, C6 energy-scan and panel firmware runs. Clippy completed successfully.

The production-page capture contains three strokes spanning all 121 expected columns. All 158 sampled inputs reached the worker and were followed by emulator run entry; no page errors were recorded. Arrival-to-run-entry p50/p95/p99/max were 13.5/24.6/27.9/29.6 ms. This endpoint does not measure controller consumption or optical presentation, and region changes do not causally match individual samples. Chrome 152.0.7977.77 / V8 15.2.124.19 ran on macOS. This single capture resumed before the strokes; it is not a repeated latency benchmark.

Isolated source combinations explain the corrected native instruction counts:

| Source combination | hello-s3 | atech-script1 | atech-sid | panel-sid |
| --- | ---: | ---: | ---: | ---: |
| Original baseline | 4,296,987 | 319,602,849 | 338,480,800 | 396,793,342 |
| Narrow-MMIO rejection only | 4,296,987 | 319,602,849 | 338,480,800 | 396,793,342 |
| Bus interrupt notification + narrow-MMIO rejection | 4,296,987 | 319,625,999 | 338,491,585 | 396,793,342 |
| Bus + scheduler | 4,296,987 | 319,625,267 | 338,494,620 | 396,472,575 |
| Operand correction only | 4,295,116 | 319,602,849 | 338,480,800 | 396,786,672 |
| Bus + scheduler + operands | 4,295,116 | 319,625,267 | 338,494,620 | 396,469,561 |

The baseline and narrow-MMIO-only runs pass all 13 selected golden tests. Other configurations intentionally fail old exact-count expectations. The combined run reproduces the native count changes; operand corrections also advance four reboot-console timestamps by 1 ms. Existing audio expectations remain unchanged. Earlier per-run source hashes were reconstructed afterward, and the earlier scheduler snapshot was not retained separately: its reconstruction uses the final TRAP-fragment policy. The semantic manifest documents that limit; it does not establish byte-for-byte provenance for the earlier scheduler run.

Raw logs, screenshots, input/frame records, hashes, patches and source snapshots are in [the correctness archive](https://github.com/aliceisjustplaying/esp32sim/releases/download/browser-correctness-evidence-2026-09-05/correctness-evidence-2026-09-05.tar.gz). Archive SHA-256: `8be2f682db99043f55ae80bcc0cb57ee446dec83fdba963f2e6f14499d5e88d1`; byte size: `187428`. Local account names are redacted; the package manifest hashes every distributed payload file. Browser profiles, firmware binaries, WASM binaries, compiled outputs and performance campaigns are excluded. The capture records firmware/module hashes; the retained WASM file was checked against the recorded capture hash during packaging. Compact results omit full trace, frame, sample and serial arrays; those remain in the archive.

The semantic baseline is `88e55de85a145bb82b38bd4df0715d14011ba243`; its retained combined source files match `9eea9424f4f1f1a61c664b0f858dc4d967e2c8ca`. The latter is also the packaging checkout revision, not an assertion that every earlier run used that exact commit. The page harness and six page-source files match their recorded capture hashes. PR48 later merged main without changing its source tree; that publication revision is `08458ad73325cc5b35dc4d2f7e4b47855fedc1e8` and is separate from these execution records.

To reproduce a source combination, extract the archive beneath a repository checkout containing the baseline revision, then run `python3 evidence-package/semantic-ablation/reproduce.py bus-scheduler-operands`. This materializes fresh source without running tests. Add `--run` to compile and run its 13 selected release goldens against the original expectations. The manifest records Rust 1.98.0 and the required ROM directory; supply the ROM files at that location or adjust that local manifest field. The combined run is expected to fail the original exact-count expectations shown above. Firmware inputs and offline Cargo dependencies must be supplied separately.

To rerun WASM checks from the source checkout, use `tools/wasm-jit-test.sh`. Run `tools/wasm-build.sh` to install the ordinary browser build, then `node tools/wasm-test.mjs hello c3-hello c6-hello c6-energy-scan panel` with its firmware manifests, ROM directory and assets installed. For page capture, serve the matching production page and hashed firmware/module assets, start isolated headless Chrome with remote debugging port 9239, then run `node tools/browser-benchmark/capture-page-response.mjs 'http://127.0.0.1:8810/?wasm&fw=page-check&touchTrace' OUTPUT_DIRECTORY 9239`. The archived assets map describes the captured inputs; local file locations need adjustment on another machine.
