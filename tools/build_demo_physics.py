#!/usr/bin/env python3
"""Demo del layer fisico (collaudo virtuale) sulla linea di smistamento colli.

- Tag "SimIO" sul PLC di linea (plc_sim, come il DB SimIO su S7-1500/PLCSIM Advanced):
  attivo, heartbeat, fotocellule, lettura scanner, velocità reale.
- Impostazioni fisiche: driver simulabile PLC_LINEA, materiali (collo, pacco, busta).
- Vista "linea" ridisegnata con le proprietà fisiche: ingresso colli → CV1 → CV2 con
  scanner SC1 → CV3 con fotocellula → deviatore DV1 → uscite 1..3 e scarto.

Il PLC (plc_sim) comanda nastri e deviatore; la fisica muove i colli e restituisce
fotocellule e letture; WCS e regole di smistamento lavorano come con l'impianto reale.

Uso:  python3 tools/build_demo_physics.py [http://127.0.0.1:8080]
Con la sicurezza attiva servono PTWCS_USER / PTWCS_PASSWORD (progettista o admin).
"""
import json
import math
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


# ------------------------------------------------------------------ progetto: tag SimIO e fisica
p = call("GET", "/api/project")["project"]


def T(name, addr, typ, desc, **kw):
    upsert(p["tags"], {"name": name, "driver": "PLC_LINEA", "address": addr, "type": typ, "writable": True,
                       "description": desc, "area": "Servizi", **kw})


T("SimIO.Active", "coil:99", "bool", "Simulazione attiva (il PLC usa gli ingressi simulati)")
T("SimIO.Heartbeat", "hr:99", "u16", "Watchdog della simulazione")
for i, (n, d) in enumerate((("PE_SC", "fotocellula scanner"), ("PE_DV", "fotocellula prima del deviatore"),
                             ("PE_OUT1", "fotocellula fine uscita 1"), ("PE_OUT2", "fotocellula fine uscita 2"),
                             ("PE_OUT3", "fotocellula fine uscita 3"), ("PE_REJ", "fotocellula fine scarto"))):
    T(f"SimIO.{n}", f"coil:{100 + i}", "bool", f"Ingresso simulato: {d}")
T("SimIO.ScanCount", "hr:100", "u16", "Ingresso simulato: contatore letture scanner")
T("SimIO.ScanCode", "hr:101", "string", "Ingresso simulato: barcode letto", len=20)
T("SimIO.ScanWeight", "hr:111", "u16", "Ingresso simulato: peso", unit="g×10")
T("SimIO.CV1_Speed", "hr:112", "u16", "Ingresso simulato: velocità reale CV1", unit="m/min×10")
p["physics"] = {
    "step_ms": 20, "scale_mm": 10, "drivers": ["PLC_LINEA"],
    "heartbeat_tag": "SimIO.Heartbeat", "active_tag": "SimIO.Active",
    "materials": [
        {"name": "Collo", "length_m": 0.4, "width_m": 0.3, "mass_min": 1, "mass_max": 18, "color": "#c8915a", "share": 3, "description": "Cartone standard"},
        {"name": "Pacco", "length_m": 0.6, "width_m": 0.4, "mass_min": 8, "mass_max": 34, "color": "#a16207", "share": 1, "description": "Cartone grande"},
        {"name": "Busta", "length_m": 0.3, "width_m": 0.25, "mass_min": 0.1, "mass_max": 2, "color": "#e5e7eb", "share": 1, "description": "Busta imbottita"},
    ],
}
r = call("PUT", "/api/project", p)
print("progetto:", r.get("ok"), r.get("warnings") or "")

# ------------------------------------------------------------------ vista "linea" con il layer fisico
B = lambda tag: {"kind": "tag", "tag": tag}


def comp(id, type, x, y, w, h, props=None, bindings=None, events=None, physics=None, rotation=0):
    c = {"id": id, "name": id, "type": type, "x": round(x, 1), "y": round(y, 1), "w": w, "h": h, "rotation": rotation,
         "props": props or {}, "bindings": bindings or {}, "events": events or {}}
    if physics:
        c["physics"] = physics
    return c


def conv_from(id, sx, sy, length_px, angle, label, run, physics, h=40):
    """Nastro che parte da (sx, sy) con l'angolo dato: calcola il riquadro ruotato."""
    a = math.radians(angle)
    cx, cy = sx + math.cos(a) * length_px / 2, sy + math.sin(a) * length_px / 2
    return comp(id, "conveyor", cx - length_px / 2, cy - h / 2, length_px, h, {"label": label},
                {"running": B(run)}, physics=physics, rotation=angle)


toggle = lambda tag, conf=None: {"onClick": {"actions": [{"kind": "write", "tag": tag, "value": "toggle"}], **({"confirm": conf} if conf else {})}}
Y = 300  # asse della linea
C = [
    comp("title", "label", 30, 18, 700, 36, {"text": "Linea smistamento colli", "fontSize": 24, "bold": True}),
    comp("panel", "rect", 20, 70, 1240, 470, {"fill": "#0f172a", "radius": 12, "stroke": "#1e293b"}),
    comp("simhint", "label", 760, 24, 480, 24, {"text": "Layer fisico: Designer → ⚛ Fisica · runtime → Simulazione", "fontSize": 13, "color": "#64748b"}),
    # ingresso colli (sorgente) sull'inizio di CV1
    comp("SRC", "label", 50, Y - 62, 120, 22, {"text": "⊕ Ingresso colli", "fontSize": 13, "color": "#94a3b8"},
         physics={"kind": "source", "on": "CV1", "interval_min": 1.8, "interval_max": 4.0,
                  "barcodes": "IT-MI-000123,IT-RM-004512,IT-TO-778899,DE-BE-112233,FR-PA-556677,IT-NA-990011,ES-MD-445566,IT-BO-123456,IT-FI-654321,XX-HEAVY-01",
                  "materials": "Collo,Pacco,Busta"}),
    comp("CV1", "conveyor", 50, Y - 20, 300, 40, {"label": "CV1 carico"}, {"running": B("CV1.Run"), "fault": B("CV1.Fault")},
         toggle("CV1.Run", "Commutare marcia CV1?"),
         physics={"kind": "conveyor", "speed": 0.8, "accel": 0.8, "decel": 1.6, "run": "CV1.Run", "next": "CV2",
                  "speed_fb": "SimIO.CV1_Speed", "fb_scale": 600}),
    comp("CV2", "conveyor", 360, Y - 20, 300, 40, {"label": "CV2 scanner"}, {"running": B("CV2.Run"), "fault": B("CV2.Fault")},
         toggle("CV2.Run"), physics={"kind": "conveyor", "speed": 0.8, "accel": 0.8, "decel": 1.6, "run": "CV2.Run", "next": "CV3"}),
    comp("SC1", "symbol", 465, Y - 150, 90, 108, {"symbol": "scanner"},
         {"elements.beam.visible": B("SC1.PhotoEye"), "elements.led.fill": {"kind": "expr", "expr": "tag('SC1.Result') === 0 ? '#22c55e' : '#ef4444'"}},
         physics={"kind": "scanner", "on": "CV2", "at_m": 1.5, "code_tag": "SimIO.ScanCode", "count_tag": "SimIO.ScanCount",
                  "weight_tag": "SimIO.ScanWeight", "weight_scale": 100, "noread_pct": 3}),
    comp("PE_SC", "symbol", 497, Y - 13, 26, 26, {"symbol": "dev_photocell"},
         {"elements.beam.stroke": {"kind": "tag", "tag": "SimIO.PE_SC", "transform": {"kind": "map", "rules": [{"in": "true", "out": "#f59e0b"}], "default": "#22c55e"}}},
         physics={"kind": "photocell", "on": "CV2", "at_m": 1.5, "tag": "SimIO.PE_SC"}, rotation=90),
    comp("CV3", "conveyor", 670, Y - 20, 220, 40, {"label": "CV3 smistamento"}, {"running": B("CV3.Run"), "fault": B("CV3.Fault")},
         toggle("CV3.Run"), physics={"kind": "conveyor", "speed": 0.8, "accel": 0.8, "decel": 1.6, "run": "CV3.Run", "next": "DV1"}),
    comp("PE_DV", "symbol", 767, Y - 13, 26, 26, {"symbol": "dev_photocell"},
         {"elements.beam.stroke": {"kind": "tag", "tag": "SimIO.PE_DV", "transform": {"kind": "map", "rules": [{"in": "true", "out": "#f59e0b"}], "default": "#22c55e"}}},
         physics={"kind": "photocell", "on": "CV3", "at_m": 1.1, "tag": "SimIO.PE_DV"}, rotation=90),
    comp("DV1", "symbol", 880, Y - 55, 110, 110, {"symbol": "diverter"},
         {"elements.flap.fill": {"kind": "tag", "tag": "DV1.Position", "transform": {"kind": "map", "rules": [{"in": "99", "out": "#ef4444"}], "default": "#f59e0b"}},
          "elements.flap.rotate": {"kind": "tag", "tag": "DV1.Position", "transform": {"kind": "map", "default": "0",
                                                                                         "rules": [{"in": "1", "out": "-40"}, {"in": "2", "out": "0"}, {"in": "3", "out": "40"}, {"in": "99", "out": "80"}]}}},
         physics={"kind": "diverter", "position": "DV1.Position", "switch_s": 0.4,
                  "outputs": [{"value": "1", "next": "OUT1"}, {"value": "2", "next": "OUT2"}, {"value": "3", "next": "OUT3"}, {"value": "99", "next": "REJ"}]}),
]
SX = 1008
outs = [("OUT1", -40, 200, "Uscita 1", "PE_OUT1", "OUT1.Count", (1165, 92)), ("OUT2", 0, 200, "Uscita 2", "PE_OUT2", "OUT2.Count", (1165, 326)),
        ("OUT3", 40, 200, "Uscita 3", "PE_OUT3", "OUT3.Count", (1165, 470)), ("REJ", 80, 140, "Scarto", "PE_REJ", "REJECT.Count", (920, 450))]
for oid, ang, L, label, pe, cnt, (lx, ly) in outs:
    C.append(conv_from(oid, SX, Y, L, ang, label, "CV3.Run",
                       {"kind": "conveyor", "speed": 0.8, "accel": 0.8, "decel": 1.6, "run": "CV3.Run", "sink": True}, h=36))
    a = math.radians(ang)
    ex, ey = SX + math.cos(a) * (L - 12), Y + math.sin(a) * (L - 12)
    C.append(comp(pe, "symbol", ex - 12, ey - 12, 24, 24, {"symbol": "dev_photocell"},
                  {"elements.beam.stroke": {"kind": "tag", "tag": f"SimIO.{pe}", "transform": {"kind": "map", "rules": [{"in": "true", "out": "#f59e0b"}], "default": "#22c55e"}}},
                  physics={"kind": "photocell", "on": oid, "at_m": (L - 12) / 100, "tag": f"SimIO.{pe}"}, rotation=ang + 90))
    C.append(comp(f"cnt_{oid}", "value", lx, ly, 100, 32, {"label": label, "fontSize": 16, **({"color": "#fca5a5"} if oid == "REJ" else {})}, {"value": B(cnt)}))
C += [
    comp("barcode", "value", 580, Y - 150, 220, 34, {"label": "Barcode", "fontSize": 16}, {"value": B("SC1.Barcode")}),
    comp("weight", "value", 580, Y - 100, 220, 34, {"label": "Peso"}, {"value": {"kind": "expr", "expr": "(tag('SC1.Weight')/100).toFixed(2) + ' kg'"}}),
    comp("M1", "motor", 70, 380, 56, 56, {"label": "M-CV1"}, {"running": B("CV1.Run"), "fault": B("CV1.Fault")}),
    comp("speed", "bar", 150, 390, 200, 20, {"max": 60}, {"value": {"kind": "tag", "tag": "CV1.Speed", "transform": {"kind": "expr", "expr": "value/10"}}}),
    comp("speedv", "value", 150, 418, 200, 30, {"decimals": 1, "fontSize": 15, "unit": "m/min"}, {"value": {"kind": "tag", "tag": "CV1.Speed", "transform": {"kind": "expr", "expr": "value/10"}}}),
    comp("temp", "value", 150, 460, 200, 30, {"decimals": 1, "fontSize": 15, "label": "Temperatura motore", "unit": "°C"},
         {"value": {"kind": "tag", "tag": "CV1.Temp", "transform": {"kind": "expr", "expr": "value/10"}}}),
]
for i, (tag, lab, col) in enumerate((("CV1.Fault", "Guasto CV1", "#ef4444"), ("CV2.Fault", "Guasto CV2", "#ef4444"), ("CV3.Fault", "Guasto CV3", "#ef4444"),
                                     ("SC1.PhotoEye", "Collo allo scanner", "#38bdf8"), ("Line.EStop", "Emergenza", "#dc2626"), ("SimIO.Active", "Simulazione attiva", "#a78bfa"))):
    C.append(comp(f"l{i + 1}", "lamp", 400 + (i // 3) * 190, 390 + (i % 3) * 30, 22, 22, {"colorOn": col, "label": lab}, {"on": B(tag)}))
btn = lambda id, x, text, fill, ev, w=150, **kw: comp(id, "button", x, 560, w, 46, {"text": text, "fill": fill, **kw.get("props", {})}, kw.get("bindings"), ev)
C += [
    btn("start", 30, "▶ Avvia linea", "#15803d", {"onClick": {"actions": [{"kind": "script", "fn": "start_line"}]}}),
    btn("stop", 195, "■ Ferma linea", "#b91c1c", {"onClick": {"actions": [{"kind": "script", "fn": "stop_line"}], "confirm": "Fermare la linea?"}}),
    btn("reset", 360, "↺ Reset guasti", "#1d4ed8", {"onClick": {"actions": [{"kind": "script", "fn": "reset_faults"}]}}),
    btn("out3", 525, "Uscita 3 abilitata", "#475569", toggle("WCS.Out3Enabled"), 190, props={"activeFill": "#15803d"},
        bindings={"active": B("WCS.Out3Enabled"), "text": {"kind": "tag", "tag": "WCS.Out3Enabled", "transform": {"kind": "map", "rules": [{"in": "true", "out": "Uscita 3: ON"}, {"in": "false", "out": "Uscita 3: OFF"}]}}}),
    btn("maxw", 730, "Peso max", "#475569", {"onClick": {"actions": [{"kind": "write", "tag": "WCS.MaxWeight", "value": "prompt"}]}}, 190,
        bindings={"text": {"kind": "tag", "tag": "WCS.MaxWeight", "transform": {"kind": "expr", "expr": "'Peso max: ' + (value/100) + ' kg'"}}}),
    btn("stats", 935, "Statistiche", "#7c3aed", {"onClick": {"actions": [{"kind": "script", "fn": "get_stats"}]}}),
    btn("fault", 1100, "⚡ Simula guasto", "#9a3412", {"onClick": {"actions": [{"kind": "write", "tag": "Sim.InjectFault", "value": "true"}]}}),
    comp("hint", "label", 30, 620, 1220, 22, {"text": "Click sui nastri per marcia/arresto · con la simulazione fisica attiva i colli si muovono sul layout e il PLC lavora sugli ingressi simulati",
                                               "fontSize": 13, "color": "#64748b"}),
]
view = call("GET", "/api/views/linea")
view.update({"title": "Linea", "width": 1280, "height": 650, "components": C, "physics": {"scale_mm": 10}})
r = call("PUT", "/api/views/linea", view)
print("vista linea:", r.get("ok"), r.get("warnings") or "")
st = call("GET", "/api/physics")
print("modello fisico:", len(st["components"]), "componenti", "· errori:", st["errors"] or "nessuno")
