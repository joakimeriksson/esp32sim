# Review repairs in the original PRs

The review repairs are carried by their owning original PRs and merged forward through the existing stack. The table below records the independently checked source heads. Published ancestry is retained with additive commits and merges; the separate companion PR is no longer required.

The redistributed runtime at [92cc3f20](https://github.com/joakimeriksson/esp32sim/tree/sanitized-92cc3f20f931d38d03ff96fd05ee16649f2ad551) is byte-identical to the previously validated runtime at [9bc4d5a5](https://github.com/joakimeriksson/esp32sim/tree/sanitized-9bc4d5a5762454c00d858badb6beb0f6366aa175). The only differences are this review explanation and two standalone test files: stub-address coverage available earlier in #100 and an extra zero-display-rate test available earlier in #89. [Exact file comparison](redistribution/parity.txt). Documentation records were then added in #119 and merged into #120.

Native performance recovery remains open in the [M3 report](final.md). These correctness checks do not claim a speedup or hardware calibration.

| Check | Result | Receipt |
| --- | --- | --- |
| Every original stacked PR source head, cargo check --workspace | All 30 passed | [Exact revisions and exit statuses](redistribution/final-summary.txt) |
| Native release workspace, including ignored tests except external fixtures | 439 passed, including all 14 goldens and AArch64 tests | [Native log](redistribution/final-native.log) |
| Native virtual-quanta machine and stop suites | 31 + 4 passed | [VQ log](redistribution/final-vq.log) |
| Workspace all-target Clippy with warnings denied | Passed | [Clippy log](redistribution/final-clippy.log) |
| Production-policy WASM differential tests | 79,628 passed, 82,222 modules released | [WASM log](redistribution/final-wasm.log) |
| Combined jit-tests,jit-profile,cpu-profile differential tests | 79,628 passed, 82,222 modules released | [Profiling log](redistribution/final-profile.log) |
| Production WASM, timing ABI, JIT handoff and eight firmware manifests | Passed | [Production log](redistribution/final-production.log) |
| Browser/touch tests and worker/pacing suites | 21 tests plus worker suites passed | [JS log](redistribution/final-js.log) |
| Browser benchmark Python tests using uv venv | 12 passed | [Python log](redistribution/final-python.log) |

Full validation used the frozen redistributed tip 92cc3f20. Native command: ESP32SIM_ROM_DIR=<ESP ROM ELF directory> cargo test --release --workspace -- --include-ignored --skip external_. Native VQ used ESP32SIM_VQ_NATIVE=1. Both WASM configurations used the shipped compiler flags from tools/wasm-rustflags.sh. The per-head checks are compile checks, not a claim that every complete test suite was rerun on every head. Original-head targeted test runs informed the backports; the full runtime validation above applies to the final tip.

The earlier [integrated validation](validation.md) and firmware qualification remain historical receipts for their named revisions. Hardware-only questions and the #105/#106 timing dependency remain as stated in [review dispositions](reviews.md).

| Original PR | Independently checked source head |
| --- | --- |
| [#85](https://github.com/joakimeriksson/esp32sim/pull/85) | [131c75f6](https://github.com/joakimeriksson/esp32sim/tree/sanitized-131c75f6c0087df3d6d342a5bd9539154b1bf55a) |
| [#86](https://github.com/joakimeriksson/esp32sim/pull/86) | [8f336023](https://github.com/joakimeriksson/esp32sim/tree/sanitized-8f336023567ba8b7e0fa1c4727d7cfd22280a821) |
| [#87](https://github.com/joakimeriksson/esp32sim/pull/87) | [6d4e645c](https://github.com/joakimeriksson/esp32sim/tree/sanitized-6d4e645cd40182d54a6029457f9c3973a886fec8) |
| [#88](https://github.com/joakimeriksson/esp32sim/pull/88) | [24354301](https://github.com/joakimeriksson/esp32sim/tree/sanitized-243543015866fc8572a1f897da69cdb0188b80c4) |
| [#89](https://github.com/joakimeriksson/esp32sim/pull/89) | [a5e5d073](https://github.com/joakimeriksson/esp32sim/tree/sanitized-a5e5d073a552d61baf229b24ea9b90908e956448) |
| [#91](https://github.com/joakimeriksson/esp32sim/pull/91) | [602cc868](https://github.com/joakimeriksson/esp32sim/tree/sanitized-602cc86801017f55dfad809612762751ca294061) |
| [#92](https://github.com/joakimeriksson/esp32sim/pull/92) | [242296c7](https://github.com/joakimeriksson/esp32sim/tree/sanitized-242296c7d6940a812a8ac59878369eb126911033) |
| [#93](https://github.com/joakimeriksson/esp32sim/pull/93) | [6af39d7c](https://github.com/joakimeriksson/esp32sim/tree/sanitized-6af39d7c4d213058d8b7a69e0f9957fced5a3bce) |
| [#96](https://github.com/joakimeriksson/esp32sim/pull/96) | [4896c61a](https://github.com/joakimeriksson/esp32sim/tree/sanitized-4896c61a9158849b3a634c3840dc4111d374dedb) |
| [#97](https://github.com/joakimeriksson/esp32sim/pull/97) | [d31b25ca](https://github.com/joakimeriksson/esp32sim/tree/sanitized-d31b25cad74b19f0abc5c9ef3150563bff348b45) |
| [#98](https://github.com/joakimeriksson/esp32sim/pull/98) | [03d411d3](https://github.com/joakimeriksson/esp32sim/tree/sanitized-03d411d3cc37cc9c1fa4f08defdff1956a3c3681) |
| [#99](https://github.com/joakimeriksson/esp32sim/pull/99) | [899b1905](https://github.com/joakimeriksson/esp32sim/tree/sanitized-899b190503cc843c3640db3604ec13c5cf8b2c85) |
| [#100](https://github.com/joakimeriksson/esp32sim/pull/100) | [ec4b74ff](https://github.com/joakimeriksson/esp32sim/tree/sanitized-ec4b74ff60304b309562d26526e694a64f848dec) |
| [#101](https://github.com/joakimeriksson/esp32sim/pull/101) | [e306bf4e](https://github.com/joakimeriksson/esp32sim/tree/sanitized-e306bf4e0bd5648a962e1c98353cf1db60fbe15b) |
| [#102](https://github.com/joakimeriksson/esp32sim/pull/102) | [cc41a9d4](https://github.com/joakimeriksson/esp32sim/tree/sanitized-cc41a9d4a57eaacea1c8d34c7bbdde253b81a02a) |
| [#94](https://github.com/joakimeriksson/esp32sim/pull/94) | [d13ac908](https://github.com/joakimeriksson/esp32sim/tree/sanitized-d13ac9089ed3ef81197efff3d2e941e06e0f75c0) |
| [#104](https://github.com/joakimeriksson/esp32sim/pull/104) | [743e4395](https://github.com/joakimeriksson/esp32sim/tree/sanitized-743e43954c1553930624f7e6602e3e38c5c7e40d) |
| [#105](https://github.com/joakimeriksson/esp32sim/pull/105) | [f4f4e14d](https://github.com/joakimeriksson/esp32sim/tree/sanitized-f4f4e14ddc903f91249b777096eb31c69f89ae94) |
| [#106](https://github.com/joakimeriksson/esp32sim/pull/106) | [8eccd18b](https://github.com/joakimeriksson/esp32sim/tree/sanitized-8eccd18b1e4d1e281c7560eb07e31e53b3522817) |
| [#107](https://github.com/joakimeriksson/esp32sim/pull/107) | [5dc7cd05](https://github.com/joakimeriksson/esp32sim/tree/sanitized-5dc7cd054db712b7278b11eb43816ed21a0cf761) |
| [#110](https://github.com/joakimeriksson/esp32sim/pull/110) | [8b1baddd](https://github.com/joakimeriksson/esp32sim/tree/sanitized-8b1badddd865df3bc9dc3c7cd8a9012f58e78022) |
| [#111](https://github.com/joakimeriksson/esp32sim/pull/111) | [c7f8f365](https://github.com/joakimeriksson/esp32sim/tree/sanitized-c7f8f365fb5546b40e25d66c863da3c8a27a368b) |
| [#112](https://github.com/joakimeriksson/esp32sim/pull/112) | [4e4db52d](https://github.com/joakimeriksson/esp32sim/tree/sanitized-4e4db52dc8b8c6a220a3f8b66a4db9bafc75eb46) |
| [#113](https://github.com/joakimeriksson/esp32sim/pull/113) | [c3a4f9a4](https://github.com/joakimeriksson/esp32sim/tree/sanitized-c3a4f9a46566c485183835e8d8e96abda970d918) |
| [#114](https://github.com/joakimeriksson/esp32sim/pull/114) | [b3961722](https://github.com/joakimeriksson/esp32sim/tree/sanitized-b39617221be2c1b3743aba93a6d9e8ecc47bc737) |
| [#115](https://github.com/joakimeriksson/esp32sim/pull/115) | [2e5e7d4f](https://github.com/joakimeriksson/esp32sim/tree/sanitized-2e5e7d4f1289b197b6e8b4b3e21682236e402cfd) |
| [#117](https://github.com/joakimeriksson/esp32sim/pull/117) | [3bda1942](https://github.com/joakimeriksson/esp32sim/tree/sanitized-3bda194242814ffc12c9af2a62122c05a70bb782) |
| [#118](https://github.com/joakimeriksson/esp32sim/pull/118) | [1f29cd16](https://github.com/joakimeriksson/esp32sim/tree/sanitized-1f29cd1676e3259ea934ff0a890200b7f87a1be4) |
| [#119](https://github.com/joakimeriksson/esp32sim/pull/119) | [ed45a338](https://github.com/joakimeriksson/esp32sim/tree/sanitized-ed45a338f6eafad5f79a4d5aa20b89e6b2944feb) |
| [#120](https://github.com/joakimeriksson/esp32sim/pull/120) | [92cc3f20](https://github.com/joakimeriksson/esp32sim/tree/sanitized-92cc3f20f931d38d03ff96fd05ee16649f2ad551) |

Backports follow API ownership. #87 owns the decoded-block regression; #102 owns the opcode predicate rename and cache-probe test reset because those modules first exist there. #91 owns machine-local fetch tags; later #101 and #105 preserve that change through module and fetch-ring refactors. #89 owns display-rate clamping; #118 owns reuse of the cached interval introduced there. #112 preserves the WASM target guards when inheriting profiling repairs. #119 owns aggregate evidence and review records, while #120 retains interior-alias runtime scope. The source-head links above and [review dispositions](reviews.md) identify the implementation and tests.
