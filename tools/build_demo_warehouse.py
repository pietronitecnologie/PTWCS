"""Crea la demo "Magazzino" usando la libreria (come farebbe il Designer):
aree, UDT di libreria, istanze sul driver interno, script a timer di simulazione
e la vista con i simboli collegati via parametro `device` + faceplate al click.
Uso: python3 tools/build_demo_warehouse.py [http://localhost:8080]"""
import json, sys, urllib.request

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:8080"

def call(m, u, b=None):
    r = urllib.request.Request(BASE + u, data=json.dumps(b).encode() if b is not None else None, method=m,
                               headers={"content-type": "application/json"})
    try:
        return json.loads(urllib.request.urlopen(r).read() or b"null")
    except urllib.error.HTTPError as e:
        raise SystemExit(f"{m} {u}: {e.code} {e.read().decode()[:400]}")

lib = call("GET", "/api/library")
p = call("GET", "/api/project")["project"]
for a in [{"name": "Magazzino", "description": "Magazzino automatico e movimentazione"},
          {"name": "Processo", "description": "Impianto di servizio (serbatoio acqua)"}]:
    if not any(x["name"] == a["name"] for x in p["areas"]):
        p["areas"].append(a)
need = ["Trasporto", "Elevatore", "Porta", "Traslo", "Scaffale", "Veicolo", "Serbatoio", "Motore", "Valvola", "Strumento"]
for u in lib["udts"]:
    if u["name"] in need and not any(x["name"] == u["name"] for x in p["udt_types"]):
        p["udt_types"].append(u)
inst = [("CVM1", "Trasporto", 200, "Magazzino"), ("CVM2", "Trasporto", 201, "Magazzino"), ("LIFT1", "Elevatore", 202, "Magazzino"),
        ("DOOR1", "Porta", 203, "Magazzino"), ("DOCK1", "Porta", 204, "Magazzino"), ("CRANE1", "Traslo", 205, "Magazzino"),
        ("RACK1", "Scaffale", 206, "Magazzino"), ("AGV1", "Veicolo", 207, "Magazzino"), ("TK1", "Serbatoio", 210, "Processo"),
        ("P1", "Motore", 211, "Processo"), ("XV1", "Valvola", 212, "Processo"), ("LT1", "Strumento", 213, "Processo")]
names = {i["name"] for i in p["udt_instances"]}
for n, t, db, area in inst:
    if n not in names:
        p["udt_instances"].append({"name": n, "type": t, "driver": "WCS_INTERNO", "area": area, "params": {"DB": db, "Base": 0}})
p.setdefault("timer_scripts", [])
if not any(t["name"] == "sim_magazzino" for t in p["timer_scripts"]):
    p["timer_scripts"].append({"name": "sim_magazzino", "function": "sim_magazzino", "interval_ms": 200, "enabled": True})
# allarme di esempio su un membro di UDT: livello alto serbatoio
r = call("PUT", "/api/project", p)
print("progetto:", r)
meta = lib["symbols"]

# avvio nastri
for cv in ("CVM1", "CVM2"):
    call("POST", "/api/tags/write", {"tag": f"{cv}.Cmd_Start", "value": True})

comps = []
def C(cid, ctype, x, y, w, h, props=None, bindings=None, events=None, custom=None):
    comps.append({"id": cid, "type": ctype, "name": cid, "x": x, "y": y, "w": w, "h": h, "rotation": 0,
                  "props": props or {}, "bindings": bindings or {}, "events": events or {}, **({"custom": custom} if custom else {})})

def S(cid, sym, x, y, w, h, device, rot=0):
    m = meta[sym]
    C(cid, "symbol", x, y, w, h, {"symbol": sym}, json.loads(json.dumps(m["bindings"])),
      {"onClick": {"actions": [{"kind": "popup", "view": m["faceplate"], "params": {"device": "{device}"}}]}},
      {"device": device})
    comps[-1]["rotation"] = rot

C("title", "label", 30, 16, 700, 36, {"text": "Magazzino automatico", "fontSize": 24, "bold": True})
C("hint", "label", 30, 50, 900, 22, {"text": "Click su un elemento per aprire il suo faceplate (il tag del dispositivo è passato come parametro 'device')", "fontSize": 13, "color": "#64748b"})
C("zone1", "rect", 20, 84, 920, 560, {"fill": "#0f172a", "stroke": "#1e293b", "radius": 12})
C("zone2", "rect", 960, 84, 300, 560, {"fill": "#0f172a", "stroke": "#1e293b", "radius": 12})
C("ztxt2", "label", 975, 96, 260, 24, {"text": "Processo – acqua di servizio", "fontSize": 14, "color": "#94a3b8"})
# baia e porte
S("dock", "door_dock", 50, 120, 110, 117, "DOCK1")
S("cvm1", "conv_roller", 180, 160, 220, 60, "CVM1")
S("door", "door_rapid", 410, 120, 95, 102, "DOOR1")
S("cvm2", "conv_belt", 520, 160, 200, 60, "CVM2")
S("lift", "lift_vertical", 735, 100, 120, 160, "LIFT1")
# magazzino
S("rack", "asrs_rack", 60, 330, 420, 300, "RACK1")
S("crane", "asrs_crane", 60, 300, 60, 330, "CRANE1")
comps[-1]["bindings"]["elements.mast.tx"] = {"kind": "expr", "expr": "(tag('{device}.PctX') || 0) * 4.3"}
S("rack2", "asrs_rack", 500, 330, 420, 300, "RACK1")
S("agv", "veh_agv", 540, 255, 110, 76, "AGV1")
# processo
S("tank", "proc_tank", 1050, 140, 110, 175, "TK1")
S("lt", "proc_level", 1175, 150, 70, 70, "LT1")
S("pump", "proc_pump", 990, 360, 90, 90, "P1")
S("valve", "proc_valve", 1150, 380, 90, 72, "XV1")
C("pipe1", "line", 1035, 330, 60, 6, {"stroke": "#94a3b8", "strokeWidth": 10}, {"flow": {"kind": "tag", "tag": "P1.Run"}}, None)
comps[-1]["rotation"] = 90
C("pipe2", "line", 1160, 330, 50, 6, {"stroke": "#94a3b8", "strokeWidth": 10}, {"flow": {"kind": "tag", "tag": "XV1.Open"}})
comps[-1]["rotation"] = 90
C("trend", "trend", 975, 480, 270, 150, {"tags": "TK1.Level,P1.Current", "minutes": 5})
view = {"title": "Magazzino", "width": 1280, "height": 660, "background": "#0b1220", "home": False, "area": "Magazzino",
        "params": {}, "components": comps}
print("vista:", call("PUT", "/api/views/magazzino", view))
