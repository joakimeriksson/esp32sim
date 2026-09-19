"""Firmware-timer ratios: emulator console vs hardware console (same firmware, same erased start)."""
import json,sys,statistics
from pathlib import Path
selected = {"TINYDRAW_GATE1_PANEL_STAGE_AB": ["linear_pie_us","linear_scalar_us","ring_pie_us","ring_scalar_us"],
 "TINYDRAW_GATE1_PACED_COLD": ["compute_us","present_us","wall_us"], "TINYDRAW_GATE1_HARD": ["total_us"],
 "TINYDRAW_GATE1_EXPORT": ["elapsed_us"], "TINYDRAW_GATE1_WORKLOAD": ["load_us"]}
identity=["corpus","zoom","trace","kind"]
def parse(text):
    out={}
    for line in text.splitlines():
        t=line.split()
        if not t or t[0] not in selected: continue
        d={}
        for tok in t[1:]:
            if "=" in tok:
                k,v=tok.split("=",1)
                try: v=int(v)
                except ValueError: pass
                d[k]=v
        keys=identity+(["operations","samples"] if t[0]=="TINYDRAW_GATE1_HARD" else [])
        out[" ".join([t[0]]+[f"{k}={d[k]}" for k in keys if k in d])]=d
    return out
hw=parse(Path(sys.argv[1]).read_text(errors="replace"))
arms={}
for name,path in (a.split("=",1) for a in sys.argv[2:]):
    ev=json.loads(Path(path).read_text()); arms[name]=parse("".join(r["data"] for r in ev if r.get("type")=="serial"))
rows=[]; per={n:{} for n in arms}
for key,h in hw.items():
    for f in selected[key.split()[0]]:
        if not isinstance(h.get(f),int) or h[f]<=0: continue
        row={"record":key,"field":f,"hw_us":h[f]}
        for n,a in arms.items():
            v=a.get(key,{}).get(f)
            if isinstance(v,int) and v>0: row[n]=round(v/h[f],3); per[n].setdefault(f,[]).append(v/h[f])
        rows.append(row)
for r in rows: print(r)
print("--- median emulator/hardware ratio per field")
for n,fs in per.items():
    print(n,{f:(round(statistics.median(v),3),len(v)) for f,v in fs.items()})
