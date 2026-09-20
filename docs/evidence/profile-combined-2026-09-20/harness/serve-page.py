"""Serve the production page with a local benchmark firmware manifest.

python3 tools/browser-benchmark/serve-page.py ASSETS.json --wasm CORRECTED.wasm
"""
import argparse
import hashlib
import http.server
import json
import pathlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('assets', type=pathlib.Path)
parser.add_argument('--wasm', type=pathlib.Path)
parser.add_argument('--port', type=int, default=8810)
args = parser.parse_args()
root = pathlib.Path(__file__).resolve().parents[2]
assets = {key: pathlib.Path(value) for key, value in json.loads(args.assets.read_text()).items()}
if args.wasm:
    assets['wasm'] = args.wasm.resolve()

class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *a, **kw):
        super().__init__(*a, directory=str(root / 'web'), **kw)

    def do_GET(self):
        path = self.path.split('?')[0]
        if path == '/wasm/fw/page-check.json':
            value = {'board': 'waveshare-amoled18-v2', 'flash_mb': 16, 'psram_mb': 8,
                     'files': {key: f'asset/{key}' for key in assets if key != 'wasm'}}
            data, kind = json.dumps(value).encode(), 'application/json'
        elif path == '/provenance.json':
            files = {f'asset/{key}': value for key, value in assets.items()}
            files.update({str(p.relative_to(root)): p for p in (root / 'web').rglob('*')
                          if p.is_file() and p.suffix in ('.js', '.mjs', '.html')})
            value = {'sha256': {key: hashlib.sha256(p.read_bytes()).hexdigest() for key, p in sorted(files.items())}}
            data, kind = json.dumps(value).encode(), 'application/json'
        elif path == '/wasm/esp32sim.wasm' or path.startswith('/wasm/fw/asset/'):
            key = 'wasm' if path == '/wasm/esp32sim.wasm' else path.removeprefix('/wasm/fw/asset/')
            if key not in assets:
                self.send_error(404)
                return
            try:
                data = assets[key].read_bytes()
            except FileNotFoundError:
                self.send_error(404)
                return
            kind = 'application/wasm' if key == 'wasm' else 'application/octet-stream'
        else:
            return super().do_GET()
        self.send_response(200)
        self.send_header('Content-Type', kind)
        self.send_header('Content-Length', str(len(data)))
        self.send_header('Cache-Control', 'no-store')
        self.end_headers()
        self.wfile.write(data)

http.server.ThreadingHTTPServer(('127.0.0.1', args.port), Handler).serve_forever()
