#!/usr/bin/env python3
"""Demo di processo ISA-88: due reattori batch (R1, R2).

- Dispositivi R1, R2 (classe Reattore) con tag interni simulati (script a timer `sim_reactor`).
- Sequenze (fasi) con parametri: Carico, Riscaldo, Agitazione, Scarico.
- Ricette (procedure) che schedulano le fasi: "Crema" (riscaldo e agitazione in
  parallelo, controllo qualità con conferma) e "Base".
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
    for n, d in (("FillValve", "valvola di carico"), ("DrainValve", "valvola di scarico"), ("Heater", "riscaldatore"), ("Agitator", "agitatore")):
        T(n, "bool", True, d=d)
    T("SP_Temp", "f32", True, "°C", "setpoint temperatura", initial=60)
    T("SP_AgitSpeed", "f32", True, "rpm", "velocità agitatore", initial=120)
p["tags"] = [t for t in p["tags"] if not any(t["name"] == f"{u}.{n}" for u in UNITS for n in ("SP_Level", "SP_MixTime", "Product", "Start"))]
p.pop("recipe_types", None)
upsert(p["timer_scripts"], {"name": "sim_reactor", "function": "sim_reactor", "interval_ms": 500, "enabled": True})
p["devices"] = [{"name": u, "class": "Reattore", "area": "Processo", "description": f"Reattore batch {u}"} for u in UNITS]
W = lambda tag, v: {"kind": "write", "tag": "{unit}." + tag, "value": v}
p["sequences"] = [
    {"name": "Carico", "description": "Carica il reattore fino al livello richiesto", "area": "Processo", "device_class": "Reattore", "unit": "R1",
     "parameters": [{"name": "Livello", "unit": "%", "min": 10, "max": 95, "default": 60, "description": "Livello di carico"}],
     "steps": [
         {"name": "Apri carico", "actions": [W("FillValve", True)],
          "transitions": [{"when": [{"tag": "{unit}.Level", "op": ">=", "value": "{Livello}"}], "label": "livello raggiunto"}], "timeout_s": 180},
         {"name": "Chiudi carico", "actions": [W("FillValve", False)]}],
     "hold_actions": [W("FillValve", False)], "restart_actions": [W("FillValve", True)], "stop_actions": [W("FillValve", False)]},
    {"name": "Riscaldo", "description": "Porta il reattore in temperatura e la mantiene per il tempo richiesto", "area": "Processo", "device_class": "Reattore", "unit": "R1",
     "parameters": [{"name": "Temperatura", "unit": "°C", "min": 25, "max": 90, "default": 60},
                    {"name": "Mantenimento", "unit": "s", "min": 0, "max": 900, "default": 30}],
     "steps": [
         {"name": "Riscalda", "actions": [{"kind": "write", "tag": "{unit}.SP_Temp", "value": "{Temperatura}"}, W("Heater", True)],
          "transitions": [{"when": [{"tag": "{unit}.Temp", "op": ">=", "value": "{Temperatura}"}], "label": "temperatura raggiunta"}], "timeout_s": 600},
         {"name": "Mantieni", "transitions": [{"when": [{"tag": "@elapsed", "op": ">=", "value": "{Mantenimento}"}], "label": "tempo trascorso"}]},
         {"name": "Spegni", "actions": [W("Heater", False)]}],
     "hold_actions": [W("Heater", False)], "restart_actions": [W("Heater", True)], "stop_actions": [W("Heater", False)]},
    {"name": "Agitazione", "description": "Agita alla velocità richiesta per il tempo richiesto", "area": "Processo", "device_class": "Reattore", "unit": "R1",
     "parameters": [{"name": "Velocita", "unit": "rpm", "min": 40, "max": 200, "default": 120}, {"name": "Tempo", "unit": "s", "min": 5, "max": 1800, "default": 60}],
     "steps": [
         {"name": "Avvia agitatore", "actions": [{"kind": "write", "tag": "{unit}.SP_AgitSpeed", "value": "{Velocita}"}, W("Agitator", True)],
          "transitions": [{"when": [{"tag": "@elapsed", "op": ">=", "value": "{Tempo}"}]}]},
         {"name": "Ferma agitatore", "actions": [W("Agitator", False)]}],
     "hold_actions": [W("Agitator", False)], "restart_actions": [W("Agitator", True)], "stop_actions": [W("Agitator", False)]},
    {"name": "Scarico", "description": "Svuota il reattore", "area": "Processo", "device_class": "Reattore", "unit": "R1",
     "steps": [
         {"name": "Apri scarico", "actions": [W("DrainValve", True)],
          "transitions": [{"when": [{"tag": "{unit}.Level", "op": "<=", "value": 1}], "label": "reattore vuoto"}], "timeout_s": 180},
         {"name": "Chiudi scarico", "actions": [W("DrainValve", False)]}],
     "hold_actions": [W("DrainValve", False)], "restart_actions": [W("DrainValve", True)], "stop_actions": [W("DrainValve", False)]},
]
print("progetto:", call("PUT", "/api/project", p).get("ok"))

PH = lambda seq, **params: {"kind": "phase", "sequence": seq, "device": "{reattore}", **({"params": params} if params else {})}
recipes = [
    {"name": "Crema", "description": "Carico, riscaldo e agitazione in parallelo, controllo qualità, scarico",
     "devices": [{"alias": "reattore", "class": "Reattore", "default": "R1"}],
     "parameters": [{"name": "Livello", "unit": "%", "min": 30, "max": 90, "default": 70},
                    {"name": "Temperatura", "unit": "°C", "min": 40, "max": 85, "default": 65},
                    {"name": "Mantenimento", "unit": "s", "min": 10, "max": 300, "default": 40},
                    {"name": "Velocita", "unit": "rpm", "min": 60, "max": 180, "default": 120}],
     "steps": [
         {"name": "Carico", "actions": [PH("Carico", Livello="{Livello}")]},
         {"name": "Trattamento", "description": "Riscaldo e agitazione in parallelo",
          "branches": [[{"name": "Calore", "actions": [PH("Riscaldo", Temperatura="{Temperatura}", Mantenimento="{Mantenimento}")]}],
                       [{"name": "Miscelazione", "actions": [PH("Agitazione", Velocita="{Velocita}", Tempo="60")]}]]},
         {"name": "Controllo qualità", "description": "Prelievo del campione: l'operatore conferma",
          "transitions": [{"when": [{"tag": "@confirm", "op": "==", "value": True}], "label": "campione conforme"}]},
         {"name": "Scarico", "actions": [PH("Scarico")]}]},
    {"name": "Base", "description": "Carico, riscaldo breve e scarico",
     "devices": [{"alias": "reattore", "class": "Reattore", "default": "R2"}],
     "parameters": [{"name": "Livello", "unit": "%", "min": 20, "max": 80, "default": 50},
                    {"name": "Temperatura", "unit": "°C", "min": 30, "max": 60, "default": 45}],
     "steps": [
         {"name": "Carico", "actions": [PH("Carico", Livello="{Livello}")]},
         {"name": "Riscaldo", "actions": [PH("Riscaldo", Temperatura="{Temperatura}", Mantenimento="15")]},
         {"name": "Scarico", "actions": [PH("Scarico")]}]},
]
for r in recipes:
    print("ricetta", r["name"], "v", call("PUT", f"/api/recipes/{r['name']}", r)["version"])
