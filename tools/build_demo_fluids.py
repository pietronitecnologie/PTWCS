#!/usr/bin/env python3
"""Demo fluidi del layer fisico: impianto di processo con i reattori batch R1/R2.

  acqua di rete (3 bar) ─V_IN─▶ T1 acqua 5 m³ ─P1─▶ collettore J1 ─R1_FV─▶ R1 (0,5 m³, quota 3 m)
                                                                 └─R2_FV─▶ R2
  R1 / R2: riscaldatore 250 kW con termostato (SP_Temp), agitatore; scarico per gravità ─R*_DV─▶ T2
  T2 prodotto 6 m³ ─P2─V_OUT─▶ carico autobotte

- Le fasi ISA-88 della demo processo (Carico, Riscaldo, Agitazione, Scarico) comandano
  valvole, riscaldatori e agitatori dei reattori: la fisica calcola livelli, temperature, portate.
- Lo script a timer `proc_logic` fa la parte del PLC di servizio (reintegro T1, pompe P1 e P2).
- Con la simulazione attiva lo script `sim_reactor` si fa da parte (tag Proc.SimActive).

Uso (server avviato, demo processo già creata con build_demo_process.py):
    python3 tools/build_demo_fluids.py [http://127.0.0.1:8080]
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


# ------------------------------------------------------------------ progetto
p = call("GET", "/api/project")["project"]
if not any(t["name"] == "R1.Level" for t in p["tags"]):
    raise SystemExit("mancano i tag dei reattori: esegui prima tools/build_demo_process.py")


def T(name, typ, desc, unit="", writable=False, **kw):
    upsert(p["tags"], {"name": name, "driver": "WCS_INTERNO", "address": "proc_" + name.lower().replace(".", "_")[5:], "type": typ,
                       "writable": writable, "description": desc, "unit": unit, "area": "Processo", **kw})


T("Proc.Auto", "bool", "Logica di servizio automatica (reintegro T1, pompe)", writable=True, initial=True)
T("Proc.SimActive", "bool", "Simulazione fisica dei fluidi attiva", writable=True)
T("Proc.V_IN", "bool", "Valvola reintegro acqua di rete", writable=True)
T("Proc.P1.Run", "bool", "Pompa P1 alimentazione reattori: marcia", writable=True)
T("Proc.P1.Fb", "bool", "Pompa P1: in marcia (retroazione)")
T("Proc.P2.Run", "bool", "Pompa P2 carico autobotte: marcia", writable=True)
T("Proc.P2.Fb", "bool", "Pompa P2: in marcia (retroazione)")
T("Proc.V_OUT", "bool", "Valvola carico autobotte", writable=True)
for tk, d in (("T1", "serbatoio acqua"), ("T2", "serbatoio prodotto")):
    T(f"Proc.{tk}.Level", "f32", f"{tk} {d}: livello", "%", historize=True,
      alarms=[{"name": "HI", "op": ">", "value": 95, "message": f"{tk}: livello molto alto", "severity": "warning"}])
    T(f"Proc.{tk}.Temp", "f32", f"{tk} {d}: temperatura", "°C", historize=True)
for f, d in (("F_IN", "reintegro da rete"), ("F_P1", "mandata P1"), ("F_OUT", "carico autobotte")):
    T(f"Proc.{f}", "f32", f"Portata {d}", "m³/h", historize=True)
upsert(p["timer_scripts"], {"name": "proc_logic", "function": "proc_logic", "interval_ms": 300, "enabled": True})
ph = p.get("physics") or {}
ac = [t.strip() for t in (ph.get("active_tag") or "").split(",") if t.strip() and t.strip() != "Proc.SimActive"] + ["Proc.SimActive"]
ph["active_tag"] = ",".join(ac)
ph.setdefault("density", 1000)
ph.setdefault("cp", 4186)
ph.setdefault("ambient_c", 20)
p["physics"] = ph
r = call("PUT", "/api/project", p)
print("progetto:", r.get("ok"), (r.get("warnings") or [])[:3])

# ------------------------------------------------------------------ vista
B = lambda tag: {"kind": "tag", "tag": tag}
C = []


def comp(id, type, x, y, w, h, props=None, bindings=None, physics=None, rotation=0, events=None):
    c = {"id": id, "name": id, "type": type, "x": x, "y": y, "w": w, "h": h, "rotation": rotation,
         "props": props or {}, "bindings": bindings or {}, "events": events or {}}
    if physics:
        c["physics"] = physics
    C.append(c)
    return c


def pipe_h(id, x1, x2, y, physics=None):
    return comp(id, "line", x1, y - 5, x2 - x1, 10, {"stroke": "#475569", "strokeWidth": 8}, physics=physics)


def pipe_v(id, x, y1, y2, physics=None):
    L = y2 - y1
    return comp(id, "line", x - L / 2, (y1 + y2) / 2 - 5, L, 10, {"stroke": "#475569", "strokeWidth": 8}, physics=physics, rotation=90)


def label(id, x, y, text, size=13, color="#94a3b8", w=None, bold=False):
    comp(id, "label", x, y, w or max(60, len(text) * size * 0.6), size + 10, {"text": text, "fontSize": size, "color": color, "bold": bold})


def value(id, x, y, tag, lab, unit="", w=130, dec=1):
    comp(id, "value", x, y, w, 30, {"label": lab, "unit": unit, "decimals": dec, "fontSize": 14}, {"value": B(tag)})


def valve(id, x, y, cmd, physics, rot=0, lab=None):
    comp(id, "symbol", x, y, 56, 45, {"symbol": "proc_valve"},
         {"elements.body.fill": {"kind": "expr", "expr": f"tag('{cmd}') ? '#22c55e' : '#64748b'"}, "elements.label.text": {"kind": "expr", "expr": f"'{lab or id}'"}},
         physics=physics, rotation=rot)


def pump(id, x, y, run, fb, physics):
    comp(id, "symbol", x, y, 66, 66, {"symbol": "proc_pump"},
         {"elements.body.fill": {"kind": "expr", "expr": f"tag('{fb}') ? '#22c55e' : tag('{run}') ? '#f59e0b' : '#64748b'"},
          "elements.impeller.spin": B(fb), "elements.label.text": {"kind": "expr", "expr": f"'{id}'"}}, physics=physics)


def tank(id, x, y, w, h, level, lab, physics):
    comp(id, "tank", x, y, w, h, {"label": lab, "fill": "#0ea5e9"}, {"level": B(level)}, physics=physics)


comp("panel", "rect", 20, 50, 1560, 760, {"fill": "#0f172a", "radius": 12, "stroke": "#1e293b"})
label("title", 30, 12, "Impianto di processo – fluidi (layer fisico)", 24, "#e2e8f0", 760, True)
label("hint", 800, 20, "Avvia un batch da Processo → Batch: le fasi comandano valvole, riscaldatori e agitatori; la fisica calcola livelli, temperature e portate", 12, "#64748b", 760)
# acqua di rete → T1
comp("W", "label", 40, 125, 120, 22, {"text": "⇥ Acqua di rete 3 bar", "fontSize": 13, "color": "#4ade80"}, physics={"kind": "supply", "pressure_bar": 3.0, "temp_c": 15})
pipe_h("L_IN", 160, 300, 160, {"kind": "pipe", "from": "W", "to": "T1", "elements": "V_IN", "flow_tag": "Proc.F_IN"})
valve("V_IN", 200, 128, "Proc.V_IN", {"kind": "valve", "cmd": "Proc.V_IN", "kv": 10, "time_s": 3})
value("vfin", 170, 180, "Proc.F_IN", "Reintegro", "m³/h", 120)
tank("T1", 300, 90, 130, 230, "Proc.T1.Level", "T1 acqua 5 m³",
     {"kind": "tank", "volume_m3": 5, "height_m": 2.5, "level_tag": "Proc.T1.Level", "temp_tag": "Proc.T1.Temp", "init_level": 60, "init_temp": 15})
value("vt1t", 300, 362, "Proc.T1.Temp", "Temperatura T1", "°C", 130)
# T1 → P1 → collettore J1
pipe_h("L_P1", 430, 640, 295, {"kind": "pipe", "from": "T1", "to": "J1", "elements": "P1", "flow_tag": "Proc.F_P1"})
pump("P1", 500, 262, "Proc.P1.Run", "Proc.P1.Fb", {"kind": "pump", "run": "Proc.P1.Run", "head_m": 40, "q_max": 12, "fb_run": "Proc.P1.Fb"})
value("vfp1", 470, 340, "Proc.F_P1", "Mandata P1", "m³/h", 140)
comp("J1", "ellipse", 640, 285, 20, 20, {"fill": "#e879f9"}, physics={"kind": "junction"})
# J1 → R1 (diretto) e → R2 (sopra R1)
pipe_h("L_R1", 660, 820, 290, {"kind": "pipe", "from": "J1", "to": "R1", "elements": "R1_FV"})
valve("R1_FV", 712, 262, "R1.FillValve", {"kind": "valve", "cmd": "R1.FillValve", "kv": 8, "time_s": 2}, lab="R1 carico")
pipe_v("L_R2", 650, 70, 285, {"kind": "pipe", "from": "J1", "to": "R2", "elements": "R2_FV"})
pipe_h("L_R2_seg1", 650, 1180, 70)
pipe_v("L_R2_seg2", 1180, 70, 140)
valve("R2_FV", 980, 42, "R2.FillValve", {"kind": "valve", "cmd": "R2.FillValve", "kv": 8, "time_s": 2}, lab="R2 carico")
for u, x in (("R1", 820), ("R2", 1120)):
    tank(u, x, 140, 120, 170, f"{u}.Level", f"Reattore {u} 0,5 m³",
         {"kind": "tank", "volume_m3": 0.5, "height_m": 1.0, "elevation_m": 3.0, "level_tag": f"{u}.Level", "temp_tag": f"{u}.Temp", "init_temp": 20, "loss_w_k": 40})
    comp(f"H_{u}", "label", x + 8, 342, 110, 22, {"text": "♨ 250 kW", "fontSize": 13, "color": "#f97316"},
         {"color": {"kind": "expr", "expr": f"tag('{u}.Heater') ? '#f97316' : '#64748b'"}},
         physics={"kind": "heater", "on": u, "cmd": f"{u}.Heater", "power_kw": 250, "setpoint_tag": f"{u}.SP_Temp", "hysteresis": 1.0})
    comp(f"M_{u}", "motor", x + 40, 96, 40, 40, {"label": ""}, {"running": {"kind": "expr", "expr": f"tag('{u}.AgitSpeed') > 1"}},
         physics={"kind": "motor", "run": f"{u}.Agitator", "speed_sp": f"{u}.SP_AgitSpeed", "sp_max": 200, "rpm": 200, "time_s": 3, "speed_fb": f"{u}.AgitSpeed"})
    value(f"v{u}t", x + 140, 150, f"{u}.Temp", "Temperatura", "°C", 130)
    value(f"v{u}sp", x + 140, 200, f"{u}.SP_Temp", "Setpoint", "°C", 130, 0)
    value(f"v{u}a", x + 140, 250, f"{u}.AgitSpeed", "Agitatore", "rpm", 130, 0)
    # scarico per gravità nel serbatoio prodotto
    xd = x + 60
    pipe_v(f"L_D{u[1]}", xd, 370, 560, {"kind": "pipe", "from": u, "to": "T2", "elements": f"{u}_DV"})
    pipe_h(f"L_D{u[1]}_seg1", min(xd, 1050), max(xd, 1050), 560)
    valve(f"{u}_DV", xd - 28, 430, f"{u}.DrainValve", {"kind": "valve", "cmd": f"{u}.DrainValve", "kv": 20, "time_s": 2}, rot=90, lab=f"{u} scarico")
tank("T2", 950, 520, 200, 220, "Proc.T2.Level", "T2 prodotto 6 m³",
     {"kind": "tank", "volume_m3": 6, "height_m": 2.0, "level_tag": "Proc.T2.Level", "temp_tag": "Proc.T2.Temp", "init_temp": 20})
value("vt2t", 790, 700, "Proc.T2.Temp", "Temperatura T2", "°C", 130)
# T2 → P2 → V_OUT → carico autobotte
pipe_h("L_OUT", 1150, 1440, 715, {"kind": "pipe", "from": "T2", "to": "D", "elements": "P2,V_OUT", "flow_tag": "Proc.F_OUT"})
pump("P2", 1200, 682, "Proc.P2.Run", "Proc.P2.Fb", {"kind": "pump", "run": "Proc.P2.Run", "head_m": 20, "q_max": 15, "fb_run": "Proc.P2.Fb"})
valve("V_OUT", 1310, 687, "Proc.V_OUT", {"kind": "valve", "cmd": "Proc.V_OUT", "kv": 15, "time_s": 3}, lab="V_OUT")
comp("D", "label", 1445, 705, 130, 22, {"text": "⇤ Carico autobotte", "fontSize": 13, "color": "#94a3b8"}, physics={"kind": "drain"})
value("vfout", 1300, 750, "Proc.F_OUT", "Carico", "m³/h", 130)
comp("auto", "button", 40, 740, 230, 40, {"text": "Logica di servizio", "fill": "#475569", "activeFill": "#15803d"},
     {"active": B("Proc.Auto"), "text": {"kind": "expr", "expr": "'Logica di servizio: ' + (tag('Proc.Auto') ? 'AUTO' : 'MANUALE')"}},
     {"onClick": {"actions": [{"kind": "write", "tag": "Proc.Auto", "value": "toggle"}]}})
comp("lsim", "lamp", 300, 749, 22, 22, {"colorOn": "#a78bfa", "label": "Simulazione fisica attiva"}, {"on": B("Proc.SimActive")})
comp("tobatch", "button", 560, 740, 200, 40, {"text": "▶ Batch e fasi", "fill": "#1d4ed8"}, {}, {"onClick": {"actions": [{"kind": "navigate", "view": "#process"}]}})
C.pop()  # la navigazione verso le pagine di sistema si fa dal menu Processo
view = {"title": "Impianto di processo", "width": 1600, "height": 820, "background": "#0b1220", "area": "Processo", "components": C}
r = call("PUT", "/api/views/processo_fisico", view)
print("vista processo_fisico:", r.get("ok"), len(C), "componenti", (r.get("warnings") or [])[:3])
st = call("GET", "/api/physics")
mine = [c for c in st["components"] if c["key"].startswith("processo_fisico/")]
print("modello fisico:", len(mine), "componenti di processo · errori:", [e for e in st["errors"] if e.startswith("processo_fisico")] or "nessuno")
