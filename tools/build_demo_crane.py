#!/usr/bin/env python3
"""Trasloelevatori con assi nel layer fisico: magazzino pallet MP1 (3 corsie).

- UDT "AssiTraslo" e istanze AX1..AX3: quote obiettivo X/Y/Z (dal PLC di corsia), posizioni
  reali, in posizione, carico sul carro, guasto, reset (dalla fisica).
- Vista "mag_pallet_fisico": le tre corsie in elevazione (50 colonne × 10 livelli × 2 lati,
  passo 1240 × 1778 mm), con gli assi del traslo, il carico e le forche.
- Con la simulazione fisica attiva (tag MP.SimActive) lo script sim_pallet fa il PLC di corsia:
  esegue le missioni del WCS comandando gli assi (forche dentro → X/Y alla cella → forche fuori →
  sollevamento o abbassamento → forche dentro) e verifica il carico con il sensore del carro.

Uso (server avviato, demo MP1 già creata con build_demo_pallet.py):
    python3 tools/build_demo_crane.py [http://127.0.0.1:8080]
Con la sicurezza attiva servono PTWCS_USER / PTWCS_PASSWORD (progettista o admin).
"""
import json
import os
import sys
import urllib.error
import urllib.request

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080"
COOKIE = {}


def call(m, u, b=None):
    r = urllib.request.Request(BASE + u, data=json.dumps(b).encode() if b is not None else None, method=m,
                               headers={"content-type": "application/json", **COOKIE})
    try:
        with urllib.request.urlopen(r) as resp:
            if resp.headers.get("Set-Cookie"):
                COOKIE["Cookie"] = resp.headers["Set-Cookie"].split(";")[0]
            return json.loads(resp.read() or b"null")
    except urllib.error.HTTPError as e:
        raise SystemExit(f"{m} {u}: {e.code} {e.read().decode()[:800]}")
    except urllib.error.URLError as e:
        raise SystemExit(f"server PTWCS non raggiungibile su {BASE} ({e.reason}): avvialo con  cargo run --release -- config/plant.yaml")


me = call("GET", "/api/auth/me")
if me["enabled"] and not me.get("all"):
    call("POST", "/api/auth/login", {"username": os.environ.get("PTWCS_USER", "admin"), "password": os.environ.get("PTWCS_PASSWORD", "")})


def upsert(lst, item, key="name"):
    for i, x in enumerate(lst):
        if x[key] == item[key]:
            lst[i] = item
            return
    lst.append(item)


COLS, LEVELS, PX, PY, Y0 = 50, 10, 1240, 1778, 100
p = call("GET", "/api/project")["project"]
if not any(w["name"] == "MP1" for w in p.get("warehouses", [])):
    raise SystemExit("manca il magazzino MP1: esegui prima tools/build_demo_pallet.py")
M = lambda name, typ, desc, w=False, **kw: {"name": name, "type": typ, "address": "ax_{Instance}_" + name.lower(), "writable": w, "description": desc, **kw}
upsert(p["udt_types"], {"name": "AssiTraslo", "description": "Assi del trasloelevatore (layer fisico): quote obiettivo dal PLC, posizioni reali dalla fisica",
                        "params": [], "members": [
                            M("TgtX", "f32", "Quota obiettivo traslazione X (mm)", True, unit="mm"), M("TgtY", "f32", "Quota obiettivo sollevamento Y (mm)", True, unit="mm"),
                            M("TgtZ", "f32", "Quota obiettivo forche Z (mm, − lato 1, + lato 2)", True, unit="mm"),
                            M("ActX", "f32", "Posizione reale X", unit="mm"), M("ActY", "f32", "Posizione reale Y", unit="mm"), M("ActZ", "f32", "Posizione reale forche", unit="mm"),
                            M("InPosX", "bool", "X in posizione"), M("InPosY", "bool", "Y in posizione"), M("InPosZ", "bool", "Forche in posizione"),
                            M("Load", "bool", "Carico sul carro (fotocellula)"), M("Moving", "bool", "Assi in movimento"),
                            M("Fault", "bool", "Guasto rilevato (interblocco, azionamento)", alarms=[{"name": "FAULT", "op": "==", "value": True, "message": "{Instance}: guasto assi del traslo", "severity": "alarm"}]),
                            M("Reset", "bool", "Reset guasti (fronte)", True)]})
p["udt_instances"] = [i for i in p["udt_instances"] if i.get("type") != "AssiTraslo"]
for a in (1, 2, 3):
    p["udt_instances"].append({"name": f"AX{a}", "type": "AssiTraslo", "driver": "WCS_INTERNO", "area": "MP-Trasli", "params": {}, "description": f"Assi del traslo C{a}"})
upsert(p["tags"], {"name": "MP.SimActive", "driver": "WCS_INTERNO", "address": "mp_simactive", "type": "bool", "writable": True,
                   "description": "Layer fisico attivo: i trasli di MP1 si muovono con gli assi simulati", "area": "MP-Trasli"})
ph = p.get("physics") or {}
ac = [t.strip() for t in (ph.get("active_tag") or "").split(",") if t.strip() and t.strip() != "MP.SimActive"] + ["MP.SimActive"]
ph["active_tag"] = ",".join(ac)
p["physics"] = ph
r = call("PUT", "/api/project", p)
print("progetto:", r.get("ok"), (r.get("warnings") or [])[:3])

B = lambda tag: {"kind": "tag", "tag": tag}
C = []


def comp(id, type, x, y, w, h, props=None, bindings=None, physics=None):
    c = {"id": id, "name": id, "type": type, "x": x, "y": y, "w": w, "h": h, "rotation": 0, "props": props or {}, "bindings": bindings or {}, "events": {}}
    if physics:
        c["physics"] = physics
    C.append(c)


comp("title", "label", 30, 12, 900, 34, {"text": "Magazzino pallet MP1 – trasli con assi (layer fisico)", "fontSize": 24, "color": "#e2e8f0", "bold": True})
comp("hint", "label", 30, 48, 1500, 22, {"text": "Elevazione delle tre corsie: celle (metà sinistra = lato 1, destra = lato 2), P&D alla colonna 0. Le missioni arrivano dal WCS; il PLC di corsia (sim_pallet) comanda gli assi, la fisica li muove.", "fontSize": 13, "color": "#64748b"})
for a in (1, 2, 3):
    y = 90 + (a - 1) * 260
    comp(f"p{a}", "rect", 20, y, 1560, 250, {"fill": "#0f172a", "radius": 10, "stroke": "#1e293b"})
    comp(f"t{a}", "label", 34, y + 8, 300, 22, {"text": f"Corsia {a} – traslo C{a}" + (" (doppia UdC)" if a == 3 else ""), "fontSize": 15, "color": "#cbd5e1", "bold": True})
    for i, (tag, lab, unit) in enumerate(((f"AX{a}.ActX", "X", "mm"), (f"AX{a}.ActY", "Y", "mm"), (f"AX{a}.ActZ", "Forche", "mm"))):
        comp(f"v{a}{i}", "value", 380 + i * 170, y + 4, 160, 28, {"label": lab, "unit": unit, "decimals": 0, "fontSize": 13}, {"value": B(tag)})
    comp(f"m{a}", "value", 900, y + 4, 200, 28, {"label": "Missione", "fontSize": 13}, {"value": B(f"C{a}.Mission")})
    comp(f"l{a}", "lamp", 1130, y + 10, 18, 18, {"colorOn": "#f59e0b", "label": "Carico"}, {"on": B(f"AX{a}.Load")})
    comp(f"f{a}", "lamp", 1230, y + 10, 18, 18, {"colorOn": "#ef4444", "label": "Guasto"}, {"on": {"kind": "expr", "expr": f"tag('C{a}.Fault') || tag('AX{a}.Fault')"}})
    comp(f"C{a}", "rect", 40, y + 40, 1520, 200, {"fill": "#0b1220", "stroke": "#1e293b", "radius": 4}, physics={
        "kind": "crane", "cols": COLS, "levels": LEVELS, "pitch_x_mm": PX, "pitch_y_mm": PY, "x0_mm": 0, "y0_mm": Y0,
        "x_max_mm": COLS * PX + 600, "y_max_mm": Y0 + (LEVELS - 1) * PY + 1500, "z_reach_mm": 1300, "lift_mm": 100,
        "x_vmax": 4.0, "x_acc": 0.6, "y_vmax": 1.5, "y_acc": 0.8, "z_vmax": 0.8, "z_acc": 1.5, "capacity": 2 if a == 3 else 1,
        "x_target": f"AX{a}.TgtX", "y_target": f"AX{a}.TgtY", "z_target": f"AX{a}.TgtZ",
        "x_pos": f"AX{a}.ActX", "y_pos": f"AX{a}.ActY", "z_pos": f"AX{a}.ActZ",
        "x_inpos": f"AX{a}.InPosX", "y_inpos": f"AX{a}.InPosY", "z_inpos": f"AX{a}.InPosZ",
        "load_tag": f"AX{a}.Load", "fault_tag": f"AX{a}.Fault", "moving_tag": f"AX{a}.Moving", "reset_tag": f"AX{a}.Reset"})
view = {"title": "Magazzino pallet – fisica", "width": 1600, "height": 880, "background": "#0b1220", "area": "MP-Trasli", "components": C,
        "physics": {"scale_mm": 10}}
r = call("PUT", "/api/views/mag_pallet_fisico", view)
print("vista mag_pallet_fisico:", r.get("ok"), len(C), "componenti", (r.get("warnings") or [])[:3])
st = call("GET", "/api/physics")
print("modello fisico:", sum(1 for c in st["components"] if c["kind"] == "crane"), "trasli · errori:", [e for e in st["errors"] if "mag_pallet_fisico" in e] or "nessuno")
