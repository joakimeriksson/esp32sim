# Overnight bench results

Updated 2026-09-20 14:03:47. One pair per pass, alternating base/candidate, serialized. "red%" = wall-time reduction vs base (positive = faster).

## A/A controls (base vs base): the noise floor

| run | base s | cand s | red% |
|---|---:|---:|---:|
| CONTROL-aa--c000 | 72.87 | 73.18 | -0.43 |
| CONTROL-aa--c001 | 74.54 | 71.09 | +4.63 |
| CONTROL-aa--c002 | 70.32 | 72.05 | -2.46 |
| CONTROL-aa--c003 | 73.66 | 73.16 | +0.68 |
| CONTROL-aa--c004 | 74.49 | 73.85 | +0.87 |
| CONTROL-aa--c005 | 75.49 | 76.19 | -0.92 |
| CONTROL-aa--c006 | 72.72 | 73.42 | -0.96 |

Control spread: -2.46% .. +4.63%

## Candidates (sorted by mean reduction)

| job | agent | mean red% | per-pair red% | base s -> cand s | exact/passed | tinydraw red% | note |
|---|---|---:|---|---|---|---|---|
| kernel-s1s2s3-cbc02ff6-pocket-tank | kernel | +12.10 | +12.10 | 73.8->64.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact kernel-s1s2s3, see its agent note/commits |
| kernel-s1-f2717977-pocket-tank | kernel | +12.02 | +12.02 | 72.6->63.9 | True | - | EX156 s1: guarded block body: resumed/cut block calls enter via br_table and pay one STOP compare per instruction instead of checked-body entry/budget/LEND/pending checks; LEND hinted at compile time gives one static loop-end site. Census (12 guest s): checked body carried 1.44G of 4.28G insns. Commit 94f2cef4 on base. |
| kernel-s1s2-b2a0ac89-pocket-tank | kernel | +11.92 | +11.92 | 73.3->64.5 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact kernel-s1s2, see its agent note/commits |
| coverage-s1-be80f476-pocket-tank | coverage | +10.72 | +10.72 | 73.9->66.0 | True | - | EX155 s1: inline WUR/RUR SAR_BYTE + ee.src.q[.qup], ee.vsr/vsl.32, vsubs/vadds s8/s16, vmin/vmax: core-1 4-bit unpack kernel compiles (interp 208.9M->14.1M insns). On plain base. |
| onecall-s1r-7067f8d2-pocket-tank | onecall | +10.51 | +10.51 | 73.6->65.9 | True | - | EX153 s1r: s1 + chain also after non-trapping RET/RETW helpers. onecall 21dc368b. wrapper calls 398M->165M |
| deadlines-a-54ad01a7-pocket-tank | deadlines | +10.11 | +10.11 | 72.3->65.0 | True | - | EX157 s1: passive GDMA IN channel no longer holds the 256-cycle cadence guard (native census: 26.9M of 27.0M due flushes were held only by gdma.inp running; flushes 29.2M->2.7M). commit fb506140 on base |
| deadlines-u256k-b391b6b0-pocket-tank | deadlines | +9.78 | +9.78 | 72.9->65.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact deadlines-u256k, see its agent note/commits |
| onecall-c1-6125df00-pocket-tank | onecall | +9.46 | +9.46 | 73.3->66.4 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact onecall-c1, see its agent note/commits |
| deadlines-q60k-b1c22417-pocket-tank | deadlines | +9.29 | +9.29 | 73.6->66.7 | True | - | EX157 s1+quiet cap 59904 (ESP32SIM_DEFER_BUILD=59904, just under one USB SOF period): stacks on deadlines-a (fb506140). a-cnt: VQ runs mostly hit the 32768 quiet cap (k 256-511 quanta); this ~doubles it |
| deadlines-u64k-05ed799a-pocket-tank | deadlines | +8.45 | +8.45 | 71.8->65.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact deadlines-u64k, see its agent note/commits |
| flags-inl4000-8c091acf-pocket-tank | flags | +6.32 | +6.32 | 72.9->68.3 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-inl4000, see its agent note/commits |
| flags-inl2000-5ce976e6-pocket-tank | flags | +5.09 | +5.09 | 74.1->70.3 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-inl2000, see its agent note/commits |
| turnover-s1-b505b798-pocket-tank | turnover | +4.45 | +4.45 | 72.9->69.6 | True | - | EX152 s1: run_many - consecutive blocks inside Cpu (block.rs run_blocks) instead of returning to machine step_blocks per block; lean flag when no probes/stubs/observers; alone on base |
| flags-simd-inl1000-b1686279-pocket-tank | flags | +4.35 | +4.35 | 74.5->71.3 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-simd-inl1000, see its agent note/commits |
| onecall-s2-18d20624-pocket-tank | onecall | +3.73 | +3.73 | 74.5->71.8 | True | - | EX153 s2: s1r chain + checked chunk copies inside regions (region cuts itself, per-instruction spill at each cut point; region bytes ~4x). onecall 6d36fd9b --features ex153-checked |
| onecall-s1-b0dbc7f7-pocket-tank | onecall | +3.38 | +3.38 | 74.3->71.8 | True | - | EX153 s1: wrapper chains compiled calls (plain END/LEFT, no interpreter helper; helpers incl. RETW break the chain). onecall 74a3000e. counters: wrapper calls 398M->194M |
| flags-nodebug-f33104f1-pocket-tank | flags | +3.34 | +3.34 | 77.4->74.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-nodebug, see its agent note/commits |
| frames-s3-0ac83196-pocket-tank | frames | +3.21 | +3.21 | 74.9->72.5 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact frames-s3, see its agent note/commits |
| frames-b-oldcadence-92243a4f-pocket-tank | frames | +2.93 | +2.93 | 75.4->73.1 | True | - | EX158 probe B (upper bound): pre-PR89 AMOLED cadence 50 Hz + quiet-interval deferral; exact, 737 frames vs 3094 |
| onecall-s2b-9d3496a5-pocket-tank | onecall | +2.91 | +2.91 | 73.5->71.4 | True | - | EX153 s2b: s2 + short-credit dispatch enters the chunk's checked copy (no budget_lt_len gate). onecall 6d36fd9b --features ex153-short. counters: block tail-cut calls 82.9M->7.7M, wrapper calls 398M->165M |
| flags-unroll600-4a58c696-pocket-tank | flags | +2.71 | +2.71 | 75.8->73.7 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-unroll600, see its agent note/commits |
| flags-all-e971ebbf-pocket-tank | flags | +2.41 | +2.41 | 75.4->73.6 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-all, see its agent note/commits |
| kernel-s2-02afd0fa-pocket-tank | kernel | +2.34 | +2.34 | 75.7->73.9 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact kernel-s2, see its agent note/commits |
| flags-sinl-O3-2f1ed3c9-pocket-tank | flags | +2.18 | +2.18 | 74.4->72.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-sinl-O3, see its agent note/commits |
| frames-s1b-155f7517-pocket-tank | frames | +2.01 | +2.01 | 76.1->74.5 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact frames-s1b, see its agent note/commits |
| frames-a-nopub-bebd6b97-pocket-tank | frames | +1.85 | +1.85 | 74.8->73.4 | True | - | EX158 probe A (upper bound, not a candidate): no display frame publication at all; exact, 0 binary frames vs 3094 |
| hwloop-s2b-b924ac73-pocket-tank | hwloop | +1.76 | +1.76 | 73.8->72.5 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact hwloop-s2b, see its agent note/commits |
| spec-tax1-f1687a3f-pocket-tank | spec | +1.72 | +1.72 | 76.7->75.4 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact spec-tax1, see its agent note/commits |
| calls-s2-3b7c7cd5-pocket-tank | calls | +1.59 | +1.59 | 71.6->70.5 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact calls-s2, see its agent note/commits |
| flags-inline1000-5dc81c51-pocket-tank | flags | +0.85 | +0.85 | 72.9->72.3 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-inline1000, see its agent note/commits |
| hwloop-s2be-efa41ae8-pocket-tank | hwloop | +0.63 | +0.63 | 72.8->72.3 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact hwloop-s2be, see its agent note/commits |
| flags-simd-78be7c25-pocket-tank | flags | +0.49 | +0.49 | 73.2->72.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-simd, see its agent note/commits |
| flags-inl500-c020cd86-pocket-tank | flags | +0.45 | +0.45 | 72.7->72.4 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-inl500, see its agent note/commits |
| flags-inl250-a5094223-pocket-tank | flags | +0.36 | +0.36 | 72.0->71.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-inl250, see its agent note/commits |
| frames-s1-d8f187d9-pocket-tank | frames | +0.35 | +0.35 | 73.7->73.4 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact frames-s1, see its agent note/commits |
| flags-simd-extc-0201ac68-pocket-tank | flags | -0.19 | -0.19 | 74.5->74.7 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-simd-extc, see its agent note/commits |
| flags-sinl-O4-73ac2afb-pocket-tank | flags | -0.21 | -0.21 | 74.1->74.3 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-sinl-O4, see its agent note/commits |
| frames-s1s3-a4640d81-pocket-tank | frames | -0.29 | -0.29 | 74.4->74.7 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact frames-s1s3, see its agent note/commits |
| frames-s1a-75a1048a-pocket-tank | frames | -0.37 | -0.37 | 73.1->73.3 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact frames-s1a, see its agent note/commits |
| flags-base-O3inl-cfc3d6c9-pocket-tank | flags | -0.48 | -0.48 | 73.8->74.2 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-base-O3inl, see its agent note/commits |
| calls-s3-39e89862-pocket-tank | calls | -0.75 | -0.75 | 73.1->73.6 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact calls-s3, see its agent note/commits |
| flags-abort-cdd97cf4-pocket-tank | flags | -0.88 | -0.88 | 71.2->71.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-abort, see its agent note/commits |
| flags-simd-O3-5ba54786-pocket-tank | flags | -0.93 | -0.93 | 76.3->77.0 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-simd-O3, see its agent note/commits |
| flags-base-O3-f38df6bc-pocket-tank | flags | -0.94 | -0.94 | 73.2->73.8 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-base-O3, see its agent note/commits |
| calls-s1s2a-db128518-pocket-tank | calls | -1.37 | -1.37 | 73.4->74.4 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact calls-s1s2a, see its agent note/commits |
| kernel-s2s3-a35c7315-pocket-tank | kernel | -1.46 | -1.46 | 72.5->73.5 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact kernel-s2s3, see its agent note/commits |
| calls-s1s2as3-f4108bf2-pocket-tank | calls | -1.61 | -1.61 | 71.2->72.3 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact calls-s1s2as3, see its agent note/commits |
| flags-base-O4-33b86dcc-pocket-tank | flags | -1.96 | -1.96 | 73.1->74.5 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-base-O4, see its agent note/commits |
| calls-s1-09cc6fa1-pocket-tank | calls | -2.03 | -2.03 | 72.6->74.1 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact calls-s1, see its agent note/commits |
| flags-all-nosimd-08809cfd-pocket-tank | flags | -2.29 | -2.29 | 72.7->74.4 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-all-nosimd, see its agent note/commits |
| flags-bulk-e3b68a44-pocket-tank | flags | -2.85 | -2.85 | 73.3->75.4 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-bulk, see its agent note/commits |
| flags-simd-tail-a0d50c14-pocket-tank | flags | -2.95 | -2.95 | 74.7->76.9 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-simd-tail, see its agent note/commits |
| spec-tax2-a31e1614-pocket-tank | spec | -3.63 | -3.63 | 72.6->75.2 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact spec-tax2, see its agent note/commits |
| onecall-s2only-602b1cbf-pocket-tank | onecall | -7.11 | -7.11 | 71.0->76.0 | True | - | EX153 s2only: checked copies + short entry WITHOUT wrapper chaining (isolates (b)). onecall 6d36fd9b --features ex153-short,ex153-nochain |
| spec-tax-e23efb72-pocket-tank | spec | -12.38 | -12.38 | 72.3->81.2 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact spec-tax, see its agent note/commits |
| flags-opts-a03c18e9-pocket-tank | flags | -12.79 | -12.79 | 75.0->84.6 | True | - | auto-queued by exact-all.sh after EXACT ok; artifact flags-opts, see its agent note/commits |
| spec-tax3-30114a9f-pocket-tank | spec | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact spec-tax3, see its agent note/commits |
| spec-tax4-7ea8bc5c-pocket-tank | spec | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact spec-tax4, see its agent note/commits |
| spec-tax7-70697d75-pocket-tank | spec | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact spec-tax7, see its agent note/commits |
| spills-m1-f0efcd1b-pocket-tank | spills | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact spills-m1, see its agent note/commits |
| spills-m2-8ef8ff94-pocket-tank | spills | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact spills-m2, see its agent note/commits |
| turnover-s1s2-6ddaceff-pocket-tank | turnover | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact turnover-s1s2, see its agent note/commits |
| turnover-s1s2s3-16a3e375-pocket-tank | turnover | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact turnover-s1s2s3, see its agent note/commits |
| turnover-s2-c58b96eb-pocket-tank | turnover | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact turnover-s2, see its agent note/commits |
| turnover-s3-b3fefc2f-pocket-tank | turnover | n/a | not run yet | | | | auto-queued by exact-all.sh after EXACT ok; artifact turnover-s3, see its agent note/commits |
