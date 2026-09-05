# Review fixes: observer throughput and execution policies

Ordinary trap observers now retain block execution. `Wants::TRAP_PC` separately requests exact instruction attribution; `--irq-latency` and `--vcd` use ordinary `TRAP` and do not pay for one-instruction fragments. Exact PC attribution still does not refine the scheduler-cycle timestamps supplied to observers.

S3 peripheral reads and writes consistently require aligned 32-bit accesses. Unsupported narrow accesses fault before device or timing side effects. This is an emulator policy, not a claim about every silicon PMS configuration. Documentation also records idle deadlines, read-driven interrupt refresh, the reasons for earlier golden changes, and the partial-frame tradeoff during continuous redraws. This review follow-up changes no goldens.

## Observer measurement

The Atech script1 workload ran for five emulated seconds in fresh CLI processes. One warmup round and six measured rounds each included all nine combinations of three revisions and three observer modes. Combination order reversed on alternating measured rounds; all 63 runs were retained. The host was ARM64 macOS 27.0, with Rust 1.98.0. No concurrent build or browser benchmark ran during these measurements.

| Revision | Plain median | `--irq-latency` median | `--vcd` median |
| --- | ---: | ---: | ---: |
| Preserved main `563fecd4` | 2.557 s | 2.724 s | 3.324 s |
| Reviewed PR48 `177c25a6` | 2.559 s | 6.294 s | 6.786 s |
| Fixed PR48 `850efecd` | 2.566 s | 2.735 s | 3.222 s |

The fixed observer modes return to approximately the preserved main's wall time on this workload. Observers still have their normal callback and output costs: fixed IRQ-latency takes 1.066 times plain execution, and fixed VCD takes 1.256 times plain execution. These measurements do not establish universal performance equivalence or renew the earlier browser speed claims.

Every mode and repetition has the same instruction totals within each revision: main has 12,339,151 on core 0 and 307,263,698 on core 1; reviewed and fixed have 12,340,984 on core 0 and 307,284,283 on core 1. Every reviewed and fixed VCD is byte-identical: 51,538,356 bytes, SHA-256 `1b65f77727bda78698a1562160f1dbaaa2477b2b999a2b597db3b4189821d6bd`. The main/reviewed instruction difference is from the previously documented operand, interrupt and scheduler changes.

## Validation

- PR48: 200 native tests, 38,681 actual-WASM differential cases, native release Clippy and five WASM firmware demos passed.
- Combined PR48 → PR49 → PR52: 201 native tests, 41,711 actual-WASM differential cases with 39,391 compiled modules released, native/WASM release Clippy and five WASM demos passed. The WASM checks used a fresh target directory.
- Both native workspace runs had zero failures or ignored cases, with seven external-input cases filtered. Existing golden files are unchanged.
- PR53: all four operand tests passed after adding the independently assembled `S32NB` encoding `0x59f320` and a check of its scaled store offset. The false-overflow corpus now has 32 cases; PR48 contains the same addition.
- The combined top branch completed one TinyDraw browser battery with all 36 verdict gates, 9,819,885,134 instructions and console output identical to the previous top-branch capture. This single run without real-time pacing does not renew earlier browser speed claims or establish input latency. The fixed `ssaa_receipt=yellow` marker tracks an existing performance limitation in smoothing stroke edges after drawing. It is separate from the 36 pass/fail gates; closing it requires a dedicated settling-performance check, which this run does not provide.

## Reproduction and records

[The capture archive](https://github.com/aliceisjustplaying/esp32sim/releases/download/pr-review-fixes-evidence-2026-09-05/pr-review-fixes-evidence-2026-09-05.tar.gz) contains all observer timings and stdout/stderr, input and executable hashes, review-fix patches, test logs, the browser capture and the scripts. VCD files are represented by their byte counts and SHA-256 hashes. Local paths are replaced with `@WORKSPACE@` and `@HOME@`.

Build `cli` in separate checkouts of `563fecd4`, `177c25a6` and `850efecd` with `cargo build --release -p esp32sim`. In the extracted archive, replay the recorded command order with:

```sh
python3 replay-observers.py \
  --workspace /path/to/esp32sim \
  --rom /path/to/esp32s3_rev0_rom.elf \
  --main /path/to/main/target/release/esp32sim \
  --reviewed /path/to/reviewed/target/release/esp32sim \
  --fixed /path/to/fixed/target/release/esp32sim \
  --output /path/to/new-results
```

Use the Atech files under `web/wasm/fw/public/` whose hashes are recorded in `environment.json`. The replay retains new stdout/stderr and VCD files and rejects changed instruction totals. `environment.json` records full source revisions; the evidence-only commits that follow those revisions do not change the tested code.

Earlier broader comparisons remain in the [native evidence](../native-regression-fix-2026-09-05/README.md) and [browser evidence](../native-fix-browser-validation-2026-09-05/README.md).
