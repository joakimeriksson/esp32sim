PREDICTION: MODEST (census says memory conflicts are no obstacle; the device deadline caps windows at ~4 quanta, so the prize is ~10% of wall at best, minus tracking tax). Morning table settles it.

# EX150 optimistic multi-quantum execution for two busy cores (agent `spec`)

Related: EX133/EX144 (virtual quanta, only when the peer idles), EX047/EX143 (bigger quantum, not bit-exact), EX006 (threads, rejected).
Materially different: keep the 64-instruction schedule's result bit-exact by running core 0 for K quanta, then core 1 for K quanta,
and checking that core 1 touched nothing core 0 wrote (and wrote nothing core 0 touched); a conflict rolls both back and replays lockstep.

Worktree `/Users/alice/src/a/esp32sim-exp-spec`, branch `exp/spec`.

## 1. Conflict census (native `--no-jit`, pocket-tank, 31 guest seconds)

Instrumentation: commit cb737a14 (+ d11ed267 for ext-only rounds and device pages). `emu-core/src/census.rs`, hooks in
`esp32s3/src/bus.rs` (every CPU `read*/write*_access` and `read_bulk`; `_unpriced` = DMA/host) and in `esp-soc/src/machine.rs::run_unmodeled`.
Binary and script: `runs/spec-census/esp32sim-census-cb737a14`, `runs/spec-census/spec-pt.sh`; raw: `runs/spec-census/census32.jsonl`
(cumulative line per guest second); tables: `report.py`, `slices.py` in the same folder.

Rule simulated: tumbling windows of K both-busy rounds; core 0 leads. A window conflicts if core 1 reads a unit core 0 wrote anywhere in
the window, or core 1 writes a unit core 0 read or wrote ("whole" rule, what gen-stamp bitmaps give). "tri" = exact order-aware rule
(core 1 round i vs core 0 round j>i only). Window enders: mode 0 = device-register access in the round (either core) or the next device
deadline (`bus.next_deadline()`, as `vq_quanta` bounds EX133); mode 1 = device access, or a round whose device flush changed an IRQ line or did DMA
(i.e. deadlines are crossed optimistically); mode 2 = no enders.

Round mix (31 s): both-busy rounds 56.07M (the wasm census `notes/census.md` §1b counts 54.65M in 30 s: ping-pong = **47.9% of guest time,
68.4% of instructions**; this bounds the prize), other rounds 51.3M. 14,581 both-busy episodes, mean 3,845 rounds each.

| gran | K | enders | windows | whole-rule clean | tri-rule clean | mean rounds/window | turnovers left vs lockstep |
|---:|---:|---|---:|---:|---:|---:|---:|
| 64 B | 2 | 0 dev+deadline | 27.0M | 99.99% | 100.00% | 1.99 | 0.525 |
| 64 B | 4 | 0 | 14.0M | 99.98% | 99.99% | 3.86 | 0.286 |
| 64 B | 16..256 | 0 | 14.0M | 99.98% | 99.99% | 3.86 | 0.285 |
| 64 B | 8 | 1 dev+irq/dma | 7.30M | 99.96% | 99.98% | 7.41 | 0.166 |
| 64 B | 16 | 1 | 3.99M | 99.92% | 99.97% | 13.54 | 0.107 |
| 64 B | 64 | 1 | 1.56M | 99.80% | 99.92% | 34.66 | 0.063 |
| 64 B | 256 | 1 | 0.96M | 99.67% | 99.86% | 56.20 | 0.053 |
| 256 B | 16 | 1 | 3.99M | 99.92% | 99.96% | 13.54 | 0.107 |
| 4 KiB | 4 | 0 | 14.0M | 86.75% | 90.09% | 3.86 | 0.384 |
| 4 KiB | 16 | 1 | 3.99M | 77.65% | 77.96% | 13.54 | 0.354 |

Run until first conflict, no enders at all (mode 2), 64 B: mean 1,660 rounds, 99.97% of both-busy rounds covered. **Memory sharing is
not the obstacle at 64-256 B**; 4 KiB is too coarse (the cores share pages, not lines).

Other window enders, per both-busy round: device-register access 3.55% (core 0 1.92M, core 1 only 66K in 31 s); IRQ-line change 0.77%;
DMA 0.24%; but IRQ/DMA without a device access in the same round only **0.05%** (10,937 of 22.0M rounds in the 12 s rerun `census12b.jsonl`).
Device deadline (`next_deadline/64`): 24% of rounds 1, 52% 2-3, 24% 4-7, almost nothing larger: the EX134 cadence cap of 256 cycles
keeps the deadline at <= 4 quanta. So **mode 0 (what an engine can do without snapshotting devices) caps K at ~4: 3.86 rounds per window,
71% of turnovers removed**. Mode 1 (K=16: 89% removed) would need device state rollback or a proof that the flush is silent.
Device pages (12 s, core 0): SPI2 0x60024000 3.76M, 0x6003f000 0.45M, 0x60023000 0.19M, 0x6003b000 0.15M; core 1: 0x60023000 0.14M.

Boot vs steady state (3 s slices, `slices.py census32.jsonl 3`): flat. K=4/mode 0 whole-clean 99.95-99.99% and 3.86 rounds/window in every
slice; 21-24 s is an almost single-core stretch (37K both-busy rounds) with 94-96%. No separate boot regime in this workload.

## 2. Tracking tax (queued; pure overhead, bit-exact)

Hack in `xtensa-lx7/src/jit/wasm_memory.rs::spec_tax` (end of `probe()`, so scalar and PIE fast paths both pay): core 0 code (leader)
stores the window generation into a 4 MB stamp map per access; core 1 code (follower) loads the leader map(s), compares with a generation
that never matches and `br_if`s to the slow path (never taken); stores on both cores append (host addr, old value) to a 1 MB ring.
Build knob `SPEC_MODE` bits: 1 leader marks, 2 follower checks, 4 undo log. Commit 966d447f.

## 3. Arithmetic (prediction)

EX047 on the same workload: quantum 512 (88% fewer ping-pong turnovers) took 65.4 -> 54.2 s (-17%) while retiring 7.5% more instructions,
so removing all ping-pong turnovers is worth ~17-23% of wall. Mode 0 removes 71% of them: ~12-16% before tax. Rollback rate is tiny
(conflict 0.02% of windows + core-1 device access ~0.5% of windows at K=4). Tax T: generated code is ~42% of wall; if marks/checks/undo
cost 10-20% of generated-code time, T = 4-8% of wall. Net prediction ~+5-10%: below the original 20% bar. K=16 (crossing deadlines)
would add at most ~4 points and needs device rollback: not built.
