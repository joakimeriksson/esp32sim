"""Observed cache probe quantities with explicit limits on model interpretation."""
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path

cells = defaultdict(list)
services = defaultdict(list)
for boot, path in enumerate(sys.argv[1:], 1):
    for line in Path(path).read_text().splitlines():
        if "TINYDRAW_TIER_B_NDJSON " not in line:
            continue
        r = json.loads(line.split("TINYDRAW_TIER_B_NDJSON ", 1)[1])
        if r.get("record") != "sample":
            continue
        cells[r["cell"]].append(r)
        if "psramServiceCycles" in r:
            services[boot, r["psramClockHz"]].append(r)

def median(cell, field="cycles"):
    return statistics.median(r[field] for r in cells[cell])

writeback = []
for count in (1, 2, 4, 8, 16):
    clean = median(f"writeback_clean_{count}_lines")
    dirty = median(f"writeback_dirty_{count}_lines")
    writeback.append({"lines": count, "cleanCycles": clean, "dirtyCycles": dirty,
                      "extraCycles": dirty-clean, "extraCyclesPerLine": (dirty-clean)/count})
decomposed = []
for clock in (40, 80):
    for count in (1, 16, 512):
        clean = median(f"msync_decompose_l{count}_d0_p{clock}")
        dirty = median(f"msync_decompose_l{count}_d{count}_p{clock}")
        decomposed.append({"psramMHz": clock, "lines": count, "extraCyclesPerDirtyLine": (dirty-clean)/count})
service = []
for (boot, clock), rs in services.items():
    cycles = [r["psramServiceCycles"] for r in rs]
    misses = sorted(set(r["psramServiceCounters"]["dbusPsramMisses"] for r in rs))
    service.append({"boot": boot, "psramHz": clock, "samples": len(rs), "bytes": 4096,
                    "missCounts": misses, "minCycles": min(cycles), "medianCycles": statistics.median(cycles),
                    "maxCycles": max(cycles), "medianCyclesPerLineIncludingLoop": statistics.median(cycles)/64})
print(json.dumps({
    "units": "CPU cycles at 240 MHz, 64-byte data cache lines",
    "geometry": {"capacityBytes": 32768, "lineBytes": 64, "sdkconfigWays": 8,
                 "simpleModelWays": 4, "romObservation": "Supplied ROM Cache_Set_DCache_Mode ignores ways argument; fixed-four-way header agrees. Actual rev0.2 ROM not read back.",
                 "measuredAssociativity": None, "replacementPolicyMeasured": False},
    "firstColdDataLineWholeProbeCycles": {"psram": sorted(set(r["cycles"] for r in cells["first_line_d_psram"])), "flash": sorted(set(r["cycles"] for r in cells["first_line_d_flash"]))},
    "psramServiceTraversal": service,
    "explicitWritebackDelta": writeback,
    "clockDecomposedWritebackDelta": decomposed,
    "hotStore256Operations": {"psramMedianCycles": median("store_hit_psram"), "internalMedianCycles": median("store_hit_psram", "baselineCycles")},
    "roughExperimentParameters": {
        "hitExtraCycles": 0,
        "psramStreamingServiceCyclesPerLine": {"try": [150, 160, 170], "status": "effective serialized-service sensitivity sweep, not identified incremental miss latency"},
        "explicitDirtyFlushExtraCyclesPerLine80MHz": {"observedRange": [154, 161.33203125], "status": "explicit C2M flush delta, not proven automatic-eviction charge"},
        "automaticEvictionExtraCycles": None,
        "criticalWordVsFullLine": "First-line whole probe 93-96 cycles versus steady 166.734-169.688 cycles/line indicates a single fixed latency does not represent both observations; overlap/critical-word-first is an inference."
    },
    "limits": [
        "No matched warm-load traversal baseline, so incremental fill latency cannot be isolated from CPU loop and pipeline overlap.",
        "Cold stride reads only one byte per cache line; its 4096-byte coverage forces 64 line fills, not 4096 load bytes.",
        "Explicit dirty-minus-clean flush controls identify marginal flushed-line work but not demand-eviction overlap or stalls.",
        "No set-conflict or way-count sweep exists in these cohorts; geometry and replacement cannot be inferred empirically.",
        "Both cores share counter registers; concurrent cells prove contention patterns, not a universal per-request arbitration cost.",
        "PIE vector issue throughput is not isolated by these scalar load/store cells."
    ]
}, indent=2))
