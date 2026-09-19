"""Capture selected existing Tier-B cells with this board's watchdog reset quirk.

Run using uv in a venv with esptool and pyserial installed. Does not flash.
"""
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
p.add_argument("--cells", required=True)
p.add_argument("--timeout", type=float, default=90)
a = p.parse_args()
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
sent = False
finished = False
deadline = time.monotonic() + a.timeout
with a.output.open("wb") as out:
    while time.monotonic() < deadline:
        line = device.readline()
        if not line:
            continue
        out.write(line)
        out.flush()
        if b"TINYDRAW_TIER_B_SELECT_READY" in line and not sent:
            device.write(f"TIER_B_SELECT {a.cells}\n".encode())
            sent = True
        if b'"record":"run-complete"' in line:
            finished = True
            deadline = time.monotonic() + 1
device.close()
print(json.dumps({"selectionSent": sent, "suiteComplete": finished, "output": str(a.output)}))
sys.exit(0 if finished else 2)
