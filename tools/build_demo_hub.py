#!/usr/bin/env python3
"""Demo grande: hub di smistamento pacchi con il layer fisico (oltre 60 nastri).

Aree e linee:
  Hub
  ├─ Ricevimento: Baia 1, Baia 2, Baia 3 (scarico camion → merge sul collettore), Collettore
  │                (collettore, pre-smistatore SC0 Nord/Sud, linea scarti con postazione manuale)
  ├─ Sorter Nord: immissione con merge del ricircolo, scanner SCN, 7 deviatori (6 scivoli + scarti),
  │               ricircolo ad anello
  └─ Sorter Sud:  come il Nord

Genera:
- tag e UDT (HubLinea, HubNastro) sul driver PLC_HUB (plc_hub, Modbus 5021), ingressi simulati;
- punti di decisione HSC0 / HSCN / HSCS (script hub_presort, hub_sort_nord, hub_sort_sud);
- viste hub (panoramica), hub_ricevimento, hub_nord, hub_sud con le proprietà fisiche
  (i nastri proseguono da una vista all'altra con riferimenti vista/id);
- config/hub/plc_hub.json: la "configurazione hardware" della logica di plc_hub.

Uso (server avviato):  python3 tools/build_demo_hub.py [http://127.0.0.1:8080]
Poi riavviare plc_hub:  cargo run --release --bin plc_hub
Con la sicurezza attiva servono PTWCS_USER / PTWCS_PASSWORD (progettista o admin).
"""
import json
import math
import os
import sys
import urllib.error
import urllib.request

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080"
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
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


DRV = "PLC_HUB"
SPEED = 0.8   # m/s nastri di trasporto
SORT_SPEED = 1.2  # m/s sorter (più veloce: separa i colli)

# ------------------------------------------------------------------ modello logico (indici PLC)
lines = []      # (istanza, nome, area)
convs = []      # dict: inst, line, area, gate(main_pe name)
pes = []        # (tag, area, descrizione)
divs = []       # (tag, area)
counters = []   # (tag, pe name, area, descrizione)
plc = {"lines": [], "conveyors": [], "scanners": [], "sorters": [], "counters": []}


def line(inst, name, area):
    n = len(lines)
    lines.append((inst, name, area))
    plc["lines"].append({"name": inst, "n": n})
    return n


def conveyor(inst, ln, area, desc):
    n = len(convs)
    convs.append({"inst": inst, "line": ln, "area": area, "desc": desc, "n": n})
    return n


def pe(tag, area, desc):
    pes.append((tag, area, desc))
    return len(pes) - 1


def pe_idx(tag):
    return next(i for i, p in enumerate(pes) if p[0] == tag)


def div(tag, area):
    divs.append((tag, area))
    return len(divs) - 1


def counter(tag, pe_tag, area, desc):
    ir = 10 + len(counters)
    counters.append((tag, pe_tag, area, desc, ir))
    plc["counters"].append({"pe": pe_idx(pe_tag), "ir": ir})


# ------------------------------------------------------------------ viste: utilità di disegno
B = lambda tag: {"kind": "tag", "tag": tag}


def comp(id, type, x, y, w, h, props=None, bindings=None, events=None, physics=None, rotation=0):
    c = {"id": id, "name": id, "type": type, "x": round(x, 1), "y": round(y, 1), "w": round(w, 1), "h": h,
         "rotation": rotation, "props": props or {}, "bindings": bindings or {}, "events": events or {}}
    if physics:
        c["physics"] = physics
    return c


class Sheet:
    """Una vista: componenti, nastri con geometria (inizio, angolo, lunghezza in m)."""

    def __init__(self, vid):
        self.vid, self.C, self.ends = vid, [], {}

    def conv(self, inst, sx, sy, length_m, angle, label, nxt="", speed=SPEED, gap=None, sink=False, h=40, label_on=True):
        L = length_m * 100
        a = math.radians(angle)
        cx, cy = sx + math.cos(a) * L / 2, sy + math.sin(a) * L / 2
        ph = {"kind": "conveyor", "run": f"{inst}.Run", "speed": speed, "accel": 1.0, "decel": 2.0,
              "speed_fb": f"{inst}.Speed", "fb_scale": 100}
        if nxt:
            ph["next"] = nxt
        if sink:
            ph["sink"] = True
        if gap:
            ph["gap_m"] = gap
        props = {"label": label if label_on else ""}
        rot = angle
        if angle in (180, -180):  # verso sinistra: direzione "left" (etichetta leggibile)
            props["direction"], rot = "left", 0
        self.C.append(comp(inst, "conveyor", cx - L / 2, cy - h / 2, L, h, props,
                           {"running": B(f"{inst}.Run"), "fault": B(f"{inst}.Fault")}, physics=ph, rotation=rot))
        end = (sx + math.cos(a) * L, sy + math.sin(a) * L)
        self.ends[inst] = (sx, sy, end, angle, length_m)
        return end

    def at(self, inst, s_m):
        sx, sy, _, ang, _ = self.ends[inst]
        a = math.radians(ang)
        return sx + math.cos(a) * s_m * 100, sy + math.sin(a) * s_m * 100, ang

    def photocell(self, tag, inst, s_m, size=22):
        x, y, ang = self.at(inst, s_m)
        cid = tag.replace(".", "_")
        self.C.append(comp(cid, "symbol", x - size / 2, y - size / 2, size, size, {"symbol": "dev_photocell"},
                           {"elements.beam.stroke": {"kind": "tag", "tag": tag, "transform": {"kind": "map", "rules": [{"in": "true", "out": "#f59e0b"}], "default": "#22c55e"}}},
                           physics={"kind": "photocell", "on": inst, "at_m": round(s_m, 3), "tag": tag}, rotation=ang + 90))

    def scanner(self, cid, inst, s_m, sim):
        x, y, _ = self.at(inst, s_m)
        self.C.append(comp(cid, "symbol", x - 27, y - 100, 54, 64, {"symbol": "scanner"}, {},
                           physics={"kind": "scanner", "on": inst, "at_m": s_m, "code_tag": f"{sim}.SimCode", "count_tag": f"{sim}.SimCount",
                                    "weight_tag": f"{sim}.SimWeight", "weight_scale": 100, "noread_pct": 2}))

    def label(self, id, x, y, text, size=13, color="#94a3b8", bold=False, w=None):
        self.C.append(comp(id, "label", x, y, w or max(60, len(text) * size * 0.6), size + 10, {"text": text, "fontSize": size, "color": color, "bold": bold}))

    def value(self, id, x, y, tag, label, w=110, color=None):
        self.C.append(comp(id, "value", x, y, w, 30, {"label": label, "fontSize": 15, **({"color": color} if color else {})}, {"value": B(tag)}))

    def panel(self, id, x, y, w, h, title=None):
        self.C.append(comp(id, "rect", x, y, w, h, {"fill": "#0f172a", "radius": 10, "stroke": "#1e293b"}))
        if title:
            self.label(f"{id}_t", x + 12, y + 6, title, 14, "#cbd5e1", True)

    def line_btn(self, id, x, y, inst, name, w=170):
        self.C.append(comp(id, "button", x, y, w, 36, {"text": name, "fill": "#475569", "activeFill": "#15803d", "fontSize": 13},
                           {"active": B(f"{inst}.Running"),
                            "text": {"kind": "expr", "expr": f"'{name}: ' + (tag('{inst}.Running') ? 'IN MARCIA' : tag('{inst}.Enable') ? 'GUASTO' : 'FERMA')"}},
                           {"onClick": {"actions": [{"kind": "write", "tag": f"{inst}.Enable", "value": "toggle"}], "confirm": f"Avviare/fermare {name}?"}}))

    def view(self, title, area, w=1600, h=1000, home=False):
        return {"title": title, "width": w, "height": h, "background": "#0b1220", "area": area, "home": home,
                "components": self.C, "physics": {"scale_mm": 10}}


div_rect = lambda id, x, y, tag, ang=0: comp(id, "rect", x - 9, y - 26, 18, 52, {"fill": "#475569", "radius": 3, "stroke": "#0f172a"},
                                             {"fill": {"kind": "expr", "expr": f"tag('{tag}') == 1 ? '#f59e0b' : '#64748b'"}}, rotation=ang)

BARCODES = [f"{p}-{n:06d}" for p in ("IT-MI", "IT-TO", "IT-BO", "IT-VE", "IT-GE", "IT-RM", "IT-NA", "IT-FI", "IT-BA", "IT-PA")
            for n in (1201, 4512, 7788)] + ["DE-BE-112233", "FR-PA-556677", "CH-ZH-101010", "ES-MD-445566", "GR-AT-778800", "XX-ZZ-000001"]

# ================================================================== RICEVIMENTO
R = Sheet("hub_ricevimento")
R.label("title", 30, 18, "Hub – Ricevimento e pre-smistamento", 24, "#e2e8f0", True, 700)
R.label("hint", 760, 26, "3 baie di scarico → merge sul collettore (priorità alla linea principale) → scanner SC0 → Nord / Sud / scarti", 13, "#64748b", w=830)
L_COL = None
rows = [(170, 1), (370, 2), (570, 3)]
COLX = 1000
col_starts = [150, 350, 550, 750]  # inizio segmenti del collettore (verso il basso)
L_COL = line("HUB_COL", "Collettore e pre-smistamento", "Hub-Collettore")
for y, k in rows:
    ln = line(f"HUB_B{k}", f"Baia {k}", f"Hub-Baia{k}")
    area = f"Hub-Baia{k}"
    R.panel(f"pb{k}", 20, y - 70, 930, 150, f"Baia {k} – scarico camion")
    R.line_btn(f"bb{k}", 760, y - 64, f"HUB_B{k}", f"Baia {k}")
    names = [f"B{k}_C{i}" for i in range(1, 5)]
    for i, nm in enumerate(names):
        conveyor(nm, ln, area, f"Baia {k} – {'nastro telescopico' if i == 0 else 'immissione sul collettore' if i == 3 else f'trasporto {i}'}")
    x = 60
    x = R.conv(names[0], x, y, 3.0, 0, "Telescopico", names[1])[0]
    x = R.conv(names[1], x, y, 2.5, 0, f"B{k}-C2", names[2])[0]
    x = R.conv(names[2], x, y, 2.5, 0, f"B{k}-C3", names[3])[0]
    R.conv(names[3], x, y, 1.2, 0, "C4 merge", f"COL_{k}")
    R.C.append(comp(f"src{k}", "label", 60, y - 44, 150, 20, {"text": f"⊕ camion baia {k}", "fontSize": 12, "color": "#4ade80"},
                    physics={"kind": "source", "on": names[0], "interval_min": 3.0, "interval_max": 6.5, "materials": "Collo,Pacco,Busta",
                             "barcodes": ",".join(BARCODES), "enable": f"HUB_B{k}.Enable"}))
    p = pe(f"HUB.PE_B{k}_OUT", area, f"Baia {k}: colli scaricati")
    R.photocell(f"HUB.PE_B{k}_OUT", names[2], 2.2)
    counter(f"HUB.B{k}_Count", f"HUB.PE_B{k}_OUT", area, f"Baia {k}: colli scaricati")
    R.value(f"cb{k}", 560, y - 64, f"HUB.B{k}_Count", "Colli", 120)
# collettore verticale (verso il basso) e linea di pre-smistamento
R.panel("pcol", 960, 90, 620, 950, "Collettore e pre-smistamento")
R.line_btn("bcol", 1380, 96, "HUB_COL", "Collettore", 190)
for i in range(4):
    nm = f"COL_{i + 1}"
    conveyor(nm, L_COL, "Hub-Collettore", f"Collettore segmento {i + 1}")
    R.conv(nm, COLX, col_starts[i], 2.0, 90, f"COL-{i + 1}", f"COL_{i + 2}" if i < 3 else "PS_1")
# merge: la baia k (k ≥ 2) attende se la fotocellula di fine del segmento precedente è occupata
for k in (2, 3):
    pe(f"HUB.PE_COL{k - 1}_END", "Hub-Collettore", f"Collettore {k - 1}: fine segmento (merge baia {k})")
    R.photocell(f"HUB.PE_COL{k - 1}_END", f"COL_{k - 1}", 1.7)
for k in (2, 3):
    next(c for c in convs if c["inst"] == f"B{k}_C4")["gate"] = f"HUB.PE_COL{k - 1}_END"
for nm, d in (("PS_1", "Pre-smistamento: scanner SC0"), ("PS_2", "Pre-smistamento: ingresso deviatore")):
    conveyor(nm, L_COL, "Hub-Collettore", d)
x = R.conv("PS_1", COLX, 950, 2.4, 0, "PS-1 scanner", "PS_2", speed=1.1, gap=0.3)[0]
x = R.conv("PS_2", x, 950, 1.4, 0, "PS-2", "DV0", speed=1.1)[0]
R.scanner("SC0", "PS_1", 1.2, "HSIM0")
pe("HUB.PE_DV0", "Hub-Collettore", "Pre-smistamento: collo davanti al deviatore")
R.photocell("HUB.PE_DV0", "PS_2", 0.6)
d0 = div("HUB.DV0_Pos", "Hub-Collettore")
DVX = x + 30
R.C.append(comp("DV0", "symbol", DVX - 40, 950 - 40, 80, 80, {"symbol": "diverter"},
                {"elements.flap.fill": {"kind": "tag", "tag": "HUB.DV0_Pos", "transform": {"kind": "map", "rules": [{"in": "99", "out": "#ef4444"}], "default": "#f59e0b"}},
                 "elements.flap.rotate": {"kind": "tag", "tag": "HUB.DV0_Pos", "transform": {"kind": "map", "default": "0", "rules": [{"in": "1", "out": "-60"}, {"in": "2", "out": "0"}, {"in": "99", "out": "60"}]}}},
                physics={"kind": "diverter", "position": "HUB.DV0_Pos", "switch_s": 0.3,
                         "outputs": [{"value": "1", "next": "PSN_1"}, {"value": "2", "next": "PSS_1"}, {"value": "99", "next": "REJ_1"}]}))
# uscite del pre-smistatore: Nord (verso l'alto), Sud (a destra, poi giù), scarti (in basso a sinistra)
for nm, d in (("PSN_1", "Alimentazione sorter Nord 1"), ("PSN_2", "Alimentazione sorter Nord 2"), ("PSN_3", "Alimentazione sorter Nord 3"),
              ("PSS_1", "Alimentazione sorter Sud 1"), ("PSS_2", "Alimentazione sorter Sud 2"),
              ("REJ_1", "Linea scarti 1"), ("REJ_2", "Linea scarti 2")):
    conveyor(nm, L_COL, "Hub-Collettore", d)
y = R.conv("PSN_1", DVX + 40, 930, 2.6, -90, "→ Nord", "PSN_2")[1]
y = R.conv("PSN_2", DVX + 40, y, 2.6, -90, "PSN-2", "PSN_3")[1]
R.conv("PSN_3", DVX + 40, y, 2.6, -90, "PSN-3", "hub_nord/SN_IN")
R.label("toN", DVX - 120, y - 10, "↗ Sorter Nord", 14, "#e879f9", True)
x2 = R.conv("PSS_1", DVX + 60, 950, 1.0, 0, "→ Sud", "PSS_2", label_on=False)[0]
R.conv("PSS_2", x2, 950, 1.0, -90, "PSS-2", "hub_sud/SS_IN", label_on=False)
R.label("toS", x2 - 150, 815, "↗ Sorter Sud", 14, "#e879f9", True)
yr = R.conv("REJ_1", DVX - 10, 1010, 1.0, 180, "Scarti", "REJ_2", label_on=False)
R.conv("REJ_2", yr[0], yr[1], 1.6, 180, "Scarti", "", sink=True, label_on=False)
R.label("rej", yr[0] - 440, 998, "✋ postazione scarti (codifica manuale)", 12, "#fca5a5", w=260)
for tag, inst, s, area, d, ctag in (("HUB.PE_PSN", "PSN_1", 0.5, "Hub-Collettore", "verso Nord", "HUB.PS_Nord"),
                                     ("HUB.PE_PSS", "PSS_1", 0.5, "Hub-Collettore", "verso Sud", "HUB.PS_Sud"),
                                     ("HUB.PE_REJ", "REJ_2", 1.4, "Hub-Collettore", "agli scarti", "HUB.PS_Scarti")):
    pe(tag, area, f"Pre-smistamento: collo {d}")
    R.photocell(tag, inst, s)
    counter(ctag, tag, area, f"Pre-smistamento: colli {d}")
R.value("cpn", 1250, 600, "HUB.PS_Nord", "→ Nord", 130)
R.value("cps", 1250, 650, "HUB.PS_Sud", "→ Sud", 130)
R.value("cpr", 1250, 700, "HUB.PS_Scarti", "Scarti", 130, "#fca5a5")
R.C.append(comp("scbc", "value", 1160, 870, 200, 30, {"label": "SC0", "fontSize": 14}, {"value": B("HSC0.Barcode")}))
plc["scanners"].append({"s": 0, "base": 3000, "sorter": 0})
plc["sorters"].append({"name": "Pre-smistamento", "stages": [{"d": d0, "pe": pe_idx("HUB.PE_DV0"), "kind": "route", "routes": [1, 2, 99], "default": 99}]})

# ================================================================== SORTER (Nord, Sud)
CITY = {1: ["Milano", "Torino", "Bologna", "Venezia", "Genova", "Estero"],
        2: ["Roma", "Napoli", "Firenze", "Bari", "Palermo", "Estero"]}
sheets = {}
for si, (P, name, vid, area, sc) in enumerate(((("SN", "Nord", "hub_nord", "Hub-SorterNord", "HSCN")),
                                                (("SS", "Sud", "hub_sud", "Hub-SorterSud", "HSCS"))), start=1):
    S = Sheet(vid)
    sheets[vid] = S
    ln = line(f"HUB_{P}", f"Sorter {name}", area)
    S.label("title", 30, 18, f"Hub – Sorter {name}", 24, "#e2e8f0", True, 500)
    S.label("hint", 560, 26, "anello: immissione (merge con il ricircolo) → scanner → 7 deviatori (6 scivoli + scarti) → ricircolo", 13, "#64748b", w=900)
    S.panel("pl", 20, 70, 1560, 640)
    S.line_btn("bl", 1360, 80, f"HUB_{P}", f"Sorter {name}", 200)
    Y = 330
    for nm, d in ((f"{P}_IN", "alimentazione dal pre-smistamento"), (f"{P}_I1", "immissione (merge ricircolo)"), (f"{P}_I2", f"scanner {sc}"), (f"{P}_I3", "accelerazione")):
        conveyor(nm, ln, area, f"Sorter {name}: {d}")
    x = S.conv(f"{P}_IN", 30, Y, 1.3, 0, "Ingresso", f"{P}_I1")[0]
    x = S.conv(f"{P}_I1", x + 20, Y, 1.8, 0, "I1 merge", f"{P}_I2")[0]
    x = S.conv(f"{P}_I2", x, Y, 2.0, 0, "I2 scanner", f"{P}_I3", speed=1.0, gap=0.35)[0]
    x = S.conv(f"{P}_I3", x, Y, 1.4, 0, "I3", f"{P}_S1", speed=SORT_SPEED)[0]
    S.scanner(sc, f"{P}_I2", 1.0, f"{sc}SIM")
    S.C.append(comp("scbc", "value", x - 380, Y - 160, 230, 30, {"label": sc, "fontSize": 14}, {"value": B(f"{sc}.Barcode")}))
    stages = []
    for k in range(1, 8):
        sec, ch, dv = f"{P}_S{k}", f"{P}_CH{k}", f"{P}_D{k}"
        conveyor(sec, ln, area, f"Sorter {name}: sezione {k}")
        conveyor(ch, ln, area, f"Sorter {name}: scivolo {k}" + (" scarti" if k == 7 else f" – {CITY[si][k - 1]}"))
        x = S.conv(sec, x, Y, 1.2, 0, f"S{k}", f"{P}_D{k}", speed=SORT_SPEED, label_on=False)[0]
        dpos = f"{P}.D{k}_Pos"
        di = div(dpos, area)
        S.C.append(div_rect(dv, x, Y, dpos))
        S.C[-1]["physics"] = {"kind": "diverter", "position": dpos, "switch_s": 0.25,
                              "outputs": [{"value": "0", "next": f"{P}_S{k + 1}" if k < 7 else f"{P}_R1"}, {"value": "1", "next": ch}]}
        S.conv(ch, x + 2, Y + 30, 2.4, 90, "", "", speed=SPEED, sink=True, h=36, label_on=False)
        pe_d, pe_f, pe_c = f"{P}.PE_D{k}", f"{P}.PE_CH{k}_FULL", f"{P}.PE_CH{k}"
        pe(pe_d, area, f"Sorter {name}: collo davanti al deviatore {k}")
        pe(pe_f, area, f"Sorter {name}: scivolo {k} pieno")
        pe(pe_c, area, f"Sorter {name}: collo nello scivolo {k}")
        S.photocell(pe_d, sec, 0.55)
        S.photocell(pe_f, ch, 0.35)
        S.photocell(pe_c, ch, 2.2)
        counter(f"{P}.CH{k}_Count", pe_c, area, f"Sorter {name}: colli nello scivolo {k}")
        cname = "Scarti" if k == 7 else CITY[si][k - 1]
        S.label(f"chl{k}", x - 50, Y + 290, f"{k if k < 7 else '✋'} {cname}", 12, "#fca5a5" if k == 7 else "#cbd5e1", w=110)
        S.value(f"chc{k}", x - 45, Y + 312, f"{P}.CH{k}_Count", "", 90, "#fca5a5" if k == 7 else None)
        stages.append({"d": di, "pe": pe_idx(pe_d), "kind": "chute", "chute": 99 if k == 7 else k, "full_pe": pe_idx(pe_f)})
    # ricircolo: su, a sinistra (due tratti), giù fino all'immissione
    for nm, d in ((f"{P}_R1", "ricircolo salita"), (f"{P}_R2", "ricircolo 2"), (f"{P}_R3", "ricircolo 3"), (f"{P}_R4", "ricircolo discesa")):
        conveyor(nm, ln, area, f"Sorter {name}: {d}")
    xe = x
    S.conv(f"{P}_R1", xe + 20, Y, 2.0, -90, "R1", f"{P}_R2", speed=SPEED)
    xm = (xe + 20 + 180) / 2
    S.conv(f"{P}_R2", xe + 20, Y - 200, (xe + 20 - xm) / 100, 180, "Ricircolo", f"{P}_R3")
    S.conv(f"{P}_R3", xm, Y - 200, (xm - 180) / 100, 180, "Ricircolo", f"{P}_R4")
    S.conv(f"{P}_R4", 180, Y - 200, 2.0, 90, "R4", f"{P}_I1")
    pe_r = f"{P}.PE_R4_END"
    pe(pe_r, area, f"Sorter {name}: ricircolo all'immissione")
    S.photocell(pe_r, f"{P}_R4", 1.8)
    next(c for c in convs if c["inst"] == f"{P}_IN")["gate"] = pe_r  # il ricircolo ha la precedenza
    S.label("rcl", xm - 160, Y - 250, "⟲ ricircolo (scivolo pieno o nessuna risposta)", 12, "#94a3b8", w=320)
    S.C.append(comp("rcc", "value", xm + 200, Y - 262, 120, 30, {"label": "Ricircoli", "fontSize": 14}, {"value": B(f"{P}.Recirc")}))
    plc["scanners"].append({"s": si, "base": 3000 + 20 * si, "sorter": si})
    plc["sorters"].append({"name": f"Sorter {name}", "stages": stages, "recirc_ir": 200 + si})

# PLC: nastri con merge
for c in convs:
    e = {"name": c["inst"], "n": c["n"], "line": c["line"]}
    if c.get("gate"):
        e["gate"] = {"main_pe": pe_idx(c["gate"]), "max_wait_s": 6}
    plc["conveyors"].append(e)

# ================================================================== progetto
p = call("GET", "/api/project")["project"]
for a in [{"name": "Hub", "description": "Hub di smistamento pacchi (demo grande con layer fisico)"},
          {"name": "Hub-Ricevimento", "description": "Baie di scarico e pre-smistamento", "parent": "Hub"},
          {"name": "Hub-Baia1", "description": "Baia di scarico 1", "parent": "Hub-Ricevimento"},
          {"name": "Hub-Baia2", "description": "Baia di scarico 2", "parent": "Hub-Ricevimento"},
          {"name": "Hub-Baia3", "description": "Baia di scarico 3", "parent": "Hub-Ricevimento"},
          {"name": "Hub-Collettore", "description": "Collettore, pre-smistatore SC0 e scarti", "parent": "Hub-Ricevimento"},
          {"name": "Hub-SorterNord", "description": "Sorter Nord ad anello, 6 scivoli + scarti", "parent": "Hub"},
          {"name": "Hub-SorterSud", "description": "Sorter Sud ad anello, 6 scivoli + scarti", "parent": "Hub"}]:
    upsert(p["areas"], a)
upsert(p["udt_types"], {"name": "HubLinea", "description": "Linea dell'hub: abilitazione dal WCS e stato dal PLC", "params": [{"name": "N", "default": 0, "description": "Numero linea"}],
                        "members": [{"name": "Enable", "type": "bool", "address": "coil:{N+100}", "writable": True, "description": "Abilitazione (avvio/arresto)"},
                                    {"name": "Running", "type": "bool", "address": "di:{N+100}", "description": "In marcia senza guasti"}]})
upsert(p["udt_types"], {"name": "HubNastro", "description": "Nastro dell'hub: marcia e guasto dal PLC, velocità reale (ingresso simulato)",
                        "params": [{"name": "N", "default": 0, "description": "Numero nastro"}],
                        "members": [{"name": "Run", "type": "bool", "address": "coil:{N+1000}", "description": "Marcia (uscita PLC)"},
                                    {"name": "Fault", "type": "bool", "address": "di:{N+1000}", "description": "Guasto motore",
                                     "alarms": [{"name": "FAULT", "op": "==", "value": True, "message": "{Instance}: guasto motore (marcia senza velocità)", "severity": "alarm"}]},
                                    {"name": "Speed", "type": "u16", "address": "hr:{N+1000}", "writable": True, "unit": "m/s×100", "description": "Velocità reale (ingresso simulato)"}]})
p["udt_instances"] = [i for i in p["udt_instances"] if i.get("type") not in ("HubLinea", "HubNastro") and not i["name"].startswith("HSC")]
for inst, nm, area in lines:
    p["udt_instances"].append({"name": inst, "type": "HubLinea", "driver": DRV, "area": area, "params": {"N": [l[0] for l in lines].index(inst)}, "description": nm})
for c in convs:
    p["udt_instances"].append({"name": c["inst"], "type": "HubNastro", "driver": DRV, "area": c["area"], "params": {"N": c["n"]}, "description": c["desc"]})
for s, (sc, area, desc) in enumerate((("HSC0", "Hub-Collettore", "Pre-smistatore SC0"), ("HSCN", "Hub-SorterNord", "Scanner sorter Nord"), ("HSCS", "Hub-SorterSud", "Scanner sorter Sud"))):
    p["udt_instances"].append({"name": sc, "type": "TelegrammaScanner", "driver": DRV, "area": area, "scan_class": "veloce", "params": {"Base": 3000 + 20 * s}, "description": desc})
HUB_PREFIX = ("HUB.", "SN.", "SS.", "HSIM0.", "HSCNSIM.", "HSCSSIM.", "HubSim.")
p["tags"] = [t for t in p["tags"] if not t["name"].startswith(HUB_PREFIX)]


def T(name, addr, typ, area, desc, **kw):
    p["tags"].append({"name": name, "driver": DRV, "address": addr, "type": typ, "area": area, "description": desc, **kw})


for k, (tag, area, desc) in enumerate(pes):
    T(tag, f"coil:{2000 + k}", "bool", area, f"{desc} (ingresso simulato)", writable=True, scan_class="veloce")
for k, (tag, area) in enumerate(divs):
    T(tag, f"hr:{1100 + k}", "u16", area, "Posizione deviatore (uscita PLC)", scan_class="veloce")
for s, sim in enumerate(("HSIM0", "HSCNSIM", "HSCSSIM")):
    area = ("Hub-Collettore", "Hub-SorterNord", "Hub-SorterSud")[s]
    T(f"{sim}.SimCount", f"hr:{2000 + 16 * s}", "u16", area, "Scanner: contatore letture (ingresso simulato)", writable=True)
    T(f"{sim}.SimCode", f"hr:{2001 + 16 * s}", "string", area, "Scanner: barcode (ingresso simulato)", writable=True, len=20)
    T(f"{sim}.SimWeight", f"hr:{2011 + 16 * s}", "u16", area, "Scanner: peso (ingresso simulato)", writable=True, unit="g×10")
for tag, pe_tag, area, desc, ir in counters:
    T(tag, f"ir:{ir}", "u16", area, desc, historize=True)
for si, P in ((1, "SN"), (2, "SS")):
    T(f"{P}.Recirc", f"ir:{200 + si}", "u16", ("Hub-SorterNord", "Hub-SorterSud")[si - 1], "Colli ricircolati", historize=True)
T("HubSim.Active", "coil:99", "bool", "Hub", "Simulazione attiva (PLC hub)", writable=True)
T("HubSim.Heartbeat", "hr:99", "u16", "Hub", "Watchdog della simulazione (PLC hub)", writable=True)
T("HUB.ResetFaults", "coil:30", "bool", "Hub", "Reset guasti e inseguimento", writable=True)
for name, fn, desc, area in (("HSC0", "hub_presort", "Hub: pre-smistamento Nord/Sud/scarti", "Hub-Collettore"),
                             ("HSCN", "hub_sort_nord", "Hub: sorter Nord, scelta dello scivolo", "Hub-SorterNord"),
                             ("HSCS", "hub_sort_sud", "Hub: sorter Sud, scelta dello scivolo", "Hub-SorterSud")):
    upsert(p["decision_points"], {"name": name, "description": desc, "telegram": name, "handshake": "counter", "function": fn,
                                  "timeout_ms": 400, "fallback": {"destination": 99, "result": 9}, "area": area})
ph = p.get("physics") or {}
ph.setdefault("materials", [])
ph["drivers"] = sorted(set(ph.get("drivers", [])) | {DRV})
hb = [t for t in (ph.get("heartbeat_tag") or "").split(",") if t.strip() and t.strip() != "HubSim.Heartbeat"] + ["HubSim.Heartbeat"]
ac = [t for t in (ph.get("active_tag") or "").split(",") if t.strip() and t.strip() != "HubSim.Active"] + ["HubSim.Active"]
ph["heartbeat_tag"], ph["active_tag"] = ",".join(hb), ",".join(ac)
if not ph["materials"]:
    ph["materials"] = [{"name": "Collo", "length_m": 0.4, "width_m": 0.3, "mass_min": 1, "mass_max": 18, "color": "#c8915a", "share": 3},
                       {"name": "Pacco", "length_m": 0.6, "width_m": 0.4, "mass_min": 8, "mass_max": 34, "color": "#a16207", "share": 1},
                       {"name": "Busta", "length_m": 0.3, "width_m": 0.25, "mass_min": 0.1, "mass_max": 2, "color": "#e5e7eb", "share": 1}]
p["physics"] = ph
r = call("PUT", "/api/project", p)
print("progetto:", r.get("ok"), (r.get("warnings") or "")[:3] if r.get("warnings") else "")

# ================================================================== panoramica
O = Sheet("hub")
O.label("title", 30, 18, "Hub di smistamento pacchi – panoramica", 26, "#e2e8f0", True, 900)
O.label("hint", 30, 58, f"{len(convs)} nastri · {len(pes)} fotocellule · 3 scanner · 15 deviatori · 6 linee · layer fisico: runtime → Simulazione", 14, "#64748b", w=1000)
nav = lambda id, x, y, view, text: O.C.append(comp(id, "button", x, y, 200, 40, {"text": text, "fill": "#1d4ed8", "fontSize": 14}, {},
                                                    {"onClick": {"actions": [{"kind": "navigate", "view": view}]}}))
O.panel("pr", 30, 100, 740, 400, "Ricevimento")
for i, k in enumerate((1, 2, 3)):
    O.line_btn(f"lb{k}", 50, 150 + i * 56, f"HUB_B{k}", f"Baia {k}", 260)
    O.value(f"vb{k}", 330, 150 + i * 56, f"HUB.B{k}_Count", "Colli scaricati", 170)
O.line_btn("lcol", 50, 318, "HUB_COL", "Collettore", 260)
O.value("vpn", 50, 380, "HUB.PS_Nord", "→ Nord", 150)
O.value("vps", 220, 380, "HUB.PS_Sud", "→ Sud", 150)
O.value("vpr", 390, 380, "HUB.PS_Scarti", "Scarti", 150, "#fca5a5")
O.C.append(comp("vbc", "value", 50, 430, 450, 30, {"label": "Ultima lettura SC0", "fontSize": 14}, {"value": B("HSC0.Barcode")}))
nav("nr", 550, 440, "hub_ricevimento", "▶ Ricevimento")
for j, (P, name, vid) in enumerate((("SN", "Nord", "hub_nord"), ("SS", "Sud", "hub_sud"))):
    x0 = 800 + j * 390
    O.panel(f"ps{P}", x0, 100, 370, 560, f"Sorter {name}")
    O.line_btn(f"lb{P}", x0 + 20, 150, f"HUB_{P}", f"Sorter {name}", 330)
    for k in range(1, 8):
        cname = "Scarti" if k == 7 else CITY[j + 1][k - 1]
        O.value(f"v{P}{k}", x0 + 20 + ((k - 1) % 2) * 170, 205 + ((k - 1) // 2) * 50, f"{P}.CH{k}_Count", f"{k} {cname}", 160, "#fca5a5" if k == 7 else None)
    O.value(f"r{P}", x0 + 190, 355, f"{P}.Recirc", "Ricircoli", 160)
    O.C.append(comp(f"bc{P}", "value", x0 + 20, 420, 330, 30, {"label": f"Ultima lettura", "fontSize": 14}, {"value": B(f"HSC{P[1]}.Barcode")}))
    nav(f"n{P}", x0 + 20, 600, vid, f"▶ Sorter {name}")
allen = [f"{l[0]}.Enable" for l in lines]
O.C.append(comp("all_on", "button", 30, 540, 230, 46, {"text": "▶ Avvia tutte le linee", "fill": "#15803d"}, {},
                {"onClick": {"actions": [{"kind": "write", "tag": t, "value": "true"} for t in allen]}}))
O.C.append(comp("all_off", "button", 275, 540, 230, 46, {"text": "■ Ferma tutte le linee", "fill": "#b91c1c"}, {},
                {"onClick": {"actions": [{"kind": "write", "tag": t, "value": "false"} for t in allen], "confirm": "Fermare tutte le linee dell'hub?"}}))
O.C.append(comp("reset", "button", 520, 540, 230, 46, {"text": "↺ Reset guasti", "fill": "#1d4ed8"}, {},
                {"onClick": {"actions": [{"kind": "write", "tag": "HUB.ResetFaults", "value": "true"}]}}))
O.C.append(comp("simon", "lamp", 40, 620, 22, 22, {"colorOn": "#a78bfa", "label": "Simulazione fisica attiva (PLC hub)"}, {"on": B("HubSim.Active")}))

for vid, sheet, title, area in (("hub", O, "Hub – panoramica", "Hub"), ("hub_ricevimento", R, "Hub – Ricevimento", "Hub-Ricevimento"),
                                ("hub_nord", sheets["hub_nord"], "Hub – Sorter Nord", "Hub-SorterNord"),
                                ("hub_sud", sheets["hub_sud"], "Hub – Sorter Sud", "Hub-SorterSud")):
    v = sheet.view(title, area, 1600, 1060 if vid == "hub_ricevimento" else 730 if vid != "hub" else 700)
    r = call("PUT", f"/api/views/{vid}", v)
    print(f"vista {vid}:", r.get("ok"), len(sheet.C), "componenti", (r.get("warnings") or [])[:3])

os.makedirs(os.path.join(ROOT, "config", "hub"), exist_ok=True)
with open(os.path.join(ROOT, "config", "hub", "plc_hub.json"), "w") as f:
    json.dump(plc, f, indent=1)
print(f"PLC: config/hub/plc_hub.json – {len(plc['conveyors'])} nastri, {len(pes)} fotocellule, {len(divs)} deviatori, {len(plc['counters'])} contatori")
st = call("GET", "/api/physics")
n_hub = sum(1 for c in st["components"] if c["key"].startswith("hub"))
print("modello fisico:", n_hub, "componenti dell'hub · errori:", st["errors"] or "nessuno")
print("→ riavvia plc_hub (cargo run --release --bin plc_hub) per caricare la nuova configurazione")
