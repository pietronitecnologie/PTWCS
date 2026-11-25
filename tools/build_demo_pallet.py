"""Esempio "Magazzino pallet": 2 ingressi, 2 uscite, 3 trasloelevatori, 3000 locazioni
(3 corsie × 2 lati × 50 colonne × 10 livelli).

Crea via API (come dal Designer): aree, UDT (telegramma missioni traslo, telegramma
ingresso, baia d'uscita), istanze, tag, punti di decisione IN1/IN2 con regole grafiche
(uscita `store:MP1`), il magazzino MP1, lo script a timer di simulazione e le viste
"mag_pallet" (panoramica aggregata) e "mag_pallet_corsia" (prospetto per corsia).

Uso: python3 tools/build_demo_pallet.py [http://localhost:8080]
"""
import json
import sys
import urllib.request

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080"


def call(m, u, b=None):
    r = urllib.request.Request(BASE + u, data=json.dumps(b).encode() if b is not None else None, method=m,
                               headers={"content-type": "application/json"})
    try:
        return json.loads(urllib.request.urlopen(r).read() or b"null")
    except urllib.error.HTTPError as e:
        raise SystemExit(f"{m} {u}: {e.code} {e.read().decode()[:600]}")


def upsert(lst, item, key="name"):
    for i, x in enumerate(lst):
        if x[key] == item[key]:
            lst[i] = item
            return
    lst.append(item)


p = call("GET", "/api/project")["project"]
for k in ("areas", "udt_types", "udt_instances", "tags", "decision_points", "timer_scripts", "warehouses"):
    p.setdefault(k, [])

# ---------------------------------------------------------------- aree
for a in [{"name": "MagPallet", "description": "Magazzino automatico pallet MP1"},
          {"name": "MP-Ingressi", "description": "Ingressi IN1/IN2 e distribuzione", "parent": "MagPallet"},
          {"name": "MP-Trasli", "description": "Trasloelevatori C1..C3", "parent": "MagPallet"},
          {"name": "MP-Uscite", "description": "Raccolta e baie BU1/BU2", "parent": "MagPallet"}]:
    upsert(p["areas"], a)

# ---------------------------------------------------------------- UDT
def m(name, typ, off, w=False, role=None, **kw):
    kind = {"bool": f"DBX{{Base+{off[0]}}}.{off[1]}" if isinstance(off, tuple) else "",
            "u16": f"DBW{{Base+{off}}}", "u32": f"DBD{{Base+{off}}}", "f32": f"DBD{{Base+{off}}}", "string": f"DBB{{Base+{off}}}"}[typ]
    d = {"name": name, "type": typ, "address": "DB{DB}." + kind}
    if w:
        d["writable"] = True
    if role:
        d["role"] = role
    d.update(kw)
    return d

upsert(p["udt_types"], {
    "name": "TrasloMissioni",
    "description": "Trasloelevatore con telegramma missioni WCS↔PLC (Start/Busy/Done) – compatibile con i faceplate Traslo",
    "params": [{"name": "DB", "default": 300, "description": "DB del traslo"}, {"name": "Base", "default": 0}],
    "members": [
        m("Start", "bool", (0, 0), True, description="Avvio missione (WCS)"),
        m("Busy", "bool", (0, 1), description="Missione in esecuzione (PLC)"),
        m("Fault", "bool", (0, 2), description="Guasto", alarms=[{"name": "FAULT", "message": "Traslo {Instance} in guasto", "severity": "alarm"}]),
        m("Auto", "bool", (0, 3), description="Automatico"),
        m("Moving", "bool", (0, 4), description="In movimento"),
        m("Loaded", "bool", (0, 5), description="Pallet a bordo"),
        m("Cmd_Reset", "bool", (0, 6), True, description="Reset guasto"),
        m("Mission", "u32", 2, True, description="Id missione (WCS)"),
        m("Type", "u16", 6, True, description="Tipo: 1 deposito, 2 prelievo"),
        m("SrcSide", "u16", 8, True, description="Origine: lato"), m("SrcCol", "u16", 10, True, description="Origine: colonna (0 = P&D)"),
        m("SrcLvl", "u16", 12, True, description="Origine: livello"),
        m("DstSide", "u16", 14, True, description="Destinazione: lato"), m("DstCol", "u16", 16, True, description="Destinazione: colonna (0 = P&D)"),
        m("DstLvl", "u16", 18, True, description="Destinazione: livello"), m("DstOut", "u16", 20, True, description="Baia d'uscita"),
        m("Done", "u32", 22, description="Ultima missione completata (PLC)"),
        m("Missions", "u32", 26, description="Missioni eseguite"),
        m("PctX", "f32", 30, description="Posizione corsia %"), m("PctY", "f32", 34, description="Posizione verticale %"),
        m("Fork", "f32", 38, description="Forche (-100..100)"),
        m("PosX", "f32", 42, unit="m", description="Posizione corsia", historize=True), m("PosY", "f32", 46, unit="m", description="Posizione verticale"),
    ]})
upsert(p["udt_types"], {
    "name": "TelegrammaIngresso",
    "description": "Ingresso pallet: identificazione e controllo sagoma/peso, richiesta di stoccaggio al WCS",
    "params": [{"name": "DB", "default": 310}, {"name": "Base", "default": 0}],
    "members": [
        m("Occupied", "bool", (0, 0), description="Pallet presente"),
        m("ReqID", "u16", 2, role="request", description="Contatore richiesta (PLC)"),
        m("Barcode", "string", 4, role="input", len=20, description="Identificativo pallet"),
        m("Weight", "f32", 26, role="input", unit="kg", description="Peso"),
        m("Height", "u16", 30, role="input", unit="mm", description="Altezza"),
        m("RespID", "u16", 32, True, role="response", description="Eco ReqID (WCS)"),
        m("Destination", "u16", 34, True, role="output", description="Corsia assegnata (0 = respinto)"),
        m("Result", "u16", 36, True, role="output", description="Esito: 0 ok, 1 noread, 2 peso, 3 sagoma, 4 non stoccabile"),
        m("Rejected", "u32", 38, description="Pallet respinti"),
    ]})
upsert(p["udt_types"], {
    "name": "BaiaUscita",
    "description": "Baia d'uscita pallet",
    "params": [{"name": "DB", "default": 320}, {"name": "Base", "default": 0}],
    "members": [m("Occupied", "bool", (0, 0), description="Pallet in baia"), m("Mission", "u32", 2, description="Missione"),
                m("Count", "u32", 6, description="Pallet consegnati")]})

# ---------------------------------------------------------------- istanze e tag
for i in (1, 2, 3):
    upsert(p["udt_instances"], {"name": f"C{i}", "type": "TrasloMissioni", "driver": "WCS_INTERNO", "area": "MP-Trasli",
                                "params": {"DB": 300 + i}, "description": f"Trasloelevatore corsia {i}"})
for i in (1, 2):
    upsert(p["udt_instances"], {"name": f"IN{i}", "type": "TelegrammaIngresso", "driver": "WCS_INTERNO", "area": "MP-Ingressi",
                                "params": {"DB": 310 + i}, "description": f"Ingresso {i}"})
    upsert(p["udt_instances"], {"name": f"BU{i}", "type": "BaiaUscita", "driver": "WCS_INTERNO", "area": "MP-Uscite",
                                "params": {"DB": 320 + i}, "description": f"Baia d'uscita {i}"})
tags = [
    {"name": "MP.MaxWeight", "driver": "WCS_INTERNO", "address": "mp_maxw", "type": "f32", "writable": True, "unit": "kg", "initial": 1200, "area": "MagPallet", "description": "Peso massimo pallet"},
    {"name": "MP.MaxHeight", "driver": "WCS_INTERNO", "address": "mp_maxh", "type": "u16", "writable": True, "unit": "mm", "initial": 1800, "area": "MagPallet", "description": "Altezza massima pallet"},
    {"name": "MPDIST.Occupied", "driver": "WCS_INTERNO", "address": "mp_dist_occ", "type": "bool", "area": "MP-Ingressi"},
    {"name": "MPDIST.Count", "driver": "WCS_INTERNO", "address": "mp_dist_cnt", "type": "u16", "area": "MP-Ingressi", "description": "Pallet sulla distribuzione"},
    {"name": "MPCOLL.Occupied", "driver": "WCS_INTERNO", "address": "mp_coll_occ", "type": "bool", "area": "MP-Uscite"},
    {"name": "MPCOLL.Count", "driver": "WCS_INTERNO", "address": "mp_coll_cnt", "type": "u16", "area": "MP-Uscite", "description": "Pallet sulla raccolta"},
    {"name": "SimMP.FaultC2", "driver": "WCS_INTERNO", "address": "sim_fault_c2", "type": "bool", "writable": True, "area": "Servizi", "description": "Simula guasto traslo C2"},
]
for a in (1, 2, 3):
    tags += [{"name": f"PD{a}.In", "driver": "WCS_INTERNO", "address": f"pd{a}_in", "type": "bool", "area": "MP-Trasli", "description": f"P&D ingresso corsia {a} occupato"},
             {"name": f"PD{a}.Out", "driver": "WCS_INTERNO", "address": f"pd{a}_out", "type": "bool", "area": "MP-Trasli", "description": f"P&D uscita corsia {a} occupato"}]
for t in tags:
    upsert(p["tags"], t)

# ---------------------------------------------------------------- protocollo ingressi, magazzino, timer
for i in (1, 2):
    upsert(p["decision_points"], {"name": f"IN{i}", "description": f"Ingresso {i}: assegnazione corsia/cella", "telegram": f"IN{i}",
                                  "handshake": "counter", "timeout_ms": 300, "fallback": {"destination": 0, "result": 4}, "area": "MP-Ingressi"})
upsert(p["warehouses"], {
    "name": "MP1", "description": "Magazzino pallet 3 corsie – 3000 locazioni", "area": "MagPallet",
    "aisles": [{"name": str(i), "crane": f"C{i}"} for i in (1, 2, 3)],
    "sides": 2, "columns": 50, "levels": 10, "block_cols": 5,
    "inputs": ["IN1", "IN2"], "outputs": ["BU1", "BU2"],
    "col_time_s": 0.6, "lvl_time_s": 1.2, "lu_prefix": "PAL", "initial_fill_pct": 62, "auto_outbound_s": 16})
upsert(p["timer_scripts"], {"name": "sim_pallet", "function": "sim_pallet", "interval_ms": 200, "enabled": True})
print("progetto:", call("PUT", "/api/project", p))

# regole grafiche degli ingressi
rules = {"enabled": True, "description": "Controlli di accettazione e stoccaggio nel magazzino MP1",
         "rules": [
             {"id": "noread", "name": "Identificativo non letto", "when": [{"field": "barcode", "op": "in", "value": "NOREAD,"}],
              "then": {"outputs": {"destination": "0", "result": "1"}, "log": "pallet senza identificativo: respinto"}},
             {"id": "peso", "name": "Sovrappeso", "when": [{"field": "weight", "op": "gt", "value": "tag:MP.MaxWeight"}],
              "then": {"outputs": {"destination": "0", "result": "2"}, "log": "{barcode}: sovrappeso {weight} kg"}},
             {"id": "sagoma", "name": "Fuori sagoma", "when": [{"field": "height", "op": "gt", "value": "tag:MP.MaxHeight"}],
              "then": {"outputs": {"destination": "0", "result": "3"}, "log": "{barcode}: fuori sagoma {height} mm"}},
             {"id": "stock", "name": "Stoccaggio in MP1", "when": [{"field": "barcode", "op": "not_empty"}],
              "then": {"outputs": {"destination": "store:MP1", "result": "0"}}}],
         "default": {"outputs": {"destination": "0", "result": "4"}, "log": "{barcode}: non stoccabile (doppio o magazzino pieno)"}}
import time
time.sleep(1.5)
for i in (1, 2):
    print(f"regole IN{i}:", call("PUT", f"/api/rules/IN{i}", rules))

# ---------------------------------------------------------------- viste
lib = call("GET", "/api/library")["symbols"]


def C(comps, cid, ctype, x, y, w, h, props=None, bindings=None, events=None, custom=None, rot=0):
    comps.append({"id": cid, "type": ctype, "name": cid, "x": x, "y": y, "w": w, "h": h, "rotation": rot,
                  "props": props or {}, "bindings": bindings or {}, "events": events or {}, **({"custom": custom} if custom else {})})


def S(comps, cid, sym, x, y, w, h, device, rot=0, extra=None):
    meta = lib[sym]
    b = json.loads(json.dumps(meta["bindings"]))
    b.update(extra or {})
    C(comps, cid, "symbol", x, y, w, h, {"symbol": sym}, b,
      {"onClick": {"actions": [{"kind": "popup", "view": meta["faceplate"], "params": {"device": "{device}"}}]}}, {"device": device}, rot)


T = lambda tag, **tr: {"kind": "tag", "tag": tag, **({"transform": tr} if tr else {})}
X = lambda e: {"kind": "expr", "expr": e}
goto = lambda a: {"onClick": {"actions": [{"kind": "navigate", "view": "mag_pallet_corsia", "params": {"aisle": str(a)}}]}}

# --- panoramica
v = []
C(v, "title", "label", 24, 12, 760, 34, {"text": "Magazzino pallet MP1 – 3 trasloelevatori · 3000 locazioni", "fontSize": 22, "bold": True})
C(v, "hint", "label", 24, 44, 900, 20, {"text": "Le scaffalature sono aggregate per blocchi di 5 colonne (saturazione media su 10 livelli). Click su una corsia per il prospetto completo delle celle.", "fontSize": 12, "color": "#64748b"})
# fronte: ingressi, distribuzione, raccolta, uscite
for i, y in ((1, 110), (2, 250)):
    C(v, f"in{i}_lbl", "label", 20, y - 22, 150, 18, {"text": f"INGRESSO {i}", "fontSize": 12, "bold": True, "color": "#93c5fd"})
    S(v, f"in{i}", "conv_roller", 20, y, 150, 44, f"IN{i}", extra={
        "elements.body.fill": X(f"tag('IN{i}.Result') > 0 && tag('IN{i}.Occupied') ? '#ef4444' : tag('IN{i}.Occupied') ? '#22c55e' : '#475569'"),
        "elements.load.visible": T(f"IN{i}.Occupied"), "elements.flow.visible": T(f"IN{i}.Occupied"), "elements.flow.flow": T(f"IN{i}.Occupied"),
        "elements.label.text": X("''")})
    v[-1]["events"] = {}
    C(v, f"in{i}_bc", "value", 20, y + 52, 150, 24, {"fontSize": 12, "decimals": 0}, {"value": X(f"tag('IN{i}.Occupied') ? tag('IN{i}.Barcode') : '—'")})
    C(v, f"in{i}_res", "label", 20, y + 80, 160, 18, {"fontSize": 11}, {
        "text": X(f"!tag('IN{i}.Occupied') ? '' : ['in attesa / corsia ' + tag('IN{i}.Destination'), 'NOREAD → respinto', 'sovrappeso → respinto', 'fuori sagoma → respinto', 'non stoccabile'][tag('IN{i}.RespID') === tag('IN{i}.ReqID') ? tag('IN{i}.Result') : 0] || ''"),
        "color": X(f"tag('IN{i}.Result') > 0 && tag('IN{i}.RespID') === tag('IN{i}.ReqID') ? '#fca5a5' : '#94a3b8'")})
for i, y in ((1, 560), (2, 700)):
    C(v, f"out{i}_lbl", "label", 20, y - 22, 150, 18, {"text": f"USCITA {i} (BU{i})", "fontSize": 12, "bold": True, "color": "#86efac"})
    S(v, f"out{i}", "conv_chain", 20, y, 150, 90, f"BU{i}", extra={
        "elements.load.visible": T(f"BU{i}.Occupied"), "elements.body.fill": X(f"tag('BU{i}.Occupied') ? '#22c55e' : '#475569'"),
        "elements.flow.visible": X("false"), "elements.label.text": X("''")})
    v[-1]["events"] = {}
    C(v, f"out{i}_cnt", "value", 20, y + 96, 150, 24, {"fontSize": 12, "unit": "pallet"}, {"value": T(f"BU{i}.Count")})
C(v, "dist", "line", 175, 330, 300, 10, {"stroke": "#475569", "strokeWidth": 14}, {"flow": T("MPDIST.Occupied")}, rot=90)
C(v, "dist_lbl", "label", 190, 470, 130, 18, {"fontSize": 11, "color": "#64748b"}, {"text": X("'distribuzione: ' + (tag('MPDIST.Count') || 0)")})
C(v, "coll", "line", 140, 560, 300, 10, {"stroke": "#475569", "strokeWidth": 14}, {"flow": T("MPCOLL.Occupied")}, rot=90)
C(v, "coll_lbl", "label", 300, 700, 120, 18, {"fontSize": 11, "color": "#64748b"}, {"text": X("'raccolta: ' + (tag('MPCOLL.Count') || 0)")})
# corsie
for a in (1, 2, 3):
    y0 = 92 + (a - 1) * 188
    C(v, f"a{a}_lbl", "label", 400, y0 - 2, 260, 20, {"text": f"CORSIA {a} – traslo C{a}", "fontSize": 13, "bold": True}, events=goto(a))
    C(v, f"a{a}_s1", "whmap", 400, y0 + 22, 900, 46, {"wh": "MP1", "aisle": a, "side": 1, "mode": "plan", "title": "lato 1"}, events=goto(a))
    S(v, f"a{a}_lane", "asrs_lane", 400, y0 + 72, 900, 32, f"C{a}")
    C(v, f"a{a}_s2", "whmap", 400, y0 + 108, 900, 46, {"wh": "MP1", "aisle": a, "side": 2, "mode": "plan", "title": "lato 2"}, events=goto(a))
    C(v, f"a{a}_pdi", "lamp", 372, y0 + 72, 14, 14, {"colorOn": "#38bdf8"}, {"on": T(f"PD{a}.In")})
    C(v, f"a{a}_pdo", "lamp", 372, y0 + 90, 14, 14, {"colorOn": "#22c55e"}, {"on": T(f"PD{a}.Out")})
    C(v, f"a{a}_st", "label", 1312, y0 + 78, 270, 20, {"fontSize": 13, "bold": True}, {
        "text": X(f"tag('C{a}.Fault') ? 'C{a}: GUASTO' : tag('C{a}.Busy') ? ('C{a}: missione ' + tag('C{a}.Mission') + (tag('C{a}.Type') === 1 ? ' – deposito' : ' – prelievo')) : 'C{a}: libero'"),
        "color": X(f"tag('C{a}.Fault') ? '#ef4444' : tag('C{a}.Busy') ? '#22c55e' : '#94a3b8'")})
    C(v, f"a{a}_det", "whsummary", 1312, y0 + 22, 270, 50, {"wh": "MP1", "aisle": a, "fontSize": 12})
C(v, "pd_lbl", "label", 345, 650, 120, 14, {"text": "● P&D in / out", "fontSize": 10, "color": "#64748b"})
C(v, "kpi", "whsummary", 1312, 662, 270, 220, {"wh": "MP1", "fontSize": 13})
C(v, "missions", "whmissions", 400, 662, 900, 220, {"wh": "MP1", "rows": 9, "fontSize": 12})
C(v, "legend", "label", 400, 640, 140, 16, {"text": "Saturazione blocchi:", "fontSize": 11, "color": "#64748b"})
for k, (pct, col) in enumerate([(0, "rgb(30,41,59)"), (35, "rgb(30,64,175)"), (60, "rgb(59,130,246)"), (80, "rgb(20,184,166)"), (92, "rgb(245,158,11)"), (100, "rgb(239,68,68)")]):
    C(v, f"lgb{k}", "rect", 530 + k * 70, 641, 14, 14, {"fill": col, "strokeWidth": 0, "radius": 2})
    C(v, f"lgt{k}", "label", 548 + k * 70, 640, 50, 16, {"text": f"{pct}%", "fontSize": 11, "color": "#94a3b8"})
for k, (txt, ev, col) in enumerate([("🔍 Cerca UdC", {"kind": "wh", "wh": "MP1", "op": "search", "view": "mag_pallet_corsia"}, "#1d4ed8"),
                                     ("⬆ Prelievo UdC", {"kind": "wh", "wh": "MP1", "op": "outbound"}, "#15803d"),
                                     ("⚡ Simula guasto C2", {"kind": "write", "tag": "SimMP.FaultC2", "value": "true"}, "#9a3412")]):
    C(v, f"btn{k}", "button", 1000 + k * 190, 10, 180, 40, {"text": txt, "fill": col, "fontSize": 13}, None, {"onClick": {"actions": [ev]}})
view = {"title": "Magazzino pallet", "width": 1600, "height": 900, "background": "#0b1220", "area": "MagPallet", "params": {}, "components": v}
print("vista mag_pallet:", call("PUT", "/api/views/mag_pallet", view))

# --- dettaglio corsia (parametro aisle)
d = []
C(d, "title", "label", 24, 12, 700, 34, {"text": "", "fontSize": 22, "bold": True}, {"text": X("'Corsia ' + param('aisle') + ' – traslo C' + param('aisle')")})
C(d, "back", "button", 1440, 12, 140, 36, {"text": "← Panoramica", "fill": "#334155"}, None, {"onClick": {"actions": [{"kind": "navigate", "view": "mag_pallet"}]}})
for k, (a, txt) in enumerate([(1, "Corsia 1"), (2, "Corsia 2"), (3, "Corsia 3")]):
    C(d, f"go{a}", "button", 1000 + k * 140, 12, 130, 36, {"text": txt, "fill": "#1e293b", "activeFill": "#1d4ed8"}, {"active": X(f"param('aisle') == '{a}'")},
      {"onClick": {"actions": [{"kind": "navigate", "view": "mag_pallet_corsia", "params": {"aisle": str(a)}}]}})
C(d, "hint", "label", 24, 46, 1000, 18, {"text": "Prospetto completo: 50 colonne × 10 livelli per lato. Click su una cella per dettaglio, prelievo o blocco.", "fontSize": 12, "color": "#64748b"})
W, H = 1150, 300
for side, y in ((1, 80), (2, 470)):
    C(d, f"s{side}", "whmap", 24, y, W, H, {"wh": "MP1", "aisle": "{aisle}", "side": side, "mode": "cells", "labels": True, "highlight": "{hl}", "title": f"Lato {side}"})
    # traslo (vista frontale) sovrapposto al prospetto: posizione da PctX/PctY del traslo della corsia
    k = (H - 34) / 200.0
    cw = 70 * k
    S(d, f"crane{side}", "asrs_crane_mast", 24 + 26, y + 18, round(cw), H - 34, "C{aisle}", extra={
        "elements.mast.tx": X(f"(tag('{{device}}.PctX') || 0) / 100 * {round((W - 26 - cw) / k, 1)}"),
        "elements.carriage.ty": X(f"-(tag('{{device}}.PctY') || 0) / 100 * {round((H - 70) / k, 1)}"),
        "elements.label.text": X("''")})
    d[-1]["props"]["opacity"] = 0.85
for k, (txt, col) in enumerate([("libera", "#1e293b"), ("occupata", "#b7793e"), ("prenotata (deposito in corso)", "#3b82f6"), ("in prelievo", "#22c55e"), ("bloccata", "#7f1d1d")]):
    x = 24 + [0, 100, 210, 450, 580][k]
    C(d, f"lg{k}", "rect", x, 412, 14, 14, {"fill": col, "stroke": "#475569", "strokeWidth": 1, "radius": 2})
    C(d, f"lgt{k}", "label", x + 20, 410, 230, 18, {"text": txt, "fontSize": 12, "color": "#94a3b8"})
C(d, "sum", "whsummary", 1200, 80, 380, 250, {"wh": "MP1", "aisle": "{aisle}", "fontSize": 13})
C(d, "crane_lbl", "label", 1200, 345, 380, 20, {"text": "Traslo", "fontSize": 13, "bold": True})
for k, (lbl, expr) in enumerate([("Stato", "tag('C{aisle}.Fault') ? 'GUASTO' : tag('C{aisle}.Busy') ? 'in missione' : 'libero'"),
                                  ("Missione", "tag('C{aisle}.Busy') ? tag('C{aisle}.Mission') : '—'"),
                                  ("Posizione X / Y", "(tag('C{aisle}.PosX') || 0).toFixed(1) + ' m / ' + (tag('C{aisle}.PosY') || 0).toFixed(1) + ' m'"),
                                  ("Missioni eseguite", "tag('C{aisle}.Missions') || 0")]):
    C(d, f"cv{k}", "value", 1200 + (k % 2) * 195, 385 + (k // 2) * 56, 185, 30, {"label": lbl, "fontSize": 14}, {"value": X(expr)})
C(d, "fp", "button", 1200, 500, 185, 36, {"text": "Faceplate traslo", "fill": "#1d4ed8"}, None,
  {"onClick": {"actions": [{"kind": "popup", "view": "fp_asrs_crane", "params": {"device": "C{aisle}"}}]}})
C(d, "rst", "button", 1395, 500, 185, 36, {"text": "Reset guasto", "fill": "#b45309"}, None,
  {"onClick": {"actions": [{"kind": "write", "tag": "C{aisle}.Cmd_Reset", "value": "true"}]}})
C(d, "find", "button", 1200, 545, 380, 36, {"text": "🔍 Cerca UdC", "fill": "#334155"}, None,
  {"onClick": {"actions": [{"kind": "wh", "wh": "MP1", "op": "search", "view": "mag_pallet_corsia"}]}})
C(d, "trend", "trend", 1200, 600, 380, 170, {"tags": "C{aisle}.PosX", "minutes": 5})
view = {"title": "Magazzino pallet – corsia", "width": 1600, "height": 880, "background": "#0b1220", "area": "MagPallet", "menu": False,
        "params": {"aisle": "1", "hl": ""}, "components": d}
print("vista mag_pallet_corsia:", call("PUT", "/api/views/mag_pallet_corsia", view))
