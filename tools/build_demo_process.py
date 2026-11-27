#!/usr/bin/env python3
"""Demo di processo: due reattori batch (R1, R2) con ricette e sequenze.

Crea tag interni simulati, lo script a timer `sim_reactor`, il tipo di ricetta
`Batch` (unità R1, R2) con tre ricette e le sequenze `Batch_R1` / `Batch_R2`.
Con la sicurezza attiva servono PTWCS_USER / PTWCS_PASSWORD (progettista o admin).

Uso:  python3 tools/build_demo_process.py [http://127.0.0.1:8080]
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
        raise SystemExit(f"{m} {u}: {e.code} {e.read().decode()[:600]}")


me = call("GET", "/api/auth/me")
if me["enabled"] and not me.get("all"):
    call("POST", "/api/auth/login", {"username": os.environ.get("PTWCS_USER", "admin"), "password": os.environ.get("PTWCS_PASSWORD", "")})


def upsert(lst, item, key="name"):
    for i, x in enumerate(lst):
        if x[key] == item[key]:
            lst[i] = item
            return
    lst.append(item)


p = call("GET", "/api/project")["project"]
UNITS = ("R1", "R2")
for u in UNITS:
    T = lambda n, t, wr=False, unit="", d="", **kw: upsert(p["tags"], {"name": f"{u}.{n}", "driver": "WCS_INTERNO", "address": f"{u.lower()}_{n.lower()}",
                                                                        "type": t, "writable": wr, "unit": unit, "description": f"{u} – {d}", "area": "Processo", **kw})
    T("Level", "f32", unit="%", d="livello", historize=True)
    T("Temp", "f32", unit="°C", d="temperatura", historize=True)
    T("AgitSpeed", "f32", unit="rpm", d="velocità agitatore")
    for n, d in (("FillValve", "valvola di carico"), ("DrainValve", "valvola di scarico"), ("Heater", "riscaldatore"), ("Agitator", "agitatore"), ("Start", "avvio batch da PLC")):
        T(n, "bool", True, d=d)
    T("SP_Level", "f32", True, "%", "setpoint livello", initial=70)
    T("SP_Temp", "f32", True, "°C", "setpoint temperatura", initial=60)
    T("SP_MixTime", "f32", True, "s", "tempo di mantenimento", initial=30)
    T("SP_AgitSpeed", "f32", True, "rpm", "velocità agitatore", initial=120)
    T("Product", "string", True, d="prodotto", len=20)

upsert(p["timer_scripts"], {"name": "sim_reactor", "function": "sim_reactor", "interval_ms": 500, "enabled": True})
p.setdefault("recipe_types", [])
upsert(p["recipe_types"], {
    "name": "Batch", "description": "Ricetta del reattore batch: livello di carico, temperatura, tempo di mantenimento, agitazione",
    "area": "Processo", "units": list(UNITS),
    "parameters": [
        {"name": "Prodotto", "tag": "{unit}.Product", "description": "Codice prodotto", "default": "Base"},
        {"name": "Livello", "tag": "{unit}.SP_Level", "unit": "%", "min": 10, "max": 95, "default": 70, "description": "Livello di carico"},
        {"name": "Temperatura", "tag": "{unit}.SP_Temp", "unit": "°C", "min": 25, "max": 90, "default": 60, "description": "Temperatura di processo"},
        {"name": "Mantenimento", "tag": "{unit}.SP_MixTime", "unit": "s", "min": 5, "max": 600, "default": 30, "description": "Tempo di mantenimento in temperatura"},
        {"name": "Agitazione", "tag": "{unit}.SP_AgitSpeed", "unit": "rpm", "min": 40, "max": 200, "default": 120, "description": "Velocità dell'agitatore"},
    ]})
p.setdefault("sequences", [])
for u in UNITS:
    off = [{"kind": "write", "tag": "{unit}." + n, "value": False} for n in ("FillValve", "Heater", "Agitator", "DrainValve")]
    upsert(p["sequences"], {
        "name": f"Batch_{u}", "description": f"Ciclo batch del reattore {u}", "area": "Processo", "unit": u,
        "start_tag": f"{u}.Start", "params": {"recipe": "Standard"},
        "steps": [
            {"name": "Carico ricetta", "description": "Scrive i setpoint della ricetta scelta all'avvio",
             "actions": [{"kind": "recipe", "recipe_type": "Batch", "recipe": "{recipe}"}, {"kind": "log", "text": "batch {unit}: ricetta {recipe}"}],
             "transitions": [{"next": ""}]},
            {"name": "Riempimento", "actions": [{"kind": "write", "tag": "{unit}.FillValve", "value": True}],
             "transitions": [{"when": [{"tag": "{unit}.Level", "op": ">=", "value": "tag:{unit}.SP_Level"}], "label": "livello raggiunto"}],
             "timeout_s": 180},
            {"name": "Riscaldamento", "actions": [{"kind": "write", "tag": "{unit}.FillValve", "value": False},
                                                  {"kind": "write", "tag": "{unit}.Heater", "value": True},
                                                  {"kind": "write", "tag": "{unit}.Agitator", "value": True}],
             "transitions": [{"when": [{"tag": "{unit}.Temp", "op": ">=", "value": "tag:{unit}.SP_Temp"}], "label": "temperatura raggiunta"}],
             "timeout_s": 600},
            {"name": "Mantenimento", "transitions": [{"when": [{"tag": "@elapsed", "op": ">=", "value": "tag:{unit}.SP_MixTime"}], "label": "tempo trascorso"}]},
            {"name": "Controllo qualità", "description": "Prelievo del campione: l'operatore conferma l'esito",
             "actions": [{"kind": "write", "tag": "{unit}.Heater", "value": False}],
             "transitions": [{"when": [{"tag": "@confirm", "op": "==", "value": True}], "label": "campione conforme"}]},
            {"name": "Scarico", "actions": [{"kind": "write", "tag": "{unit}.Agitator", "value": False},
                                            {"kind": "write", "tag": "{unit}.DrainValve", "value": True}],
             "transitions": [{"when": [{"tag": "{unit}.Level", "op": "<=", "value": 1}], "label": "reattore vuoto"}], "timeout_s": 180},
            {"name": "Fine", "actions": [{"kind": "write", "tag": "{unit}.DrainValve", "value": False}], "transitions": [{"next": "END"}]},
        ],
        "stop_actions": off})
print("progetto:", call("PUT", "/api/project", p).get("ok"))
for name, prod, lvl, temp, mix, agit, desc in [("Standard", "Crema", 70, 65, 40, 120, "Ricetta di produzione standard"),
                                               ("Rapida", "Base", 50, 45, 15, 160, "Lotto piccolo, ciclo breve"),
                                               ("Intensiva", "Densa", 85, 80, 90, 90, "Alta temperatura, mantenimento lungo")]:
    call("PUT", f"/api/recipes/Batch/{name}", {"description": desc, "values": {"Prodotto": prod, "Livello": lvl, "Temperatura": temp, "Mantenimento": mix, "Agitazione": agit}})
    print("ricetta", name)
