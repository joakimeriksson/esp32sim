"""Serve local benchmark assets. Usage: python3 tools/browser-benchmark/serve.py assets.json."""
import argparse
import http.server
import json
import pathlib
import hashlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('assets', type=pathlib.Path)
parser.add_argument('--port', type=int, default=8792)
parser.add_argument('--web-root', type=pathlib.Path, help='Checkout containing the worker and its JavaScript modules')
args = parser.parse_args()
assets = json.loads(args.assets.read_text())
root = (args.web_root or pathlib.Path(__file__).resolve().parents[2]).resolve()

class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        name = self.path.split('?')[0]
        if name == '/provenance.json':
            # Record transitive worker dependencies as well as the entry point.
            files = {f'asset/{key}': pathlib.Path(value) for key, value in assets.items()}
            files.update({str(path.relative_to(root)): path
                          for path in (root / 'web/wasm').rglob('*')
                          if path.is_file() and path.suffix in ('.js', '.mjs')})
            files.update({f'harness/{path.name}': path
                          for path in pathlib.Path(__file__).parent.iterdir()
                          if path.suffix in ('.mjs', '.html', '.json')})
            try:
                hashes = {key: hashlib.sha256(path.read_bytes()).hexdigest()
                          for key, path in sorted(files.items())}
            except (FileNotFoundError, IsADirectoryError):
                self.send_error(404)
                return
            data = json.dumps({'sha256': hashes}).encode()
            kind = 'application/json'
        elif name == '/assets.json':
            data = json.dumps(assets).encode()
            kind = 'application/json'
        else:
            if name.startswith('/asset/'):
                path = pathlib.Path(assets.get(name[7:], '/nonexistent'))
                kind = 'application/octet-stream'
            elif name.startswith('/web/wasm/'):
                path = (root / name.lstrip('/')).resolve()
                if not path.is_relative_to(root / 'web/wasm'):
                    self.send_error(404)
                    return
                kind = 'text/javascript'
            else:
                leaf = name.lstrip('/') or 'response.html'
                if leaf not in ('response.html', 'response.mjs', 'battery.html', 'battery.mjs', 'battery-worker.mjs', 'verdict.mjs', 'verdict-schema.json'):
                    self.send_error(404)
                    return
                path = pathlib.Path(__file__).parent / leaf
                kind = ('text/html' if leaf.endswith('.html') else
                        'application/json' if leaf.endswith('.json') else 'text/javascript')
            try:
                data = path.read_bytes()
            except (FileNotFoundError, IsADirectoryError):
                self.send_error(404)
                return
        self.send_response(200)
        self.send_header('Content-Type', kind)
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass

http.server.ThreadingHTTPServer(('127.0.0.1', args.port), Handler).serve_forever()
