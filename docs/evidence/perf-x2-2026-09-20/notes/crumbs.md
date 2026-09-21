QUEUED (prediction +5 to +6% for the stack): four exact host-side crumbs outside the JIT core remove 7.9% -> 1.0% of non-idle Node profile samples on pocket-tank; GPIO masks were a loss and the EX158 publication retry shows nothing, so neither is in the stack.

# EX170 crumbs: sweep of the small non-JIT items (agent `crumbs`, Sep 21)

Worktree `/Users/alice/src/a/esp32sim-x2-crumbs`. Stack branch `x2/crumbs` = 6f6db8fc (dma dbe39b3d + 2485760a, spi ca74919d,
fma b0fcf56c, round 6f6db8fc). Single-item branches off 414c6e07: `x2/crumbs-dma` 2485760a, `x2/crumbs-spi` 96f21f6d,
`x2/crumbs-fma` 49e8e46c, `x2/crumbs-round` 0d2ad57c, `x2/crumbs-pub` 657d026f, `x2/crumbs-gpio` f11c8feb (not queued).
Run dir with every log, profile and script: `/Users/alice/src/a/esp32sim-x2/runs/crumbs/`.

## Gates (all artifacts)

- `fx.sh` = `bin/exact.mjs` plus a SHA-256 over the content of every binary frame (`runs/crumbs/exact-crumbs.mjs`), 30 guest s.
  base, crumbs-dma, -spi, -fma, -gpio, -pub, -round, -all: insns 10073833775, console b9d9966e5d9d, frames 3094,
  framesSha 847b5478d0a7b2a6, EXACT true (`runs/crumbs/fx-*.log`). Frame content is equal, not just the count.
- Native `cargo test -j4 -p esp32s3 -p xtensa-lx7 -p esp-soc -p esp-periph` on 6f6db8fc: 314 passed, 0 failed
  (`runs/crumbs/native-tests-final.log`); same on the superset tree with gpio + pub (`native-tests-all.log`).
- `tools/wasm-jit-test.mjs` on 6f6db8fc: PASS, 78903 differential cases.

## What was measured (Node `--cpu-prof`, SECS=10, share of non-idle samples; `runs/crumbs/prof-*`, `share.py`, `share2.py`)

| function | base | own artifact | crumbs-all |
|---|---|---|---|
| `SocBus::dma_copy` | 2.658% | 0.064% (dma) | 0.059% |
| `read32_access::<false>` + `write32_access::<false>` | 0.521% + 0.693% | 0.006% + absent (dma) | 0.026% |
| AMOLED `spi_transfer` | 1.447% | 0.200% (spi) | 0.219% |
| `fmaf` + `h_fused` | 0.653% + 0.841% | 0.012% + 0.023% (fma) | 0.006% + 0.032% |
| `after_round_rest` + `display_push_hz` | 0.916% + 0.181% | 0.667% + 0.071% (round) | 0.619% + 0.081% |
| GPIO `irq_sources` + `refresh_irq` | 0.366% + 0.189% | 0.041% + 0.736% (gpio: WORSE) | untouched |
| sum of the touched rows (without GPIO) | 7.91% | | 1.04% |

1. **dma** (`esp32s3/src/bus/dma.rs` `dma_copy_runs`, `bump_run`). Counter build (`runs/crumbs/counters.patch`, `count30.log`):
   296,723 `dma_copy` calls per 30 guest s, 607,688,704 bytes, all PSRAM to PSRAM (esp_async_memcpy); 607,467,544 bytes
   (99.96%) take the run path; the rest is a 2-byte tail on 110,580 calls (4094-byte descriptors alternate aligned and
   unaligned). So the base moves 152M words through two non-inlined bus decodes each: that is also where
   `read32_access::<false>`/`write32_access::<false>` (0.75% each in the Chrome profile) come from, answering item 5's "who calls
   the slow accessors". Exactness: a run is taken only inside one mapping on each side, writable destination, not MMIO, and
   host ranges disjoint or destination below source (forward copy = memmove); anything else falls back to the old loop from
   byte `i`, so fault side and partial copy are unchanged. Page versions get the same number of bumps as per-word or per-byte
   writes, including the previous-page bump for writes in the first 3 bytes of a 256-byte version page. `_unpriced` still
   skips cache pricing. Test `dma_run_copy_matches_the_word_loop` (6000 random spans: page crossings, IRAM/DRAM alias, PSRAM
   alias pages, flash destination, MMIO source, holes, both overlap directions) compares bytes, `page_ver`, result and
   `last_fault` with the kept reference loop; two mutations (no overlap guard; wrong early-bump) are caught.
2. **spi** (`esp32s3/src/board/waveshare_amoled18.rs` `write_pixel_bytes`): EX158 s1 panel half reused as is (inspected
   738a38c6; sound), test widened to windows with x1 < x0. 154,850 SPI2 transfers and 608,251,965 tx bytes per 30 guest s went
   through a per-byte `Option` state machine. RETRY of EX158; what differs is measurement quality (profile share + frame SHA)
   and isolation from the publication half.
3. **fma** (`xtensa-lx7/src/jit/wasm_float.rs`): key fact: the base helper is ALREADY the f64 recipe. `f32::mul_add` on wasm32
   is libm `fma_wide_round` (`library/compiler-builtins/libm/src/math/generic/fma_wide.rs`): promote, mul, add, and only if
   the low 29 bits equal 0x10000000 do the error fix-up. The cost was two calls (call_indirect to `h_fused`, then `fmaf`),
   not soft float. The emitter now inlines exactly that common case (f64.promote x3, f64.mul, f64.add, mask test,
   f32.demote) and calls the unchanged helper for the halfway pattern, so it is bit-exact with the helper by construction
   (unlike EX095's set-lowest-bit recipe). Proof run in wasm on the real emitted code: `tools/fma-sweep.mjs` +
   `esp32sim_test_fma_sweep` (jit-tests feature) compiles one [MADD.S, MSUB.S] block and runs it over 400M triples (800M
   fused ops): 0 mismatches, 38,850,772 triples in the halfway class that still calls the helper
   (`runs/crumbs/fma-sweep-400M.log`); classes: random bits, mid exponents, subnormals, exponent extremes, inf/NaN payloads,
   signed zeros, constructed near-ties incl. subnormal and overflow scale. Mutating the halfway constant gives 49,582
   mismatches per 2M, so the sweep sees the double-rounding class.
   SIDE FINDING (pre-existing, not changed here): a native port of that libm algorithm differs from hardware FMA on
   subnormal-result near-ties, e.g. x=0x1a001001 y=0x19ffe002 z=0x001ce786: hardware 0x001ce787, libm recipe 0x001ce786
   (11,144 of 22,768 constructed triples; `runs/crumbs/fma-libm-vs-hardware.rs`, native aarch64). The wasm build therefore can
   differ from native builds and real silicon in that corner. Fix = also send results below the f32 normal range to an exact
   path. It would change the bit-exact contract only if pocket-tank hits it (unlikely), so it needs its own experiment.
4. **gpio**: NEGATIVE. Stateless three-mask `irq()` (EX028 retry without a cache): `irq_sources`+`refresh_irq` 0.555% -> 0.777%.
   The old `any()` already short-circuits on INT_ENA=0 pins; the cost is call frequency, which only a cache (EX028) or fewer
   `refresh_irq` scans can fix. Not queued, not in the stack. Note `irq_sources` vanishes from some builds (pub: 0.005%) only
   because it gets inlined into `refresh_irq` (0.566%): always sum the two.
5. **round** (found while checking the publication path, `esp-soc/src/machine.rs` `after_round_rest`): every scheduler round
   did a `dyn BoardModel::display_push_hz()` call and a u64 division to learn a push was not due. Now a cached interval
   filters the not-due case; a due round re-derives the interval from the board before deciding (boards are only swapped
   before boot: `wasm/src/s3.rs:64` refuses after `booted`). 3094 frames and frame SHA equal, so PR #89's 120 Hz cadence holds.
6. **pub** (EX158 s1 publication half, retry): no profile effect (`after_round_rest` 0.916% -> 0.947%). Queued alone as a cheap
   confirmation of EX158 s1's noise-level result; excluded from the stack.

## Queued candidates (all against plain base, AGENT=crumbs)

| job | artifact | commit | tests | counter / profile evidence | predicted |
|---|---|---|---|---|---|
| crumbs-dma | crumbs-dma.wasm | 2485760a | dma_copy run memmove | 99.96% of 607.7 MB on run path; 3.87% -> 0.07% | +3 to +4% |
| crumbs-spi | crumbs-spi.wasm | 96f21f6d | Co5300 row runs | 608 MB tx; 1.45% -> 0.20% | +1 to +1.5% |
| crumbs-fma | crumbs-fma.wasm | 49e8e46c | inline f64 FMA common case | 800M ops 0 mismatches; 1.49% -> 0.04% | +0.7 to +1.2% |
| crumbs-round | crumbs-round.wasm | 0d2ad57c (.rev says 414c6e07 + dirty patch = same diff) | cached push interval | 1.10% -> 0.74% | +0.3% |
| crumbs-pub | crumbs-pub.wasm | 657d026f | EX158 s1 publication half | no profile change | 0 to +0.3% |
| crumbs-all | crumbs-all.wasm | 6f6db8fc | dma + spi + fma + round | touched rows 7.91% -> 1.04% | +5 to +6% |

## Catalog row (ready to paste)

| <a id="ex170"></a>EX170 | **Non-JIT crumbs: DMA run copy, panel row runs, inline f64 FMA, push-interval filter**<br>dma_copy memmove; esp_async_memcpy; Co5300 row-run; h_fused/fmaf inline; after_round_rest; GPIO irq masks | Queued; exact; timing pending | pocket-tank 30 s, base 414c6e07: insns, console SHA, 3094 frames and frame-content SHA equal for all six artifacts. Node profile shares: dma_copy + unpriced word accessors 3.87% -> 0.07% (607.5 of 607.7 MB per 30 guest s on the run path), spi_transfer 1.45% -> 0.20%, fmaf + h_fused 1.49% -> 0.04% (in-wasm sweep 800M fused ops, 0 mismatches), after_round_rest + display_push_hz 1.10% -> 0.74%. Negative: stateless GPIO irq masks 0.56% -> 0.78% (EX028 retry, dropped); EX158 s1 publication half alone shows no profile change. Side finding: libm's f64 fmaf recipe differs from hardware FMA on subnormal-result near-ties (pre-existing in wasm builds). | Adopt per item after the quiet-M3 pairs. Related: EX158 (spi and pub are retries), EX095 (different, exact by construction), EX028 (retry, negative), EX074 (modeled timing untouched). Note `/Users/alice/src/a/esp32sim-x2/notes/crumbs.md`, branch `x2/crumbs` 6f6db8fc. |

## Next crumbs (named, not built; base Node profile shares)

- `run_unmodeled` round prologue: the `idle.iter_mut().enumerate().take(S::CORES)` loop (`esp-soc/src/machine.rs:494`) is an
  out-of-line `Take<Enumerate<IterMut>>` try_fold at 0.34%; a plain 2-core index loop should remove it.
- `refresh_irq` + GPIO `irq_sources` 0.56%: needs fewer scans (dirty gating) or the EX028 cache; masks do not help.
- `periph_write` 0.36% (all via `Bus::write32`; SPI2 register page 0x60024000 3.76M accesses per 12 s per
  `notes/spec-0919-oldbase.md:48`): a per-page fast dispatch for SPI2 is the candidate.
- `check_interrupts` 0.61% (called from `step_blocks`), `tick_impl` 0.29% + `flush_ticks` 0.25%, `code_page` 0.23% (from
  `run_inner`): scheduler/JIT-wrapper territory.
- What is left of `after_round_rest` (0.62%) is per-round work, not publication: `apply_script_events` and the `rt.enabled`
  block; worth one look with a counter of rounds per second.

## Risk to exactness

dma: a bulk write bumps page versions by the same counts, so JIT invalidation is unchanged; the only state that can differ
is TLB slot contents (host-only). fma: NaN payloads rely on V8 compiling the same f64 ops the same way in the helper and in
generated code; the sweep covers NaN operands in both tiers. round: a board swapped after boot with a different push rate
would be seen one interval late (no such path exists in wasm).
