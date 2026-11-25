# Runtime Python del WCS (incorporato nel binario Rust).
# Carica gli script dell'utente e risponde a chiamate JSON-lines su stdin/stdout.
#
# Il percorso può essere una cartella (un file per funzione: `nome.py` definisce
# `def nome(payload, ctx)`; i file `_xxx.py` sono moduli condivisi importabili
# dagli script) oppure un singolo modulo con più funzioni (compatibilità).
# Le print() degli script finiscono nel log del WCS (stderr); in modalità test
# vengono catturate e restituite.
import importlib
import importlib.util
import io
import json
import os
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
    ctx.user        utente (eventi HMI)
    """

    def __init__(self, d):
        self.tags = d.get("tags", {})
        self.point = d.get("point")
        self.user = d.get("user")
        self.now = time.time()
        self._writes = []
        self._logs = []

    def tag(self, name, default=None):
        v = self.tags.get(name)
        return default if v is None else v

    def write(self, tag, value):
        self._writes.append([tag, value])
        self.tags[tag] = value

    def log(self, msg):
        self._logs.append(str(msg))


FUNCS = {}      # nome -> callable
FILES = []      # informazioni sui file caricati
ERRORS = []


def _doc(obj):
    d = (getattr(obj, "__doc__", None) or "").strip()
    return d.split("\n\n")[0] if d else ""


def _load_dir(d):
    sys.path.insert(0, d)
    names = sorted(f[:-3] for f in os.listdir(d) if f.endswith(".py") and not f.startswith("."))
    # prima i moduli condivisi, poi le funzioni
    for name in sorted(names, key=lambda n: (not n.startswith("_"), n)):
        info = {"name": name, "kind": "module" if name.startswith("_") else "function", "error": None, "doc": "", "functions": []}
        try:
            mod = importlib.import_module(name)
            info["doc"] = _doc(mod)
            if info["kind"] == "function":
                fn = getattr(mod, name, None)
                if not callable(fn):
                    raise NameError("il file %s.py deve definire def %s(payload, ctx)" % (name, name))
                FUNCS[name] = fn
                info["functions"] = [name]
                info["doc"] = _doc(fn) or info["doc"]
        except Exception:
            info["error"] = traceback.format_exc()
            ERRORS.append("%s.py:\n%s" % (name, info["error"]))
        FILES.append(info)


def _load_file(p):
    try:
        spec = importlib.util.spec_from_file_location("wcs_rules", p)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        for n in dir(mod):
            if not n.startswith("_") and callable(getattr(mod, n)) and getattr(getattr(mod, n), "__module__", None) == "wcs_rules":
                FUNCS[n] = getattr(mod, n)
        FILES.append({"name": os.path.basename(p)[:-3], "kind": "module", "error": None, "doc": _doc(mod), "functions": sorted(FUNCS)})
    except Exception:
        ERRORS.append(traceback.format_exc())


def _send(obj):
    _out.write(json.dumps(obj, default=str) + "\n")
    _out.flush()


if os.path.isdir(PATH):
    _load_dir(PATH)
else:
    _load_file(PATH)
LOAD_ERR = "\n".join(ERRORS) or None

_send({"ready": not ERRORS, "error": LOAD_ERR, "functions": sorted(FUNCS), "files": FILES})

for line in sys.stdin:
    try:
        msg = json.loads(line)
    except ValueError:
        continue
    mid = msg.get("id")
    ctx = Ctx(msg.get("ctx") or {})
    test = bool(msg.get("test"))
    buf = io.StringIO() if test else None
    if test:
        sys.stdout = buf
    t0 = time.perf_counter()
    try:
        fn = FUNCS.get(msg["fn"])
        if not callable(fn):
            raise NameError("funzione '%s' non definita%s" % (msg["fn"], ("\n" + LOAD_ERR) if LOAD_ERR else ""))
        res = fn(msg.get("payload"), ctx)
        rep = {"id": mid, "ok": True, "result": res, "writes": ctx._writes, "logs": ctx._logs}
    except Exception as e:
        tb = traceback.extract_tb(sys.exc_info()[2])
        user = [f for f in tb if not f.filename.startswith("<string>")]
        rep = {"id": mid, "ok": False, "error": "%s: %s" % (type(e).__name__, e), "trace": traceback.format_exc(), "logs": ctx._logs,
               "where": [{"file": os.path.basename(f.filename), "line": f.lineno, "func": f.name} for f in user]}
    finally:
        sys.stdout = sys.stderr
    if test:
        rep["stdout"] = buf.getvalue()
        rep["duration_ms"] = (time.perf_counter() - t0) * 1000.0
        rep["writes"] = rep.get("writes", ctx._writes)
    _send(rep)
