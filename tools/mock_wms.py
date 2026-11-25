"""WMS di prova per l'integrazione WCS ↔ WMS.

Endpoint (porta default 8090):
  GET /api/parcels/<barcode>/destination  -> {"destination": "...", "class": "..."} oppure 404
  GET /api/sort-classes                   -> [{"name", "destination", "barcodes", "patterns"}]

Uso: python tools/mock_wms.py [porta]
"""
import json, sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import unquote

# destinazioni puntuali note al WMS (es. ordini con uscita forzata)
PARCELS = {
    "IT-MI-000999": {"destination": "2", "class": "RIASSEGNATO"},
    "XX-HEAVY-01": {"destination": "3", "class": "SPECIALE"},
}
# appartenenza della classe gestita dal WMS
CLASSES = [
    {"name": "PRIORITARI", "destination": "1", "barcodes": ["IT-NA-990011", "FR-PA-556677"]},
]

class H(BaseHTTPRequestHandler):
    def _send(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        p = self.path.split("?")[0]
        if p == "/api/sort-classes":
            return self._send(200, CLASSES)
        if p.startswith("/api/parcels/") and p.endswith("/destination"):
            bc = unquote(p[len("/api/parcels/"):-len("/destination")])
            if bc in PARCELS:
                return self._send(200, PARCELS[bc])
            return self._send(404, {"error": "barcode sconosciuto"})
        self._send(404, {"error": "endpoint sconosciuto"})

    def log_message(self, fmt, *args):
        sys.stderr.write("[wms] " + fmt % args + "\n")

port = int(sys.argv[1]) if len(sys.argv) > 1 else 8090
print(f"WMS di prova su http://127.0.0.1:{port}/api", flush=True)
ThreadingHTTPServer(("127.0.0.1", port), H).serve_forever()
