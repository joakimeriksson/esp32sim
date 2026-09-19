"""Compare selected hardware and simulator firmware counters, not host throughput."""
import json
import sys
from pathlib import Path

def parse(text):
    out = {}
    for line in text.splitlines():
        tokens = line.split()
        if not tokens or tokens[0] not in selected:
            continue
        d = {}
        for token in tokens[1:]:
            if "=" in token:
                k, v = token.split("=", 1)
                try:
                    v = int(v)
                except ValueError:
                    pass
                d[k] = v
        keys = identity + (["operations", "samples"] if tokens[0] == "TINYDRAW_GATE1_HARD" else [])
        key = " ".join([tokens[0]] + [f"{k}={d[k]}" for k in keys if k in d])
        out[key] = d
    return out

selected = {
    "TINYDRAW_GATE1_PANEL_STAGE_AB": ["linear_pie_us", "linear_scalar_us", "ring_pie_us", "ring_scalar_us"],
    "TINYDRAW_GATE1_PACED_COLD": ["compute_us", "present_us", "wall_us"],
    "TINYDRAW_GATE1_HARD": ["total_us"],
    "TINYDRAW_GATE1_EXPORT": ["elapsed_us"],
    "TINYDRAW_INKTRACE": ["e2c_p95", "e2d_p95", "drain_total_us"],
    "TINYDRAW_GATE1_WORKLOAD": ["load_us"],
}
identity = ["corpus", "zoom", "trace", "kind"]
hw = parse(Path(sys.argv[1]).read_text())
baseline_events = json.loads(Path(sys.argv[2]).read_text())
base = parse("".join(r["data"] for r in baseline_events if r.get("type") == "serial"))
cpi = parse(json.loads(Path(sys.argv[3]).read_text())["serial"])
rows = []
for key, h in hw.items():
    if key not in base:
        continue
    b, c = base[key], cpi.get(key, {})
    mismatches = {k: {"hardware": h.get(k), "baseline": b.get(k), "cpi2": c.get(k)}
                  for k in ("operations", "samples", "events", "consumed", "coalesced", "steps", "tiles", "rendered", "encoded", "svg_bytes", "png_bytes")
                  if k in h and (h.get(k) != b.get(k) or (c and h.get(k) != c.get(k)))}
    for field in selected[key.split()[0]]:
        if not all(isinstance(d.get(field), int) and d[field] > 0 for d in (h, b)):
            continue
        rows.append({"record": key, "field": field, "hardwareUs": h[field], "baselineUs": b[field], "cpi2Us": c.get(field),
                     "baselineOverHardware": round(b[field] / h[field], 4), "cpi2OverHardware": round(c[field] / h[field], 4) if c.get(field) else None,
                     "workloadDifferences": mismatches})
rows.sort(key=lambda r: abs(1 - r["baselineOverHardware"]), reverse=True)
print(json.dumps({"caveat": "One hardware run, restored drawing differs. WorkloadDifferences flags selected count differences, not proof of complete state equality. Ratios are firmware timer ratios, not execution throughput.", "rows": rows}, indent=2))
