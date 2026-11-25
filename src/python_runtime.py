# Runtime Python del WCS (incorporato nel binario Rust).
# Carica il modulo di regole dell'utente e risponde a chiamate JSON-lines su stdin/stdout.
# Le print() degli script finiscono nel log del WCS (stderr).
import importlib.util
import json
import sys
import time
import traceback

PATH = sys.argv[1]
_out = sys.stdout
sys.stdout = sys.stderr


class Ctx:
    """Contesto passato alle funzioni di script.

    ctx.tags        dict nome_tag -> valore (snapshot al momento della chiamata)
    ctx.tag(name)   valore di un tag (None se non esiste)
    ctx.write(t, v) scrive un tag sul PLC (eseguito dal WCS a fine funzione)
    ctx.log(msg)    messaggio nel log eventi del WCS
    ctx.now         timestamp (secondi epoch)
    ctx.point       nome del punto di decisione / evento che ha generato la chiamata
    """

    def __init__(self, d):
        self.tags = d.get("tags", {})
        self.point = d.get("point")
        self.user = d.get("user")
        self.now = time.time()
        self._writes = []
        self._logs = []

    def tag(self, name, default=None):
        return self.tags.get(name, default)

    def write(self, tag, value):
        self._writes.append([tag, value])
        self.tags[tag] = value

    def log(self, msg):
        self._logs.append(str(msg))


def _load():
    spec = importlib.util.spec_from_file_location("wcs_rules", PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _send(obj):
    _out.write(json.dumps(obj, default=str) + "\n")
    _out.flush()


try:
    MOD, LOAD_ERR = _load(), None
except Exception:
    MOD, LOAD_ERR = None, traceback.format_exc()

_send({"ready": LOAD_ERR is None, "error": LOAD_ERR,
       "functions": sorted(n for n in dir(MOD) if not n.startswith("_") and callable(getattr(MOD, n)))
       if MOD else []})

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    mid = msg.get("id")
    ctx = Ctx(msg.get("ctx") or {})
    try:
        if LOAD_ERR:
            raise RuntimeError("modulo non caricato:\n" + LOAD_ERR)
        fn = getattr(MOD, msg["fn"], None)
        if not callable(fn):
            raise NameError("funzione '%s' non definita in %s" % (msg["fn"], PATH))
        res = fn(msg.get("payload"), ctx)
        _send({"id": mid, "ok": True, "result": res, "writes": ctx._writes, "logs": ctx._logs})
    except Exception as e:
        _send({"id": mid, "ok": False, "error": "%s: %s" % (type(e).__name__, e),
               "trace": traceback.format_exc(), "logs": ctx._logs})
