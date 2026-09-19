"""Subtract the shared IRAM call/prologue baseline from warmed FP blocks."""
import json
import re
import statistics
import sys
from collections import defaultdict
from pathlib import Path

expected = {
    "empty": "3f800000", "add_dep": "43808000", "add_ind4": "42820000",
    "mul_dep": "3f800000", "mul_ind4": "3f800000",
    "madd_dep": "43808000", "madd_ind4": "42820000",
    "float_add_dep": "40000000", "float_add_ind": "3f800000",
    "mul_trunc_dep": "3f800000", "mul_trunc_ind": "3f800000",
    "lsi_add_dep": "40000000", "lsi_add_gap": "40000000", "lsi_add_ind": "3f800000",
}
out = []
for name in sys.argv[1:]:
    samples = defaultdict(list)
    results = {}
    for line in Path(name).read_text().splitlines():
        match = re.match(r"FP_SAMPLE cell=(\w+) sample=(\d+) cycles=(\d+)", line)
        if match:
            samples[match[1]].append(int(match[3]))
        match = re.match(r"FP_RESULT cell=(\w+) result_bits=(\w+)", line)
        if match:
            results[match[1]] = match[2]
    base = statistics.median(samples["empty"])
    rows = {}
    for cell, values in samples.items():
        median = statistics.median(values)
        rows[cell] = {"samples": len(values), "minCycles": min(values), "medianCycles": median, "maxCycles": max(values),
                      "extraCyclesPer256InstructionBlock": (median-base)/1024,
                      "extraCyclesPerFpInstruction": (median-base)/1024/256,
                      "resultBits": results.get(cell), "expectedResultBits": expected[cell],
                      "resultMatches": results.get(cell) == expected[cell]}
    out.append({"source": name, "complete": set(samples) == set(expected) and all(len(v)==9 for v in samples.values()),
                "allResultsMatch": results == expected, "cells": rows})
print(json.dumps({"caveat": "Each nonempty block has 256 FP instructions; lsi_add_gap additionally has 128 NOPs. Baseline subtraction includes terminal result-read differences, so per-block values are primary. No interrupts inside measurements.", "boots": out}, indent=2))
