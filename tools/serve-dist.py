"""Static server for web/dist with an SPA fallback.

    usage:  python3 tools/serve-dist.py web/dist 8900
            then open http://127.0.0.1:8900

Build dist first (needs Node 22 — see web/.nvmrc):
    export PATH="$HOME/.cargo/bin:$PATH"
    wasm-pack build crates/frees-wasm --release --target web --out-dir ../../web/src/wasm/pkg
    cd web && npm ci && npm run build


`python3 -m http.server` 404s on client-side routes like /help (a prior agent
hit exactly that), so anything without a file extension falls back to
index.html. .wasm is served as application/wasm so streaming instantiate works.
"""
import http.server
import os
import sys

root = sys.argv[1]
port = int(sys.argv[2])


class H(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        ".wasm": "application/wasm",
        ".js": "text/javascript",
        ".mjs": "text/javascript",
    }

    def __init__(self, *a, **k):
        super().__init__(*a, directory=root, **k)

    def do_GET(self):
        p = self.translate_path(self.path)
        if not os.path.exists(p) and "." not in os.path.basename(self.path.split("?")[0]):
            self.path = "/index.html"  # SPA fallback for extensionless routes
        return super().do_GET()

    def log_message(self, fmt, *a):
        sys.stderr.write("%s\n" % (fmt % a))


http.server.ThreadingHTTPServer.allow_reuse_address = True
sys.stderr.write("serving %s on http://127.0.0.1:%d\n" % (root, port))
sys.stderr.flush()
http.server.ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()
