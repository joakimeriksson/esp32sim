"""Capture unmodified TinyDraw gate firmware with host monotonic line timestamps."""
import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

import serial

p = argparse.ArgumentParser()
p.add_argument("output", type=Path)
p.add_argument("--port", default="/dev/cu.usbmodem101")
p.add_argument("--timeout", type=float, default=300)
p.add_argument("--done-marker", default="TINYDRAW_GATE1_AUTOMATED_DONE")
p.add_argument("--ready-marker", default="TINYDRAW_VECTOR_V2_READY")
a = p.parse_args()
t0 = time.monotonic_ns()
reset = subprocess.run([sys.executable, "-m", "esptool", "--chip", "esp32s3", "--port", a.port,
                        "--after", "watchdog-reset", "chip-id"], capture_output=True, text=True, check=True)
a.output.with_suffix(".reset.log").write_text(reset.stdout + reset.stderr)
device = serial.Serial()
device.port, device.baudrate, device.timeout = a.port, 115200, 0.25
device.dtr = device.rts = False
deadline = time.monotonic() + 10
while True:
    try:
        device.open()
        break
    except (OSError, serial.SerialException):
        if time.monotonic() > deadline:
            raise
        time.sleep(0.1)
done = ready = False
deadline = time.monotonic() + a.timeout
with a.output.open("wb") as out, a.output.with_suffix(".timestamps.jsonl").open("w") as stamps:
    while time.monotonic() < deadline:
        line = device.readline()
        if not line:
            continue
        ns = time.monotonic_ns()
        out.write(line)
        out.flush()
        stamps.write(json.dumps({"hostSinceResetCommandNs": ns - t0, "line": line.decode(errors="replace").rstrip()}) + "\n")
        stamps.flush()
        if a.done_marker.encode() in line:
            done = True
        if a.ready_marker.encode() in line:
            ready = True
            deadline = time.monotonic() + 2
device.close()
print(json.dumps({"automatedDone": done, "interactiveReady": ready, "output": str(a.output)}))
sys.exit(0 if done and ready else 2)
