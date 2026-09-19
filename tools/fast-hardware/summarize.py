"""Small exploratory summary of existing Tier-B NDJSON, not an accuracy gate."""
import json
import statistics
import sys
from collections import defaultdict
from pathlib import Path

rows = defaultdict(list)
for name in sys.argv[1:]:
    for line in Path(name).read_text().splitlines():
        if "TINYDRAW_TIER_B_NDJSON " not in line:
            continue
        record = json.loads(line.split("TINYDRAW_TIER_B_NDJSON ", 1)[1])
        if record.get("record") == "sample":
            rows[record["cell"]].append(record)
            if record["cell"].endswith("_sweep"):
                rows[f'{record["cell"]}/bytes={record["bytes"]}'].append(record)
result = {}
for cell, samples in rows.items():
    result[cell] = {"samples": len(samples)}
    for field in ("cycles", "bytes", "baselineCycles", "submissionCycles", "completionCycles"):
        values = [r[field] for r in samples if field in r]
        if values:
            result[cell][field] = {"min": min(values), "median": statistics.median(values), "max": max(values)}
print(json.dumps(result, indent=2, sort_keys=True))
