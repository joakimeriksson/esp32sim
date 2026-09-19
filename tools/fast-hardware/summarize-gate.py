"""Extract firmware timers and host-observed milestone times without claiming equivalence."""
import json
import sys
from pathlib import Path

records = [json.loads(line) for line in Path(sys.argv[1]).read_text().splitlines()]
timings = []
markers = {}
verdict = None
restore = None
for r in records:
    line = r["line"]
    tokens = line.split()
    if not tokens:
        continue
    marker = tokens[0]
    if marker.startswith("TINYDRAW_"):
        markers.setdefault(marker, []).append(r["hostSinceResetCommandNs"] / 1e9)
    fields = {}
    for token in tokens[1:]:
        if "=" not in token:
            continue
        key, value = token.split("=", 1)
        try:
            value = int(value)
        except ValueError:
            pass
        fields[key] = value
    if marker == "TINYDRAW_GATE1_AUTOMATED_DONE":
        verdict = fields
    if marker == "TINYDRAW_AUTOSAVE_RESTORE":
        restore = fields
    if marker.startswith("TINYDRAW_") and any(k.endswith("_us") for k in fields):
        timings.append({"marker": marker, "hostSinceResetCommandSeconds": r["hostSinceResetCommandNs"] / 1e9, **fields})
done = markers.get("TINYDRAW_GATE1_AUTOMATED_DONE", [None])[0]
startup = next((r["hostSinceResetCommandSeconds"] for r in timings if r["marker"] == "TINYDRAW_LIVE_PRESENT" and r.get("kind") == "startup"), None)
print(json.dumps({
    "source": sys.argv[1],
    "hostTimingCaveat": "Serial receive times include USB buffering and printing. Reset command origin precedes actual reset. Startup-to-verdict brackets the battery but is not a firmware timer.",
    "autosaveRestore": restore,
    "automatedVerdict": verdict,
    "allAutomatedGatesPassed": verdict is not None and len([v for k, v in verdict.items() if k != "ssaa_receipt"]) == 36 and all(v == 1 for k, v in verdict.items() if k != "ssaa_receipt") and verdict.get("ssaa_receipt") == "yellow",
    "startupSerialToVerdictSeconds": done - startup if done is not None and startup is not None else None,
    "markerTimesSeconds": markers,
    "firmwareTimingRecords": timings,
}, indent=2))
