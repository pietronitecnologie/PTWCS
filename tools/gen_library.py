#!/usr/bin/env python3
"""Generatore della libreria WCS-SCADA: simboli SVG, UDT per famiglia, binding
predefiniti e faceplate.

Uso:  python3 tools/gen_library.py [cartella_config]   (default: config)

Produce:
  <config>/symbols/<id>.svg          simboli (categoria, titolo, elementi animabili con id)
  <config>/library/udts.yaml         tipi UDT standard per famiglia (indirizzi S7 parametrici)
  <config>/library/symbols.json      metadati: categoria, famiglia, faceplate, binding predefiniti
  <config>/views/fp_<id>.json        faceplate (vista popup con parametro `device`)

Convenzioni degli id degli elementi (binding dal Designer):
  body   colore di stato        flow   flusso animato         load   unità di carico presente
  fault  segnalazione guasto    label  testo etichetta        + parti mobili specifiche
"""
import json
import os
import sys
from xml.sax.saxutils import escape

OUT = sys.argv[1] if len(sys.argv) > 1 else "config"

# ------------------------------------------------------------------ palette
FR, FRD, BODY, BELT, ROL, DARK = "#475569", "#334155", "#64748b", "#1e293b", "#94a3b8", "#0f172a"
LOAD, LOADD, PAL, ACC, GLASS = "#c08a4b", "#8a5a2b", "#a16207", "#f59e0b", "#38bdf8"
RUN, FAULT, STOP, WARN = "#22c55e", "#ef4444", "#64748b", "#f59e0b"
LBL = "#cbd5e1"


def attrs(d):
    out = []
    for k, v in d.items():
        if v is None:
            continue
        k = k.rstrip("_").replace("_", "-")
        out.append(f'{k}="{escape(str(v), {chr(34): "&quot;"})}"')
    return " ".join(out)


def E(tag, id=None, desc=None, hidden=False, inner="", **a):
    """Elemento SVG. id+desc lo rendono un elemento collegabile nel Designer."""
    if id:
        a = {"id": id, **a}
    if desc:
        a["data-desc"] = desc
    if hidden:
        a["style"] = "display:none" + (";" + a["style"] if "style" in a else "")
    s = attrs(a)
    return f"<{tag} {s}>{inner}</{tag}>" if inner else f"<{tag} {s}/>"


def G(id=None, desc=None, inner="", hidden=False, **a):
    return E("g", id, desc, hidden, inner, **a)


def text(x, y, s, id="label", size=12, fill=LBL, anchor="middle", desc="Testo etichetta", weight=400):
    return E("text", id, desc, inner=escape(s), x=x, y=y, font_size=size, fill=fill, text_anchor=anchor,
             font_family="sans-serif", font_weight=weight, dominant_baseline="central")


def fault_box(w, h, pad=1):
    return E("rect", "fault", "Segnalazione guasto (visibile)", True, x=pad, y=pad, width=w - 2 * pad,
             height=h - 2 * pad, fill="none", stroke=FAULT, stroke_width=3, stroke_dasharray="8 4", rx=4)


def flow_line(d, w=5, dash="14 12", op=0.45, id="flow"):
    dl = sum(int(x) for x in dash.split())
    return E("path", id, "Flusso animato (in marcia)", d=d, fill="none", stroke="#ffffff", stroke_width=w,
             stroke_dasharray=dash, opacity=op, stroke_linecap="butt", style=f"--dl:{dl}")


def carton(x, y, w, h, id="load", hidden=True, desc="Unità di carico presente (visibile)"):
    inner = (E("rect", x=x, y=y, width=w, height=h, fill=LOAD, stroke=LOADD, stroke_width=2, rx=2)
             + E("rect", x=x + w / 2 - 3, y=y, width=6, height=h, fill="#e9d3a5", opacity=0.8))
    return G(id, desc, inner, hidden)


def pallet(x, y, w, h, id="load", hidden=True, loaded=True):
    inner = E("rect", x=x, y=y, width=w, height=h, fill=PAL, stroke="#713f12", stroke_width=2)
    for i in range(1, 4):
        inner += E("line", x1=x, y1=y + h * i / 4, x2=x + w, y2=y + h * i / 4, stroke="#713f12", stroke_width=1.5)
    if loaded:
        inner += E("rect", x=x + 5, y=y + 5, width=w - 10, height=h - 10, fill=LOAD, stroke=LOADD, stroke_width=2, rx=2)
        inner += E("path", d=f"M{x + 5} {y + 5} L{x + w - 5} {y + h - 5} M{x + w - 5} {y + 5} L{x + 5} {y + h - 5}",
                   stroke=LOADD, stroke_width=1, opacity=0.5)
    return G(id, "Unità di carico presente (visibile)", inner, hidden)


def rollers(x0, x1, y0, y1, step=12, w=6, id=None):
    inner = "".join(E("rect", x=x - w / 2, y=y0, width=w, height=y1 - y0, rx=2, fill=ROL, opacity=0.9)
                    for x in frange(x0, x1, step))
    return G(id, "Rulli" if id else None, inner)


def frange(a, b, s):
    x = a
    while x <= b + 1e-9:
        yield round(x, 2)
        x += s


def arrow(cx, cy, s=8, id="dir"):
    return E("path", id, "Freccia di direzione", d=f"M{cx - s} {cy - s} L{cx + s * 0.6} {cy} L{cx - s} {cy + s}",
             fill="none", stroke="#ffffff", stroke_width=3, opacity=0.8, stroke_linejoin="round")


def lamp(cx, cy, r, id, desc, color="#334155"):
    return E("circle", id, desc, cx=cx, cy=cy, r=r, fill=color, stroke=DARK, stroke_width=1.5)


import math


def arc_pt(cx, cy, r, deg):
    a = math.radians(deg)
    return cx + r * math.cos(a), cy + r * math.sin(a)


# ------------------------------------------------------------------ famiglie (UDT)
# membri: (nome, tipo, scrivibile, unità, descrizione)
FAMILIES = {
    "Trasporto": ("Nastro/rulliera motorizzata", [
        ("Run", "bool", 0, "", "In marcia"), ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"),
        ("Occupied", "bool", 0, "", "Occupato"), ("Cmd_Start", "bool", 1, "", "Comando marcia"),
        ("Cmd_Stop", "bool", 1, "", "Comando arresto"), ("Cmd_Reset", "bool", 1, "", "Reset guasto"),
        ("Speed", "f32", 0, "m/min", "Velocità"), ("Current", "f32", 0, "A", "Corrente motore")]),
    "Accumulo": ("Rulliera ad accumulo a zone", [
        ("Run", "bool", 0, "", "In marcia"), ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"),
        ("Zone1", "bool", 0, "", "Zona 1 occupata"), ("Zone2", "bool", 0, "", "Zona 2 occupata"),
        ("Zone3", "bool", 0, "", "Zona 3 occupata"), ("Zone4", "bool", 0, "", "Zona 4 occupata"),
        ("Cmd_Start", "bool", 1, "", "Comando marcia"), ("Cmd_Stop", "bool", 1, "", "Comando arresto"),
        ("Cmd_Reset", "bool", 1, "", "Reset guasto"), ("Count", "u32", 0, "pz", "Colli in accumulo"),
        ("Speed", "f32", 0, "m/min", "Velocità")]),
    "Deviatore": ("Deviatore / trasferitore", [
        ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"), ("Occupied", "bool", 0, "", "Occupato"),
        ("Run", "bool", 0, "", "In movimento"), ("Cmd_Reset", "bool", 1, "", "Reset guasto"),
        ("Position", "u16", 0, "", "Posizione (0 dritto, n uscita)"), ("Cmd_Pos", "u16", 1, "", "Comando posizione"),
        ("Count", "u32", 0, "pz", "Colli deviati"), ("Angle", "f32", 0, "°", "Angolo")]),
    "Sorter": ("Sorter (cross-belt, pattini, vassoi)", [
        ("Run", "bool", 0, "", "In marcia"), ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"),
        ("Occupied", "bool", 0, "", "Carico presente"), ("Divert", "bool", 0, "", "Deviazione in corso"),
        ("Cmd_Start", "bool", 1, "", "Comando marcia"), ("Cmd_Stop", "bool", 1, "", "Comando arresto"),
        ("Cmd_Reset", "bool", 1, "", "Reset guasto"), ("Speed", "f32", 0, "m/s", "Velocità"),
        ("Throughput", "u32", 0, "pz/h", "Produttività"), ("Sorted", "u32", 0, "pz", "Colli smistati")]),
    "Scivolo": ("Scivolo / baia di uscita", [
        ("Full", "bool", 0, "", "Pieno"), ("Fault", "bool", 0, "", "Guasto"), ("Lamp", "bool", 0, "", "Lampada chiamata"),
        ("Enabled", "bool", 1, "", "Abilitato"), ("Count", "u32", 0, "pz", "Colli"), ("FillPct", "f32", 0, "%", "Riempimento")]),
    "Elevatore": ("Elevatore / lift", [
        ("Moving", "bool", 0, "", "In movimento"), ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"),
        ("Occupied", "bool", 0, "", "Carico a bordo"), ("Run", "bool", 0, "", "Rulliera di bordo in marcia"),
        ("Cmd_Reset", "bool", 1, "", "Reset guasto"), ("Level", "u16", 0, "", "Piano attuale"),
        ("Target", "u16", 1, "", "Piano destinazione"), ("Position", "f32", 0, "mm", "Posizione"),
        ("PosPct", "f32", 0, "%", "Posizione %")]),
    "Porta": ("Porta automatica", [
        ("Open", "bool", 0, "", "Aperta"), ("Closed", "bool", 0, "", "Chiusa"), ("Moving", "bool", 0, "", "In movimento"),
        ("Fault", "bool", 0, "", "Guasto"), ("Cmd_Open", "bool", 1, "", "Comando apertura"),
        ("Cmd_Close", "bool", 1, "", "Comando chiusura"), ("OpenPct", "f32", 0, "%", "Apertura")]),
    "Traslo": ("Traslo / trasloelevatore", [
        ("Moving", "bool", 0, "", "In movimento"), ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"),
        ("Loaded", "bool", 0, "", "Carico a bordo"), ("Cmd_Reset", "bool", 1, "", "Reset guasto"),
        ("Mission", "u32", 0, "", "Missione in corso"), ("PosX", "f32", 0, "m", "Posizione corsia"),
        ("PosY", "f32", 0, "m", "Posizione verticale"), ("PctX", "f32", 0, "%", "Posizione corsia %"),
        ("PctY", "f32", 0, "%", "Posizione verticale %"), ("Fork", "f32", 0, "%", "Forche (-100..100)")]),
    "Scaffale": ("Scaffalatura / magazzino", [
        *[(f"C{i}", "bool", 0, "", f"Cella {i} occupata") for i in range(1, 13)],
        ("Cells", "u16", 0, "", "Celle totali"), ("OccupiedCells", "u16", 0, "", "Celle occupate"),
        ("FillPct", "f32", 0, "%", "Saturazione")]),
    "Veicolo": ("Veicolo (AGV/AMR, shuttle, carrello)", [
        ("Moving", "bool", 0, "", "In movimento"), ("Fault", "bool", 0, "", "Guasto"), ("Loaded", "bool", 0, "", "Carico a bordo"),
        ("Charging", "bool", 0, "", "In carica"), ("Cmd_Pause", "bool", 1, "", "Pausa"), ("Cmd_Resume", "bool", 1, "", "Riprendi"),
        ("State", "u16", 0, "", "Stato"), ("Mission", "u32", 0, "", "Missione"), ("Battery", "f32", 0, "%", "Batteria"),
        ("X", "f32", 0, "m", "Posizione X"), ("Y", "f32", 0, "m", "Posizione Y"), ("Fork", "f32", 0, "%", "Forche")]),
    "VLM": ("Magazzino verticale a vassoi", [
        ("Moving", "bool", 0, "", "In movimento"), ("Fault", "bool", 0, "", "Guasto"), ("Ready", "bool", 0, "", "Vassoio in baia"),
        ("Cmd_Reset", "bool", 1, "", "Reset guasto"), ("Tray", "u16", 0, "", "Vassoio in baia"),
        ("Cmd_Call", "u16", 1, "", "Richiama vassoio"), ("TrayPct", "f32", 0, "%", "Posizione vassoio %")]),
    "Sensore": ("Sensore digitale (fotocellula)", [
        ("Blocked", "bool", 0, "", "Interrotta"), ("Fault", "bool", 0, "", "Guasto"), ("Dirty", "bool", 0, "", "Sporca"),
        ("Count", "u32", 0, "", "Interruzioni")]),
    "Scanner": ("Lettore barcode", [
        ("ReadOk", "bool", 0, "", "Lettura OK"), ("NoRead", "bool", 0, "", "Mancata lettura"), ("Fault", "bool", 0, "", "Guasto"),
        ("LastCode", "string", 0, "", "Ultimo codice"), ("Reads", "u32", 0, "", "Letture"), ("NoReads", "u32", 0, "", "Mancate letture")]),
    "Bilancia": ("Bilancia / volumetro", [
        ("Stable", "bool", 0, "", "Peso stabile"), ("Fault", "bool", 0, "", "Guasto"), ("Cmd_Zero", "bool", 1, "", "Azzera"),
        ("Weight", "f32", 0, "kg", "Peso"), ("Length", "f32", 0, "mm", "Lunghezza"), ("Width", "f32", 0, "mm", "Larghezza"),
        ("Height", "f32", 0, "mm", "Altezza")]),
    "Segnalatore": ("Colonna luminosa", [
        ("Red", "bool", 1, "", "Rosso"), ("Yellow", "bool", 1, "", "Giallo"), ("Green", "bool", 1, "", "Verde"),
        ("Blue", "bool", 1, "", "Blu"), ("Buzzer", "bool", 1, "", "Sirena"), ("Blink", "bool", 1, "", "Lampeggio")]),
    "Emergenza": ("Pulsante di emergenza", [
        ("Pressed", "bool", 0, "", "Premuto"), ("Cmd_Reset", "bool", 1, "", "Ripristino")]),
    "Postazione": ("Postazione operatore / picking", [
        ("Operator", "bool", 0, "", "Operatore presente"), ("Lamp", "bool", 0, "", "Lampada pick"), ("Fault", "bool", 0, "", "Guasto"),
        ("Cmd_Confirm", "bool", 1, "", "Conferma"), ("Order", "string", 0, "", "Ordine"), ("ToPick", "u16", 0, "pz", "Da prelevare"),
        ("Picked", "u16", 0, "pz", "Prelevati")]),
    "UdC": ("Unità di carico", [
        ("Present", "bool", 0, "", "Presente"), ("Id", "string", 0, "", "Identificativo"), ("Destination", "u16", 0, "", "Destinazione"),
        ("Weight", "f32", 0, "kg", "Peso")]),
    "Macchina": ("Macchina automatica", [
        ("Run", "bool", 0, "", "In marcia"), ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"),
        ("Ready", "bool", 0, "", "Pronta"), ("Occupied", "bool", 0, "", "Prodotto presente"),
        ("Cmd_Start", "bool", 1, "", "Comando marcia"), ("Cmd_Stop", "bool", 1, "", "Comando arresto"),
        ("Cmd_Reset", "bool", 1, "", "Reset guasto"), ("AlarmCode", "u16", 0, "", "Codice allarme"),
        ("Count", "u32", 0, "pz", "Pezzi prodotti"), ("Speed", "f32", 0, "pz/h", "Cadenza"), ("Angle", "f32", 0, "°", "Angolo asse"),
        ("Progress", "f32", 0, "%", "Avanzamento ciclo")]),
    "Motore": ("Motore / pompa / ventilatore", [
        ("Run", "bool", 0, "", "In marcia"), ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"),
        ("Cmd_Start", "bool", 1, "", "Comando marcia"), ("Cmd_Stop", "bool", 1, "", "Comando arresto"),
        ("Cmd_Reset", "bool", 1, "", "Reset guasto"), ("Speed", "f32", 0, "%", "Velocità"), ("Current", "f32", 0, "A", "Corrente"),
        ("Hours", "u32", 0, "h", "Ore di funzionamento")]),
    "Valvola": ("Valvola on/off", [
        ("Open", "bool", 0, "", "Aperta"), ("Closed", "bool", 0, "", "Chiusa"), ("Fault", "bool", 0, "", "Guasto"),
        ("Auto", "bool", 0, "", "Automatico"), ("Cmd_Open", "bool", 1, "", "Comando apertura"),
        ("Cmd_Close", "bool", 1, "", "Comando chiusura")]),
    "ValvolaReg": ("Valvola di regolazione", [
        ("Fault", "bool", 0, "", "Guasto"), ("Auto", "bool", 0, "", "Automatico"), ("Position", "f32", 0, "%", "Apertura"),
        ("Setpoint", "f32", 1, "%", "Setpoint apertura")]),
    "Serbatoio": ("Serbatoio / silo / tramoggia", [
        ("HighLevel", "bool", 0, "", "Livello alto"), ("LowLevel", "bool", 0, "", "Livello basso"),
        ("Fault", "bool", 0, "", "Guasto strumento"), ("Level", "f32", 0, "%", "Livello"),
        ("Volume", "f32", 0, "m³", "Volume"), ("Temp", "f32", 0, "°C", "Temperatura")]),
    "Strumento": ("Trasmettitore analogico", [
        ("HH", "bool", 0, "", "Soglia altissima"), ("H", "bool", 0, "", "Soglia alta"), ("L", "bool", 0, "", "Soglia bassa"),
        ("LL", "bool", 0, "", "Soglia bassissima"), ("Fault", "bool", 0, "", "Guasto"), ("Value", "f32", 0, "", "Valore")]),
    "Scambiatore": ("Scambiatore di calore", [
        ("Fault", "bool", 0, "", "Guasto"), ("TempIn", "f32", 0, "°C", "Temperatura ingresso"),
        ("TempOut", "f32", 0, "°C", "Temperatura uscita"), ("Flow", "f32", 0, "m³/h", "Portata")]),
    "Filtro": ("Filtro", [
        ("Clogged", "bool", 0, "", "Intasato"), ("DeltaP", "f32", 0, "bar", "Pressione differenziale")]),
    "Tubazione": ("Tubazione", [("Flowing", "bool", 0, "", "Flusso presente"), ("Pressure", "f32", 0, "bar", "Pressione")]),
}

SIZES = {"bool": 0, "u8": 1, "i16": 2, "u16": 2, "i32": 4, "u32": 4, "f32": 4, "string": 22}


def build_udt(name, desc, members):
    """Layout S7: bool impacchettati nei primi byte, poi word/dword allineati."""
    out, bit, byte = [], 0, 0
    bools = [m for m in members if m[1] == "bool"]
    others = [m for m in members if m[1] != "bool"]
    for (mn, t, w, u, d) in bools:
        out.append(dict(name=mn, type=t, address=f"DB{{DB}}.DBX{{Base+{byte}}}.{bit}", **({"writable": True} if w else {}),
                        **({"unit": u} if u else {}), description=d))
        bit += 1
        if bit == 8:
            bit, byte = 0, byte + 1
    byte = byte + (1 if bit else 0)
    byte += byte % 2
    for (mn, t, w, u, d) in others:
        size = SIZES[t]
        kind = {"u8": "DBB", "i16": "DBW", "u16": "DBW", "i32": "DBD", "u32": "DBD", "f32": "DBD", "string": "DBB"}[t]
        m = dict(name=mn, type=t, address=f"DB{{DB}}.{kind}{{Base+{byte}}}", **({"writable": True} if w else {}),
                 **({"unit": u} if u else {}), description=d)
        if t == "string":
            m["len"] = 20
        if t == "f32" and not w:
            m["historize"] = True
        out.append(m)
        byte += size
        byte += byte % 2
    return {"name": name, "description": desc + f" – layout S7 di {byte} byte a partire da Base",
            "params": [{"name": "DB", "default": 100, "description": "Numero DB"},
                       {"name": "Base", "default": 0, "description": "Offset di partenza nel DB (byte)"}],
            "members": out}


# ------------------------------------------------------------------ binding predefiniti per famiglia

def B(expr):
    return {"kind": "expr", "expr": expr}


def T(member, **tr):
    b = {"kind": "tag", "tag": "{device}." + member}
    if tr:
        b["transform"] = tr
    return b


STATE_RUN = B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Run') ? '#22c55e' : '#64748b'")
LABEL = B("param('device') || ''")


def common(fam_members):
    names = {m[0] for m in fam_members}
    b = {"elements.label.text": LABEL}
    if "Fault" in names:
        b["elements.fault.visible"] = T("Fault")
    for occ in ("Occupied", "Loaded", "Present"):
        if occ in names:
            b["elements.load.visible"] = T(occ)
            break
    if "Run" in names:
        b["elements.body.fill"] = STATE_RUN
        b["elements.flow.flow"] = T("Run")
        b["elements.flow.visible"] = T("Run")
    return b


# ------------------------------------------------------------------ simboli
SYMBOLS = []  # (id, titolo, categoria, famiglia, viewBox, corpo, binding extra)


def sym(id, title, cat, fam, vb, body, extra=None):
    SYMBOLS.append((id, title, cat, fam, vb, body, extra or {}))


C_CONV = "Intralogistica/Nastri e rulliere"
C_CURVE = "Intralogistica/Curve e confluenze"
C_DIV = "Intralogistica/Deviatori e trasferimenti"
C_SORT = "Intralogistica/Sorter"
C_LIFT = "Intralogistica/Elevatori e lift"
C_DOOR = "Intralogistica/Porte"
C_ASRS = "Intralogistica/Magazzini automatici"
C_VEH = "Intralogistica/Veicoli"
C_DEV = "Intralogistica/Dispositivi di campo"
C_LOAD = "Intralogistica/Unità di carico"
C_MEOL = "Macchine/Fine linea"
C_MPACK = "Macchine/Confezionamento"
C_MROB = "Macchine/Robotica"
C_PMOT = "Processo/Pompe e motori"
C_PVAL = "Processo/Valvole"
C_PTANK = "Processo/Serbatoi"
C_PEQ = "Processo/Apparecchiature"
C_PINS = "Processo/Strumentazione"
C_PIPE = "Processo/Tubazioni"

# --- Nastri e rulliere -------------------------------------------------------
sym("conv_belt", "Nastro a tappeto", C_CONV, "Trasporto", "0 0 200 60",
    E("rect", x=0, y=0, width=200, height=60, rx=5, fill=FR)
    + E("rect", "body", "Colore di stato (marcia/fermo/guasto)", x=5, y=7, width=190, height=46, rx=3, fill=BODY)
    + "".join(E("line", x1=x, y1=9, x2=x, y2=51, stroke="#ffffff", stroke_opacity=0.08) for x in frange(14, 190, 14))
    + flow_line("M10 30 H190") + arrow(100, 30) + carton(72, 13, 56, 34) + fault_box(200, 60)
    + text(100, -9, "CV"))

sym("conv_roller", "Rulliera motorizzata", C_CONV, "Trasporto", "0 0 200 60",
    E("rect", "body", "Colore di stato (sponde)", x=0, y=0, width=200, height=60, rx=4, fill=BODY)
    + E("rect", x=0, y=8, width=200, height=44, fill=BELT) + rollers(8, 192, 8, 52, 12, 6, "rollers")
    + flow_line("M8 30 H192", op=0.35) + arrow(100, 30) + carton(70, 12, 60, 36) + fault_box(200, 60)
    + text(100, -9, "RC"))

sym("conv_gravity", "Rulliera a gravità", C_CONV, "Trasporto", "0 0 200 60",
    E("rect", "body", "Colore sponde", x=0, y=0, width=200, height=60, rx=4, fill=FR)
    + E("rect", x=0, y=8, width=200, height=44, fill=BELT) + rollers(8, 192, 8, 52, 10, 4)
    + E("path", d="M30 22 L50 30 L30 38 M150 22 L170 30 L150 38", stroke=ACC, stroke_width=2.5, fill="none", opacity=0.7)
    + carton(70, 12, 60, 36) + text(100, -9, "RG"))

acc = (E("rect", "body", "Colore di stato (sponde)", x=0, y=0, width=240, height=60, rx=4, fill=BODY)
       + E("rect", x=0, y=8, width=240, height=44, fill=BELT) + rollers(8, 232, 8, 52, 12, 6))
for i in range(4):
    x = i * 60
    acc += E("rect", f"zone{i + 1}", f"Zona {i + 1} occupata (visibile)", True, x=x + 6, y=12, width=48, height=36,
             fill=LOAD, stroke=LOADD, stroke_width=2, rx=2)
    acc += lamp(x + 56, 56, 3.5, f"pe{i + 1}", f"Fotocellula zona {i + 1}")
    if i:
        acc += E("line", x1=x, y1=4, x2=x, y2=56, stroke=ACC, stroke_width=1.5, stroke_dasharray="4 3")
acc += flow_line("M8 30 H232", op=0.3) + fault_box(240, 60) + text(120, -9, "ACC")
sym("conv_accum", "Rulliera ad accumulo (4 zone)", C_CONV, "Accumulo", "0 0 240 60", acc,
    {**{f"elements.zone{i}.visible": T(f"Zone{i}") for i in range(1, 5)},
     **{f"elements.pe{i}.fill": T(f"Zone{i}", kind="map", rules=[{"in": "true", "out": WARN}], default="#334155") for i in range(1, 5)}})

sym("conv_chain", "Catenaria per pallet", C_CONV, "Trasporto", "0 0 200 120",
    E("rect", "body", "Colore di stato (telaio)", x=0, y=0, width=200, height=120, rx=4, fill=BODY)
    + E("rect", x=6, y=6, width=188, height=108, fill=BELT)
    + "".join(E("rect", x=4, y=y, width=192, height=12, fill=DARK, rx=3) for y in (14, 54, 94))
    + G("flow", "Catene in movimento (in marcia)",
        "".join(E("line", x1=6, y1=y + 6, x2=194, y2=y + 6, stroke="#94a3b8", stroke_width=6, stroke_dasharray="6 6") for y in (14, 54, 94)),
        style="--dl:12")
    + pallet(40, 10, 120, 100) + fault_box(200, 120) + text(100, -9, "CT"))

sym("conv_telescopic", "Nastro telescopico (carico camion)", C_CONV, "Trasporto", "0 0 260 70",
    E("rect", "body", "Colore di stato (base)", x=0, y=4, width=110, height=62, rx=4, fill=BODY)
    + G("boom", "Sezioni estensibili (tx = estensione)",
        E("rect", x=100, y=10, width=80, height=50, fill=FR, rx=3) + E("rect", x=170, y=14, width=80, height=42, fill=FRD, rx=3)
        + E("rect", x=240, y=18, width=14, height=34, fill=ACC, rx=2))
    + flow_line("M10 35 H250", op=0.35) + arrow(55, 35) + carton(30, 18, 50, 34) + fault_box(110, 70)
    + text(55, -6, "TEL"))

# --- Curve e confluenze --------------------------------------------------------
cx, cy, r0, r1 = 0, 120, 30, 112
outer = arc_pt(cx, cy, r1, -90), arc_pt(cx, cy, r1, 0)
inner_ = arc_pt(cx, cy, r0, 0), arc_pt(cx, cy, r0, -90)
ring = f"M{outer[0][0]:.1f} {outer[0][1]:.1f} A{r1} {r1} 0 0 1 {outer[1][0]:.1f} {outer[1][1]:.1f} L{inner_[0][0]:.1f} {inner_[0][1]:.1f} A{r0} {r0} 0 0 0 {inner_[1][0]:.1f} {inner_[1][1]:.1f} Z"
rads = ""
for deg in range(-86, -3, 8):
    a, b = arc_pt(cx, cy, r0 + 4, deg), arc_pt(cx, cy, r1 - 4, deg)
    rads += E("line", x1=f"{a[0]:.1f}", y1=f"{a[1]:.1f}", x2=f"{b[0]:.1f}", y2=f"{b[1]:.1f}", stroke=ROL, stroke_width=5, stroke_linecap="round", opacity=0.9)
mid0, mid1 = arc_pt(cx, cy, 71, -88), arc_pt(cx, cy, 71, -2)
sym("conv_curve90", "Curva 90° a rulli", C_CURVE, "Trasporto", "0 0 120 120",
    E("path", "body", "Colore di stato", d=ring, fill=BODY, stroke=FR, stroke_width=5)
    + G(None, None, rads) + flow_line(f"M{mid0[0]:.1f} {mid0[1]:.1f} A71 71 0 0 1 {mid1[0]:.1f} {mid1[1]:.1f}", op=0.4)
    + E("path", "fault", "Segnalazione guasto (visibile)", True, d=ring, fill="none", stroke=FAULT, stroke_width=3, stroke_dasharray="8 4")
    + text(20, 132, "CU", anchor="start"))

sym("conv_merge", "Confluenza 45°", C_CURVE, "Trasporto", "0 0 200 110",
    E("polygon", "branch", "Colore ramo laterale", points="0,0 44,0 112,60 68,60", fill=BODY, stroke=FR, stroke_width=4)
    + E("rect", "body", "Colore di stato (linea principale)", x=0, y=60, width=200, height=46, rx=3, fill=BODY, stroke=FR, stroke_width=4)
    + flow_line("M8 83 H192", op=0.4) + flow_line("M22 4 L92 60", op=0.4, id="flow2")
    + arrow(150, 83) + carton(130, 66, 40, 34) + fault_box(200, 110) + text(150, 48, "MG"),
    {"elements.branch.fill": STATE_RUN, "elements.flow2.flow": T("Run"), "elements.flow2.visible": T("Run")})

pts = []
for i in range(0, 361 * 2 + 1, 10):
    r = 46 - 18 * i / 720
    x, y = arc_pt(60, 60, r, i - 90)
    pts.append(f"{x:.1f},{y:.1f}")
sym("conv_spiral", "Spirale (vista in pianta)", C_CURVE, "Trasporto", "0 0 120 120",
    E("circle", cx=60, cy=60, r=58, fill=FR)
    + E("circle", "body", "Colore di stato", cx=60, cy=60, r=52, fill=BODY)
    + E("circle", cx=60, cy=60, r=22, fill=DARK, stroke=FR, stroke_width=3)
    + E("polyline", "flow", "Flusso animato (in marcia)", points=" ".join(pts), fill="none", stroke="#ffffff",
        stroke_width=4, opacity=0.45, stroke_dasharray="10 8", style="--dl:18")
    + E("circle", "fault", "Segnalazione guasto (visibile)", True, cx=60, cy=60, r=57, fill="none", stroke=FAULT, stroke_width=3, stroke_dasharray="8 4")
    + text(60, 60, "SP", fill="#e2e8f0"))

# --- Deviatori e trasferimenti --------------------------------------------------
sym("div_swing", "Deviatore a paletta", C_DIV, "Deviatore", "0 0 170 110",
    E("polygon", points="100,30 150,0 170,22 128,52", fill=FRD, stroke=FR, stroke_width=3)
    + E("rect", "body", "Colore di stato", x=0, y=30, width=170, height=48, rx=3, fill=BODY, stroke=FR, stroke_width=3)
    + flow_line("M8 54 H162", op=0.35) + carton(14, 38, 40, 32)
    + G("flap", "Paletta (rotate = angolo)", E("rect", x=50, y=31, width=70, height=7, rx=3, fill=ACC, stroke=DARK, stroke_width=1.5)
        + E("circle", cx=52, cy=34, r=5, fill=DARK), data_pivot="52 34")
    + fault_box(170, 110) + text(85, 96, "DV"),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Position') > 0 ? '#f59e0b' : '#64748b'"),
     "elements.flap.rotate": T("Position", kind="map", rules=[{"in": "0", "out": "0"}, {"in": ">0", "out": "-28"}], default="0")})

sym("div_pusher", "Deviatore a spintore", C_DIV, "Deviatore", "0 0 160 110",
    E("rect", x=55, y=0, width=50, height=22, rx=3, fill=FRD, stroke=FR, stroke_width=2)
    + E("rect", x=55, y=76, width=50, height=34, fill="#1e293b", stroke=FR, stroke_width=2, stroke_dasharray="5 3")
    + E("rect", "body", "Colore di stato", x=0, y=26, width=160, height=48, rx=3, fill=BODY, stroke=FR, stroke_width=3)
    + flow_line("M8 50 H152", op=0.35) + carton(14, 34, 40, 32)
    + G("pusher", "Spintore (ty = corsa)", E("rect", x=77, y=8, width=6, height=20, fill=ROL) + E("rect", x=58, y=26, width=44, height=7, rx=2, fill=ACC, stroke=DARK)),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Position') > 0 ? '#f59e0b' : '#64748b'"),
     "elements.pusher.ty": T("Position", kind="map", rules=[{"in": ">0", "out": "42"}], default="0")})

wheels = ""
for x in range(20, 101, 20):
    for y in range(20, 101, 20):
        wheels += E("ellipse", cx=x, cy=y, rx=6, ry=3, fill="#e2e8f0", opacity=0.85)
sym("div_popup", "Deviatore a ruote pop-up", C_DIV, "Deviatore", "0 0 120 120",
    E("rect", "body", "Colore di stato", x=0, y=0, width=120, height=120, rx=4, fill=BODY, stroke=FR, stroke_width=3)
    + E("rect", x=6, y=6, width=108, height=108, fill=BELT)
    + G("wheels", "Ruote orientabili (rotate = direzione)", wheels, data_pivot="60 60")
    + carton(30, 34, 60, 52) + fault_box(120, 120) + text(60, -9, "PU"),
    {"elements.body.fill": STATE_RUN.copy() if False else B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Run') ? '#22c55e' : '#64748b'"),
     "elements.wheels.rotate": T("Position", kind="map", rules=[{"in": "1", "out": "-45"}, {"in": "2", "out": "45"}], default="0")})

sym("div_transfer", "Trasferitore a catene 90°", C_DIV, "Deviatore", "0 0 120 120",
    E("rect", "body", "Colore di stato", x=0, y=0, width=120, height=120, rx=4, fill=BODY, stroke=FR, stroke_width=3)
    + E("rect", x=6, y=6, width=108, height=108, fill=BELT) + rollers(14, 106, 8, 112, 12, 6)
    + G("chains", "Catene trasversali (fill = alzate)",
        "".join(E("rect", x=x, y=2, width=8, height=116, rx=2, fill="#334155") for x in (26, 56, 86)))
    + flow_line("M8 60 H112", op=0.35) + flow_line("M60 8 V112", op=0.35, id="flow2")
    + carton(30, 34, 60, 52) + fault_box(120, 120) + text(60, -9, "TR"),
    {"elements.chains.fill": T("Position", kind="map", rules=[{"in": ">0", "out": ACC}], default="#334155"),
     "elements.flow2.visible": B("tag('{device}.Position') > 0"), "elements.flow2.flow": B("tag('{device}.Position') > 0")})

sym("div_turntable", "Tavola rotante", C_DIV, "Deviatore", "0 0 120 120",
    E("rect", x=0, y=0, width=120, height=120, rx=6, fill=FRD)
    + E("circle", "body", "Colore di stato", cx=60, cy=60, r=56, fill=BODY, stroke=FR, stroke_width=3)
    + G("table", "Piano rotante (rotate = angolo)",
        E("rect", x=14, y=34, width=92, height=52, rx=3, fill=BELT) + rollers(20, 100, 34, 86, 10, 5)
        + carton(30, 40, 60, 40) + arrow(60, 60), data_pivot="60 60")
    + fault_box(120, 120) + text(60, -9, "TT"),
    {"elements.table.rotate": T("Angle"), "elements.load.visible": T("Occupied")})

# --- Sorter ------------------------------------------------------------------
sym("sort_crossbelt", "Sorter cross-belt", C_SORT, "Sorter", "0 0 260 90",
    E("rect", "body", "Colore di stato (binario)", x=0, y=8, width=260, height=74, rx=6, fill=BODY)
    + E("rect", x=4, y=14, width=252, height=62, fill=DARK)
    + E("line", "carriers", "Carrelli in movimento (flow)", x1=0, y1=45, x2=260, y2=45, stroke=FR, stroke_width=56,
        stroke_dasharray="44 8", style="--dl:52")
    + "".join(E("line", x1=x, y1=20, x2=x, y2=70, stroke="#ffffff", stroke_opacity=0.15, stroke_width=2) for x in range(22, 260, 52))
    + carton(110, 26, 36, 38) + fault_box(260, 90) + text(130, -2, "XB"),
    {"elements.carriers.flow": T("Run")})

sym("sort_shoe", "Sorter a pattini", C_SORT, "Sorter", "0 0 260 90",
    E("rect", "body", "Colore di stato (telaio)", x=0, y=8, width=260, height=74, rx=4, fill=BODY)
    + E("line", "slats", "Stecche in movimento (flow)", x1=0, y1=45, x2=260, y2=45, stroke="#94a3b8", stroke_width=62,
        stroke_dasharray="9 3", style="--dl:12")
    + G("shoes", "Pattini di deviazione (visibile)", "".join(E("rect", x=120 + i * 12, y=18 + i * 9, width=8, height=8, fill=ACC, rx=1) for i in range(6)), hidden=True)
    + carton(60, 26, 44, 38) + fault_box(260, 90) + text(130, -2, "SH"),
    {"elements.slats.flow": T("Run"), "elements.shoes.visible": T("Divert")})

sym("sort_tilttray", "Sorter a vassoi basculanti", C_SORT, "Sorter", "0 0 260 90",
    E("rect", "body", "Colore di stato (binario)", x=0, y=8, width=260, height=74, rx=6, fill=BODY)
    + E("rect", x=4, y=14, width=252, height=62, fill=DARK)
    + E("line", "carriers", "Vassoi in movimento (flow)", x1=0, y1=45, x2=260, y2=45, stroke="#0f766e", stroke_width=50,
        stroke_dasharray="40 12", style="--dl:52")
    + G("tray", "Vassoio in basculamento (rotate)", E("rect", x=110, y=22, width=40, height=46, rx=3, fill="#14b8a6", stroke=DARK)
        + carton(116, 30, 28, 30), data_pivot="130 45")
    + fault_box(260, 90) + text(130, -2, "TS"),
    {"elements.carriers.flow": T("Run"), "elements.tray.rotate": T("Divert", kind="map", rules=[{"in": "true", "out": "25"}], default="0")})

sym("sort_chute", "Scivolo / baia di uscita", C_SORT, "Scivolo", "0 0 80 150",
    E("polygon", "body", "Colore di stato", points="6,0 74,0 80,150 0,150", fill=BODY, stroke=FR, stroke_width=3)
    + "".join(E("line", x1=8, y1=y, x2=72, y2=y, stroke="#ffffff", stroke_opacity=0.12) for y in range(16, 150, 16))
    + E("rect", "level", "Riempimento (ty)", x=6, y=150, width=68, height=0, fill=LOAD, opacity=0.8)
    + E("rect", "full", "Indicatore pieno (visibile)", True, x=4, y=4, width=72, height=142, fill="none", stroke=WARN, stroke_width=4)
    + lamp(64, 14, 7, "lamp", "Lampada di chiamata (fill)")
    + text(40, 160, "CH"),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Enabled') === false ? '#334155' : '#64748b'"),
     "elements.full.visible": T("Full"), "elements.lamp.fill": T("Lamp", kind="map", rules=[{"in": "true", "out": WARN}], default="#334155"),
     "elements.lamp.blink": T("Lamp")})

sym("sort_induct", "Stazione di induzione", C_SORT, "Trasporto", "0 0 160 90",
    E("polygon", "body", "Colore di stato", points="0,10 100,10 160,60 160,86 130,86 70,36 0,36", fill=BODY, stroke=FR, stroke_width=3)
    + flow_line("M6 23 L95 23 L150 70", op=0.4) + carton(20, 12, 34, 22) + fault_box(160, 90) + text(50, 0, "IND"))

# --- Elevatori ----------------------------------------------------------------
lv = (E("rect", x=6, y=0, width=10, height=200, fill=FR) + E("rect", x=84, y=0, width=10, height=200, fill=FR)
      + E("rect", "drive", "Gruppo motore (fill)", x=0, y=0, width=100, height=14, rx=2, fill=FRD)
      + "".join(E("rect", x=-28, y=y, width=30, height=8, fill=FRD) + E("rect", x=98, y=y, width=30, height=8, fill=FRD) + text(-30, y + 4, f"P{i}", id=None, size=9, anchor="end", desc=None)
                for i, y in enumerate((178, 118, 58)))
      + G("carriage", "Navetta (ty = posizione, negativa verso l'alto)",
          E("rect", "body", "Colore di stato (navetta)", x=16, y=168, width=68, height=18, rx=2, fill=BODY, stroke=DARK)
          + carton(28, 144, 44, 24) + E("line", x1=50, y1=14, x2=50, y2=168, stroke=ROL, stroke_width=1, stroke_dasharray="3 3"))
      + fault_box(100, 200) + text(50, -9, "LIFT"))
sym("lift_vertical", "Elevatore verticale (vista laterale)", C_LIFT, "Elevatore", "-34 -18 168 222", lv,
    {"elements.carriage.ty": B("-(tag('{device}.PosPct') || 0) * 1.2"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Moving') ? '#f59e0b' : '#64748b'"),
     "elements.drive.fill": T("Moving", kind="map", rules=[{"in": "true", "out": RUN}], default=FRD)})

sym("lift_platform", "Lift (vista in pianta)", C_LIFT, "Elevatore", "0 0 110 110",
    E("rect", x=0, y=0, width=110, height=110, rx=4, fill=FRD, stroke=FR, stroke_width=3)
    + E("rect", "body", "Colore di stato (piattaforma)", x=10, y=10, width=90, height=90, rx=3, fill=BODY)
    + rollers(18, 92, 14, 96, 10, 5) + carton(25, 30, 60, 50)
    + E("path", "up", "Freccia salita (visibile)", True, d="M55 22 L75 44 L35 44 Z", fill=RUN)
    + E("path", "down", "Freccia discesa (visibile)", True, d="M55 88 L75 66 L35 66 Z", fill=ACC)
    + text(55, 55, "", id="level", desc="Piano attuale (text)", size=18, fill="#ffffff", weight=700)
    + fault_box(110, 110) + text(55, -9, "LP"),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Moving') ? '#f59e0b' : '#64748b'"),
     "elements.level.text": B("'P' + (tag('{device}.Level') ?? '-')"),
     "elements.up.visible": B("tag('{device}.Moving') && tag('{device}.Target') > tag('{device}.Level')"),
     "elements.down.visible": B("tag('{device}.Moving') && tag('{device}.Target') < tag('{device}.Level')")})

# --- Porte -------------------------------------------------------------------
sym("door_rapid", "Porta rapida avvolgibile", C_DOOR, "Porta", "0 0 140 150",
    E("defs", inner=E("clipPath", "opening", inner=E("rect", x=12, y=20, width=116, height=130)))
    + E("rect", x=0, y=0, width=12, height=150, fill=FR) + E("rect", x=128, y=0, width=12, height=150, fill=FR)
    + E("rect", "body", "Cassonetto (colore di stato)", x=0, y=0, width=140, height=20, rx=3, fill=BODY)
    + E("rect", x=12, y=20, width=116, height=130, fill=DARK)
    + G(None, None, G("curtain", "Telo (ty = apertura, negativa)", E("rect", x=12, y=20, width=116, height=130, fill="#f97316")
        + E("rect", x=20, y=60, width=100, height=26, fill="#bae6fd", opacity=0.7)
        + "".join(E("line", x1=12, y1=y, x2=128, y2=y, stroke="#c2410c", stroke_width=1) for y in range(30, 150, 12))),
        clip_path="url(#opening)")
    + lamp(126, 8, 5, "lamp", "Lampeggiante (blink)") + fault_box(140, 150) + text(70, 162, "PR"),
    {"elements.curtain.ty": B("-(tag('{device}.OpenPct') || 0) * 1.25"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Open') ? '#22c55e' : tag('{device}.Moving') ? '#f59e0b' : '#64748b'"),
     "elements.lamp.fill": T("Moving", kind="map", rules=[{"in": "true", "out": WARN}], default="#334155"),
     "elements.lamp.blink": T("Moving")})

sym("door_fire", "Porta tagliafuoco scorrevole (pianta)", C_DOOR, "Porta", "0 0 220 60",
    E("rect", x=0, y=17, width=60, height=26, fill=FRD) + E("rect", x=160, y=17, width=60, height=26, fill=FRD)
    + G(None, None, "".join(E("line", x1=x, y1=17, x2=x - 12, y2=43, stroke="#94a3b8", stroke_width=1.2) for x in list(range(12, 61, 8)) + list(range(172, 221, 8))))
    + E("line", x1=60, y1=30, x2=160, y2=30, stroke=FR, stroke_width=1, stroke_dasharray="4 4")
    + E("rect", "panel", "Anta (tx = apertura, negativa)", x=60, y=22, width=100, height=16, fill="#dc2626", stroke=DARK, stroke_width=2)
    + E("rect", "body", "Stato (indicatore)", x=104, y=46, width=12, height=10, rx=2, fill=BODY)
    + fault_box(220, 60) + text(110, 6, "PTF"),
    {"elements.panel.tx": B("-(tag('{device}.OpenPct') || 0) * 0.95"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Open') ? '#22c55e' : tag('{device}.Moving') ? '#f59e0b' : '#64748b'")})

sym("door_dock", "Baia di carico con portone sezionale", C_DOOR, "Porta", "0 0 150 160",
    E("defs", inner=E("clipPath", "dockopen", inner=E("rect", x=10, y=10, width=130, height=130)))
    + E("rect", x=0, y=0, width=150, height=150, fill=FRD, rx=3) + E("rect", x=10, y=10, width=130, height=130, fill=DARK)
    + G(None, None, G("panel", "Portone (ty = apertura, negativa)", "".join(
        E("rect", x=10, y=10 + i * 26, width=130, height=26, fill="#94a3b8", stroke=FR, stroke_width=2) for i in range(5))
        + E("rect", x=30, y=40, width=90, height=12, fill="#bae6fd", opacity=0.6)), clip_path="url(#dockopen)")
    + E("rect", "body", "Stato (soglia)", x=0, y=150, width=150, height=10, fill=BODY)
    + lamp(-6, 30, 6, "lamp_red", "Semaforo rosso (fill)", "#7f1d1d") + lamp(-6, 46, 6, "lamp_green", "Semaforo verde (fill)", "#14532d")
    + fault_box(150, 160) + text(75, -9, "BAIA"),
    {"elements.panel.ty": B("-(tag('{device}.OpenPct') || 0) * 1.3"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Open') ? '#22c55e' : '#64748b'"),
     "elements.lamp_green.fill": T("Open", kind="map", rules=[{"in": "true", "out": RUN}], default="#14532d"),
     "elements.lamp_red.fill": T("Open", kind="map", rules=[{"in": "false", "out": FAULT}], default="#7f1d1d")})

# --- Magazzini automatici -----------------------------------------------------
rack = E("rect", x=0, y=150, width=200, height=10, fill=FRD)
for i in range(5):
    rack += E("rect", x=i * 49, y=0, width=6, height=152, fill="#2563eb")
for j in range(3):
    rack += E("rect", x=0, y=48 + j * 50, width=202, height=5, fill="#f97316")
n = 1
for j in range(3):
    for i in range(4):
        x, y = 10 + i * 49, 14 + (2 - j) * 50
        rack += pallet(x, y + 6, 36, 26, id=f"c{n}", hidden=True)
        n += 1
rack += E("rect", "body", "Stato (targa)", x=72, y=152, width=56, height=8, fill=BODY) + text(100, -9, "SCAFF")
sym("asrs_rack", "Scaffalatura porta-pallet (fronte)", C_ASRS, "Scaffale", "0 0 202 162", rack,
    {**{f"elements.c{i}.visible": T(f"C{i}") for i in range(1, 13)},
     "elements.body.fill": T("FillPct", kind="map", rules=[{"in": ">90", "out": FAULT}, {"in": ">70", "out": WARN}], default=RUN)})

sym("asrs_crane", "Traslo (fronte)", C_ASRS, "Traslo", "0 0 70 200",
    E("rect", x=-200, y=194, width=470, height=6, fill=FRD) + E("rect", x=-200, y=0, width=470, height=4, fill=FRD)
    + G("mast", "Colonna (tx = posizione corsia)",
        E("rect", "body", "Colore di stato (colonna)", x=28, y=4, width=14, height=190, fill=BODY)
        + E("rect", x=6, y=180, width=58, height=14, rx=2, fill=FRD) + E("rect", x=20, y=0, width=30, height=8, fill=FRD)
        + G("carriage", "Culla (ty = quota, negativa)", E("rect", x=10, y=156, width=50, height=20, rx=2, fill=ACC, stroke=DARK)
            + G("forks", "Forche (tx = estrazione)", E("rect", x=14, y=170, width=42, height=4, fill=DARK))
            + pallet(14, 140, 42, 16))
        + fault_box(70, 200)) + text(35, -9, "TR"),
    {"elements.mast.tx": B("(tag('{device}.PctX') || 0) * 2"), "elements.carriage.ty": B("-(tag('{device}.PctY') || 0) * 1.4"),
     "elements.forks.tx": B("(tag('{device}.Fork') || 0) * 0.3"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Moving') ? '#22c55e' : '#64748b'")})

rp = E("rect", x=0, y=0, width=260, height=110, fill=DARK)
for row in (0, 70):
    rp += E("rect", x=0, y=row, width=260, height=40, fill=FRD)
    for x in range(0, 260, 26):
        rp += E("rect", x=x + 2, y=row + 3, width=22, height=34, fill="none", stroke="#2563eb", stroke_width=1.5)
rp += E("line", x1=0, y1=55, x2=260, y2=55, stroke=FR, stroke_width=2, stroke_dasharray="6 4")
rp += G("crane", "Traslo nella corsia (tx = posizione)", E("rect", "body", "Colore di stato (traslo)", x=4, y=44, width=30, height=22, rx=3, fill=BODY, stroke=DARK)
        + E("rect", "load", "Carico a bordo (visibile)", True, x=10, y=48, width=18, height=14, fill=LOAD))
rp += text(130, -9, "CORSIA")
sym("asrs_aisle", "Corsia magazzino con traslo (pianta)", C_ASRS, "Traslo", "0 0 260 110", rp,
    {"elements.crane.tx": B("(tag('{device}.PctX') || 0) * 2.2"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Moving') ? '#22c55e' : '#64748b'")})

sym("asrs_shuttle", "Shuttle (pianta)", C_ASRS, "Veicolo", "0 0 110 64",
    E("rect", "body", "Colore di stato", x=4, y=4, width=102, height=56, rx=6, fill=BODY, stroke=DARK, stroke_width=2)
    + "".join(E("rect", x=x, y=y, width=14, height=8, rx=2, fill=DARK) for x in (10, 86) for y in (0, 56))
    + E("rect", "deck", "Piano di carico", x=22, y=12, width=66, height=40, rx=3, fill=FR)
    + carton(28, 16, 54, 32) + lamp(98, 14, 4, "led", "LED di stato (fill)") + E("rect", "battery", "Barra batteria (fill)", x=10, y=26, width=6, height=12, fill=RUN)
    + fault_box(110, 64) + text(55, -9, "SH"),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Moving') ? '#22c55e' : '#64748b'"),
     "elements.led.blink": T("Moving"), "elements.led.fill": T("Moving", kind="map", rules=[{"in": "true", "out": RUN}], default="#334155"),
     "elements.battery.fill": T("Battery", kind="map", rules=[{"in": "<20", "out": FAULT}, {"in": "<50", "out": WARN}], default=RUN)})

sym("asrs_vlm", "Magazzino verticale a vassoi (VLM)", C_ASRS, "VLM", "0 0 110 200",
    E("defs", inner=E("clipPath", "vlmbay", inner=E("rect", x=14, y=110, width=82, height=44)))
    + E("rect", "body", "Colore di stato (armadio)", x=0, y=0, width=110, height=200, rx=4, fill=BODY, stroke=DARK, stroke_width=2)
    + E("rect", x=8, y=8, width=94, height=96, fill=FRD)
    + "".join(E("rect", x=12, y=y, width=86, height=5, fill="#94a3b8", opacity=0.6) for y in range(14, 100, 9))
    + E("rect", x=14, y=110, width=82, height=44, fill=DARK)
    + G(None, None, G("tray", "Vassoio (ty = posizione)", E("rect", x=16, y=132, width=78, height=8, fill=ACC)
        + carton(22, 116, 26, 16, id="trayload", hidden=False, desc="Merce sul vassoio") + carton(54, 120, 30, 12, id=None, hidden=False, desc=None)),
        clip_path="url(#vlmbay)")
    + E("rect", x=30, y=164, width=50, height=22, rx=2, fill=DARK)
    + text(55, 175, "--", id="display", desc="Display vassoio (text)", size=11, fill="#22d3ee")
    + fault_box(110, 200) + text(55, -9, "VLM"),
    {"elements.tray.ty": B("(100 - (tag('{device}.TrayPct') ?? 100)) * -0.9"),
     "elements.display.text": B("'V' + (tag('{device}.Tray') ?? '--')"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Moving') ? '#f59e0b' : '#64748b'")})

# --- Veicoli -----------------------------------------------------------------
sym("veh_agv", "AGV / AMR", C_VEH, "Veicolo", "0 0 110 76",
    E("rect", "body", "Colore di stato", x=4, y=4, width=102, height=68, rx=12, fill=BODY, stroke=DARK, stroke_width=2)
    + E("rect", x=4, y=4, width=102, height=68, rx=12, fill="none", stroke=ACC, stroke_width=3, stroke_dasharray="10 6")
    + "".join(E("rect", x=x, y=y, width=16, height=8, rx=3, fill=DARK) for x in (16, 78) for y in (0, 68))
    + E("circle", "lidar", "Scanner di sicurezza (blink in movimento)", cx=96, cy=38, r=6, fill=GLASS, stroke=DARK)
    + carton(24, 16, 56, 44) + lamp(14, 38, 4, "led", "LED di stato (fill)")
    + E("rect", "battery", "Barra batteria (fill)", x=86, y=56, width=12, height=6, fill=RUN)
    + fault_box(110, 76) + text(55, -9, "AGV"),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Charging') ? '#38bdf8' : tag('{device}.Moving') ? '#22c55e' : '#64748b'"),
     "elements.lidar.blink": T("Moving"), "elements.led.fill": T("State", kind="map", rules=[{"in": "0", "out": "#334155"}, {"in": "1", "out": RUN}, {"in": "2", "out": WARN}], default=FAULT),
     "elements.battery.fill": T("Battery", kind="map", rules=[{"in": "<20", "out": FAULT}, {"in": "<50", "out": WARN}], default=RUN)})

sym("veh_forklift", "Carrello elevatore", C_VEH, "Veicolo", "0 0 140 76",
    E("rect", "body", "Colore di stato", x=30, y=6, width=80, height=64, rx=8, fill="#eab308", stroke=DARK, stroke_width=2)
    + E("rect", x=44, y=18, width=34, height=40, rx=4, fill=DARK, opacity=0.7) + E("circle", cx=60, cy=38, r=8, fill="#475569")
    + E("rect", x=110, y=10, width=10, height=56, fill=FRD)
    + G("forks", "Forche (tx = sfilo)", E("rect", x=118, y=18, width=22, height=6, fill="#94a3b8") + E("rect", x=118, y=52, width=22, height=6, fill="#94a3b8")
        + pallet(118, 14, 22, 48))
    + "".join(E("rect", x=x, y=y, width=16, height=8, rx=3, fill=DARK) for x in (36, 90) for y in (2, 66))
    + lamp(36, 38, 4, "led", "Lampeggiante (blink)", WARN) + fault_box(140, 76) + text(70, -9, "FL"),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : '#eab308'"), "elements.led.blink": T("Moving"),
     "elements.forks.tx": B("(tag('{device}.Fork') || 0) * 0.1")})

# --- Dispositivi di campo -------------------------------------------------------
sym("dev_photocell", "Fotocellula", C_DEV, "Sensore", "0 0 80 40",
    E("rect", "body", "Corpo emettitore (colore di stato)", x=0, y=8, width=16, height=24, rx=3, fill=BODY, stroke=DARK)
    + E("rect", x=64, y=10, width=16, height=20, rx=3, fill="#e2e8f0", stroke=DARK)
    + E("line", "beam", "Raggio (stroke: interrotto)", x1=16, y1=20, x2=64, y2=20, stroke=FAULT, stroke_width=2.5, stroke_dasharray="5 3")
    + lamp(8, 14, 3, "led", "LED (fill)") + fault_box(80, 40) + text(40, 4, "PE", size=10),
    {"elements.beam.stroke": T("Blocked", kind="map", rules=[{"in": "true", "out": WARN}], default="#22c55e"),
     "elements.led.fill": T("Blocked", kind="map", rules=[{"in": "true", "out": WARN}], default="#334155"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Dirty') ? '#f59e0b' : '#64748b'")})

sym("dev_scanner", "Lettore barcode (pianta)", C_DEV, "Scanner", "0 0 100 90",
    E("polygon", "beam", "Fascio di lettura (fill)", points="50,30 96,88 4,88", fill="#ef4444", opacity=0.25)
    + E("rect", "body", "Colore di stato (testa)", x=18, y=6, width=64, height=26, rx=5, fill=BODY, stroke=DARK, stroke_width=2)
    + E("rect", x=32, y=24, width=36, height=6, rx=1, fill=GLASS)
    + lamp(74, 14, 4, "led", "LED lettura (fill)")
    + text(50, 0, "", id="code", desc="Ultimo codice (text)", size=9, fill="#e2e8f0") + fault_box(100, 90) + text(50, -12, "SCN"),
    {"elements.led.fill": B("tag('{device}.NoRead') ? '#ef4444' : tag('{device}.ReadOk') ? '#22c55e' : '#334155'"),
     "elements.beam.fill": B("tag('{device}.NoRead') ? '#ef4444' : '#22c55e'"),
     "elements.code.text": T("LastCode"), "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : '#64748b'")})

sym("dev_scale", "Bilancia dinamica", C_DEV, "Bilancia", "0 0 160 80",
    E("rect", x=0, y=50, width=160, height=12, rx=2, fill=FRD)
    + E("rect", "body", "Piatto (colore di stato)", x=4, y=36, width=152, height=14, rx=2, fill=BODY, stroke=DARK)
    + E("rect", x=50, y=62, width=60, height=18, rx=3, fill=DARK)
    + text(80, 71, "0.00 kg", id="display", desc="Display peso (text)", size=11, fill="#22d3ee")
    + carton(50, 6, 60, 30) + lamp(150, 70, 4, "led", "Peso stabile (fill)") + fault_box(160, 80) + text(80, -9, "BIL"),
    {"elements.display.text": B("(tag('{device}.Weight') ?? 0).toFixed(2) + ' kg'"),
     "elements.load.visible": B("(tag('{device}.Weight') || 0) > 0.05"),
     "elements.led.fill": T("Stable", kind="map", rules=[{"in": "true", "out": RUN}], default="#334155"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : '#64748b'")})

sym("dev_dimensioner", "Volumetro (portale)", C_DEV, "Bilancia", "0 0 140 110",
    E("rect", x=0, y=10, width=12, height=100, fill=FR) + E("rect", x=128, y=10, width=12, height=100, fill=FR)
    + E("rect", "body", "Traversa (colore di stato)", x=0, y=0, width=140, height=14, rx=2, fill=BODY)
    + E("polygon", "beam", "Fascio laser", points="70,14 120,100 20,100", fill=GLASS, opacity=0.18)
    + carton(40, 60, 60, 40) + fault_box(140, 110) + text(70, -9, "DIM"),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : '#64748b'"), "elements.load.visible": B("(tag('{device}.Length') || 0) > 0")})

lt = E("rect", x=10, y=140, width=20, height=10, fill=FRD) + E("rect", x=17, y=118, width=6, height=24, fill=FR)
for i, (nm, dim, lit) in enumerate([("red", "#450a0a", "#ef4444"), ("yellow", "#422006", "#facc15"), ("green", "#052e16", "#22c55e"), ("blue", "#172554", "#3b82f6")]):
    y = 4 + i * 28
    lt += E("rect", x=4, y=y, width=32, height=26, rx=3, fill=dim, stroke=DARK)
    lt += E("rect", nm, f"Luce {nm} accesa (visibile, blink)", True, x=4, y=y, width=32, height=26, rx=3, fill=lit,
            style=f"filter:drop-shadow(0 0 6px {lit})")
lt += E("rect", "body", "Cappello", x=6, y=0, width=28, height=5, rx=2, fill=FR)
sym("dev_lighttower", "Colonna luminosa", C_DEV, "Segnalatore", "0 0 40 150", lt + text(20, -9, "SEM"),
    {**{f"elements.{c}.visible": T(m) for c, m in (("red", "Red"), ("yellow", "Yellow"), ("green", "Green"), ("blue", "Blue"))},
     **{f"elements.{c}.blink": T("Blink") for c in ("red", "yellow", "green", "blue")}})

sym("dev_estop", "Pulsante di emergenza", C_DEV, "Emergenza", "0 0 70 70",
    E("rect", x=0, y=0, width=70, height=70, rx=6, fill="#facc15", stroke=DARK, stroke_width=2)
    + E("circle", "ring", "Anello (blink se premuto)", cx=35, cy=35, r=26, fill="none", stroke=FAULT, stroke_width=4)
    + E("circle", "body", "Fungo (colore)", cx=35, cy=35, r=19, fill="#b91c1c", stroke=DARK, stroke_width=2)
    + E("circle", cx=29, cy=29, r=6, fill="#ffffff", opacity=0.25) + text(35, -9, "EMG"),
    {"elements.ring.blink": T("Pressed"), "elements.ring.visible": T("Pressed"),
     "elements.body.fill": T("Pressed", kind="map", rules=[{"in": "true", "out": "#ef4444"}], default="#b91c1c")})

sym("dev_station", "Postazione picking", C_DEV, "Postazione", "0 0 160 110",
    E("rect", "body", "Banco (colore di stato)", x=0, y=30, width=160, height=50, rx=4, fill=BODY, stroke=DARK, stroke_width=2)
    + E("rect", x=50, y=4, width=60, height=24, rx=3, fill=DARK, stroke=FR)
    + text(80, 16, "", id="screen", desc="Ordine a video (text)", size=10, fill="#22d3ee")
    + E("rect", x=10, y=40, width=50, height=30, rx=3, fill="#1d4ed8") + E("rect", x=100, y=40, width=50, height=30, rx=3, fill="#1d4ed8")
    + lamp(35, 36, 5, "lamp", "Lampada pick (fill, blink)") + lamp(125, 36, 5, "lamp2", "Lampada conferma (fill)")
    + G("operator", "Operatore presente (visibile)", E("circle", cx=80, cy=98, r=11, fill="#fbbf24", stroke=DARK) + E("circle", cx=80, cy=98, r=4, fill=DARK), hidden=True)
    + fault_box(160, 110) + text(80, -9, "PICK"),
    {"elements.operator.visible": T("Operator"), "elements.screen.text": T("Order"),
     "elements.lamp.fill": T("Lamp", kind="map", rules=[{"in": "true", "out": WARN}], default="#334155"), "elements.lamp.blink": T("Lamp"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : '#64748b'")})

# --- Unità di carico ------------------------------------------------------------
sym("load_pallet", "Pallet con carico", C_LOAD, "UdC", "0 0 120 100", pallet(0, 0, 120, 100, id="load", hidden=False)
    + text(60, 50, "", id="code", desc="Identificativo (text)", size=11, fill="#1e293b", weight=700) + text(60, -9, "UDC"),
    {"elements.load.visible": T("Present"), "elements.code.text": T("Id")})
sym("load_carton", "Collo / cartone", C_LOAD, "UdC", "0 0 60 50", carton(2, 2, 56, 46, hidden=False)
    + text(30, 25, "", id="code", desc="Identificativo (text)", size=8, fill="#1e293b") + text(30, -9, "COLLO"),
    {"elements.load.visible": T("Present"), "elements.code.text": T("Id")})
sym("load_tote", "Cassetta / tote", C_LOAD, "UdC", "0 0 70 46",
    G("load", "Cassetta presente (visibile)", E("rect", x=2, y=2, width=66, height=42, rx=5, fill="#2563eb", stroke="#1e3a8a", stroke_width=3)
      + E("rect", x=10, y=9, width=50, height=28, rx=3, fill="#1d4ed8") + E("rect", x=26, y=4, width=18, height=5, rx=2, fill="#1e3a8a"))
    + text(35, 23, "", id="code", desc="Identificativo (text)", size=8, fill="#ffffff") + text(35, -9, "TOTE"),
    {"elements.load.visible": T("Present"), "elements.code.text": T("Id")})

# --- Macchine -------------------------------------------------------------------
sym("mac_palletizer", "Robot pallettizzatore", C_MEOL, "Macchina", "0 0 180 180",
    E("rect", x=0, y=0, width=180, height=180, rx=4, fill="none", stroke=WARN, stroke_width=2, stroke_dasharray="8 5")
    + pallet(100, 20, 70, 60, id="pal1", hidden=False, loaded=False) + pallet(100, 100, 70, 60, id="load")
    + E("circle", "body", "Base robot (colore di stato)", cx=60, cy=90, r=26, fill=BODY, stroke=DARK, stroke_width=2)
    + G("arm", "Braccio (rotate = angolo)", E("rect", x=60, y=82, width=70, height=16, rx=8, fill="#f97316", stroke=DARK)
        + E("rect", "gripper", "Pinza (fill = presa)", x=124, y=76, width=18, height=28, rx=3, fill=FR, stroke=DARK)
        + E("circle", cx=60, cy=90, r=10, fill=DARK), data_pivot="60 90")
    + fault_box(180, 180) + text(90, -9, "PAL"),
    {"elements.arm.rotate": T("Angle"), "elements.gripper.fill": T("Occupied", kind="map", rules=[{"in": "true", "out": ACC}], default=FR),
     "elements.load.visible": T("Ready")})

sym("mac_wrapper", "Avvolgitore / fasciapallet", C_MEOL, "Macchina", "0 0 170 170",
    E("rect", x=0, y=0, width=24, height=24, rx=3, fill=FRD) + E("rect", "body", "Colonna (colore di stato)", x=2, y=2, width=20, height=20, rx=3, fill=BODY)
    + E("circle", cx=95, cy=95, r=70, fill=FRD)
    + G("table", "Tavola rotante (spin in marcia)", E("circle", cx=95, cy=95, r=62, fill=FR) + pallet(60, 62, 70, 66, id="load", hidden=False)
        + E("line", x1=95, y1=33, x2=95, y2=45, stroke=ACC, stroke_width=4))
    + E("circle", "film", "Film (stroke)", cx=95, cy=95, r=55, fill="none", stroke="#e0f2fe", stroke_width=3, opacity=0.5, stroke_dasharray="6 6")
    + fault_box(170, 170) + text(85, -9, "AVV"),
    {"elements.table.spin": T("Run"), "elements.film.visible": T("Run")})

sym("mac_case_erector", "Formatrice cartoni", C_MPACK, "Macchina", "0 0 200 100",
    E("rect", "body", "Telaio (colore di stato)", x=0, y=10, width=200, height=80, rx=4, fill=BODY, stroke=DARK, stroke_width=2)
    + E("rect", x=6, y=16, width=60, height=68, fill=FRD)
    + "".join(E("rect", x=10 + i * 5, y=20, width=40, height=60, fill="#d6a35c", stroke=LOADD, opacity=0.9) for i in range(3))
    + E("rect", x=70, y=30, width=124, height=40, fill=BELT) + carton(120, 34, 42, 32)
    + flow_line("M72 50 H192", op=0.4) + fault_box(200, 100) + text(100, 0, "FORM"))

sym("mac_sealer", "Nastratrice", C_MPACK, "Macchina", "0 0 200 100",
    E("rect", x=0, y=30, width=200, height=40, fill=BELT)
    + E("rect", "body", "Portale (colore di stato)", x=70, y=4, width=60, height=92, rx=6, fill=BODY, stroke=DARK, stroke_width=2)
    + E("circle", "roll", "Rotolo nastro (spin in marcia)", cx=100, cy=22, r=12, fill="#d6a35c", stroke=LOADD, stroke_width=3)
    + carton(20, 34, 40, 32) + flow_line("M4 50 H196", op=0.4) + fault_box(200, 100) + text(100, -6, "NAS"),
    {"elements.roll.spin": T("Run")})

sym("mac_strapper", "Reggiatrice", C_MPACK, "Macchina", "0 0 200 110",
    E("rect", x=0, y=36, width=200, height=40, fill=BELT)
    + E("path", "body", "Arco (colore di stato)", d="M70 100 V14 Q70 4 80 4 H120 Q130 4 130 14 V100", fill="none", stroke=BODY, stroke_width=12)
    + E("line", "strap", "Reggia (visibile)", x1=100, y1=10, x2=100, y2=100, stroke=ACC, stroke_width=3)
    + carton(78, 40, 44, 32) + flow_line("M4 56 H196", op=0.4) + fault_box(200, 110) + text(100, -9, "REG"),
    {"elements.body.stroke": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Run') ? '#22c55e' : '#64748b'"),
     "elements.strap.visible": T("Run"), "elements.body.fill": B("'none'")})

sym("mac_labeler", "Etichettatrice print & apply", C_MPACK, "Macchina", "0 0 140 110",
    E("rect", x=0, y=70, width=140, height=36, fill=BELT)
    + E("rect", "body", "Corpo (colore di stato)", x=10, y=0, width=70, height=56, rx=5, fill=BODY, stroke=DARK, stroke_width=2)
    + E("circle", "roll", "Bobina etichette (spin)", cx=32, cy=22, r=13, fill="#f8fafc", stroke=FR, stroke_width=3)
    + G("pad", "Tampone (ty = applicazione)", E("rect", x=58, y=52, width=6, height=16, fill=FR) + E("rect", x=50, y=64, width=22, height=6, fill=ACC))
    + carton(84, 74, 44, 28) + fault_box(140, 110) + text(45, -9, "ETI"),
    {"elements.roll.spin": T("Run"), "elements.pad.ty": T("Occupied", kind="map", rules=[{"in": "true", "out": "6"}], default="0")})

sym("mac_depalletizer", "Depallettizzatore a strati", C_MEOL, "Macchina", "0 0 180 150",
    E("rect", x=0, y=0, width=12, height=150, fill=FR) + E("rect", x=168, y=0, width=12, height=150, fill=FR)
    + E("rect", "body", "Traversa (colore di stato)", x=0, y=0, width=180, height=14, fill=BODY)
    + G("head", "Testa di presa (ty)", E("rect", x=40, y=14, width=100, height=12, fill=ACC, stroke=DARK)
        + E("rect", x=48, y=26, width=84, height=6, fill=DARK))
    + "".join(carton(40 + (i % 3) * 34, 80 + (i // 3) * 22, 32, 20, id=None, hidden=False, desc=None) for i in range(9))
    + pallet(36, 142, 108, 8, id="pal", hidden=False, loaded=False) + fault_box(180, 150) + text(90, -9, "DEP"),
    {"elements.head.ty": B("(tag('{device}.Progress') || 0) * 0.5")})

sym("mac_shrink", "Tunnel di termoretrazione", C_MPACK, "Macchina", "0 0 220 100",
    E("rect", x=0, y=36, width=220, height=36, fill=BELT)
    + E("path", "body", "Tunnel (colore di stato)", d="M50 90 V20 Q50 6 64 6 H156 Q170 6 170 20 V90 Z", fill=BODY, stroke=DARK, stroke_width=2)
    + G("heat", "Riscaldamento (visibile, blink)", "".join(E("path", d=f"M{x} 70 q4 -8 0 -16 q-4 -8 0 -16", stroke="#f97316", stroke_width=3, fill="none") for x in range(78, 150, 16)))
    + flow_line("M4 54 H216", op=0.4) + fault_box(220, 100) + text(110, -6, "TUN"),
    {"elements.heat.visible": T("Run"), "elements.heat.blink": T("Run")})

sym("mac_robot", "Robot antropomorfo (vista laterale)", C_MROB, "Macchina", "0 0 160 160",
    E("rect", x=30, y=140, width=60, height=16, rx=3, fill=FRD)
    + E("rect", "body", "Base (colore di stato)", x=44, y=118, width=32, height=24, rx=4, fill=BODY)
    + G("arm", "Braccio (rotate = angolo)", E("rect", x=54, y=58, width=12, height=64, rx=6, fill="#f97316", stroke=DARK)
        + E("circle", cx=60, cy=60, r=9, fill=DARK) + E("rect", x=60, y=52, width=70, height=12, rx=6, fill="#f97316", stroke=DARK)
        + E("rect", "gripper", "Pinza (fill = presa)", x=126, y=48, width=14, height=22, rx=3, fill=FR, stroke=DARK), data_pivot="60 120")
    + E("circle", cx=60, cy=120, r=8, fill=DARK) + fault_box(160, 160) + text(80, -9, "ROB"),
    {"elements.arm.rotate": T("Angle"), "elements.gripper.fill": T("Occupied", kind="map", rules=[{"in": "true", "out": ACC}], default=FR)})

# --- Processo --------------------------------------------------------------------
sym("proc_pump", "Pompa centrifuga", C_PMOT, "Motore", "0 0 100 100",
    E("rect", x=6, y=84, width=88, height=12, rx=2, fill=FRD) + E("rect", x=58, y=8, width=36, height=18, fill=FR)
    + E("circle", "body", "Corpo pompa (colore di stato)", cx=46, cy=50, r=36, fill=BODY, stroke=DARK, stroke_width=3)
    + G("impeller", "Girante (spin in marcia)", E("path", d="M46 22 L54 50 L46 78 L38 50 Z", fill="#e2e8f0") + E("path", d="M18 50 L46 42 L74 50 L46 58 Z", fill="#e2e8f0"))
    + E("circle", cx=46, cy=50, r=6, fill=DARK) + fault_box(100, 100) + text(50, -9, "P"),
    {"elements.impeller.spin": T("Run")})
sym("proc_motor", "Motore elettrico", C_PMOT, "Motore", "0 0 120 80",
    E("rect", x=6, y=66, width=90, height=10, rx=2, fill=FRD)
    + E("rect", "body", "Carcassa (colore di stato)", x=10, y=10, width=86, height=56, rx=8, fill=BODY, stroke=DARK, stroke_width=2)
    + "".join(E("line", x1=x, y1=14, x2=x, y2=62, stroke=DARK, stroke_width=2, opacity=0.4) for x in range(22, 92, 10))
    + E("rect", x=96, y=32, width=20, height=12, fill=ROL) + G("shaft", "Albero (spin)", E("rect", x=108, y=28, width=6, height=20, fill=DARK))
    + text(53, 38, "M", id=None, desc=None, size=22, fill="#ffffff", weight=700) + fault_box(120, 80) + text(60, -9, "M"))
sym("proc_fan", "Ventilatore", C_PMOT, "Motore", "0 0 100 100",
    E("circle", "body", "Cassa (colore di stato)", cx=50, cy=50, r=46, fill=BODY, stroke=DARK, stroke_width=3)
    + G("blades", "Pale (spin in marcia)", "".join(E("path", d="M50 50 Q40 20 50 10 Q62 22 50 50", fill="#e2e8f0", transform=f"rotate({a} 50 50)") for a in (0, 90, 180, 270)))
    + E("circle", cx=50, cy=50, r=7, fill=DARK) + fault_box(100, 100) + text(50, -9, "FAN"),
    {"elements.blades.spin": T("Run")})
sym("proc_compressor", "Compressore", C_PMOT, "Motore", "0 0 120 100",
    E("rect", x=4, y=84, width=112, height=12, rx=2, fill=FRD)
    + E("path", "body", "Corpo (colore di stato)", d="M20 84 L10 20 H110 L100 84 Z", fill=BODY, stroke=DARK, stroke_width=3)
    + E("path", d="M30 30 L90 52 L30 74", fill="none", stroke="#e2e8f0", stroke_width=4)
    + fault_box(120, 100) + text(60, -9, "CMP"))
sym("proc_mixer", "Miscelatore / agitatore", C_PMOT, "Motore", "0 0 100 140",
    E("rect", x=38, y=0, width=24, height=20, rx=3, fill=FR)
    + E("path", "body", "Vasca (colore di stato)", d="M10 30 H90 V110 Q50 140 10 110 Z", fill=BODY, stroke=DARK, stroke_width=3, opacity=0.85)
    + E("line", x1=50, y1=20, x2=50, y2=110, stroke=DARK, stroke_width=4)
    + G("impeller", "Agitatore (spin)", E("path", d="M28 104 L72 112 M28 112 L72 104", stroke="#e2e8f0", stroke_width=6))
    + fault_box(100, 140) + text(50, -9, "MIX"),
    {"elements.impeller.spin": T("Run")})

sym("proc_valve", "Valvola on/off", C_PVAL, "Valvola", "0 0 100 80",
    E("rect", x=45, y=12, width=10, height=26, fill=FR)
    + E("rect", "actuator", "Attuatore (colore)", x=28, y=2, width=44, height=14, rx=3, fill=ROL, stroke=DARK, stroke_width=2)
    + E("polygon", "body", "Corpo (aperta/chiusa)", points="4,34 50,56 4,78", fill=BODY, stroke=DARK, stroke_width=3)
    + E("polygon", "body2", "Corpo (seconda metà)", points="96,34 50,56 96,78", fill=BODY, stroke=DARK, stroke_width=3)
    + fault_box(100, 80) + text(50, -9, "XV"),
    {"elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Open') ? '#22c55e' : tag('{device}.Closed') ? '#64748b' : '#f59e0b'"),
     "elements.body2.fill": B("tag('{device}.Fault') ? '#ef4444' : tag('{device}.Open') ? '#22c55e' : tag('{device}.Closed') ? '#64748b' : '#f59e0b'")})
sym("proc_valve_ctrl", "Valvola di regolazione", C_PVAL, "ValvolaReg", "0 0 100 100",
    E("path", "actuator", "Membrana (colore)", d="M22 30 Q50 0 78 30 Z", fill=ROL, stroke=DARK, stroke_width=2)
    + E("rect", x=46, y=30, width=8, height=24, fill=FR)
    + E("polygon", "body", "Corpo (colore di stato)", points="4,54 50,76 4,98", fill=BODY, stroke=DARK, stroke_width=3)
    + E("polygon", "body2", "Corpo (seconda metà)", points="96,54 50,76 96,98", fill=BODY, stroke=DARK, stroke_width=3)
    + text(50, 20, "", id="value", desc="Apertura % (text)", size=10, fill=DARK, weight=700)
    + fault_box(100, 100) + text(50, -9, "FCV"),
    {"elements.value.text": B("(tag('{device}.Position') ?? 0).toFixed(0) + '%'"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : (tag('{device}.Position') || 0) > 1 ? '#22c55e' : '#64748b'"),
     "elements.body2.fill": B("tag('{device}.Fault') ? '#ef4444' : (tag('{device}.Position') || 0) > 1 ? '#22c55e' : '#64748b'")})
sym("proc_valve3", "Valvola a 3 vie", C_PVAL, "Valvola", "0 0 100 100",
    E("rect", x=45, y=6, width=10, height=22, fill=FR) + E("rect", x=30, y=0, width=40, height=10, rx=3, fill=ROL)
    + E("polygon", "body", "Via A (colore)", points="4,30 50,52 4,74", fill=BODY, stroke=DARK, stroke_width=3)
    + E("polygon", "body2", "Via B (colore)", points="96,30 50,52 96,74", fill=BODY, stroke=DARK, stroke_width=3)
    + E("polygon", "body3", "Via C (colore)", points="28,98 50,52 72,98", fill=BODY, stroke=DARK, stroke_width=3)
    + fault_box(100, 100) + text(50, -9, "3V"),
    {"elements.body.fill": B("tag('{device}.Open') ? '#22c55e' : '#64748b'"), "elements.body3.fill": B("tag('{device}.Open') ? '#64748b' : '#22c55e'")})

tank_clip = E("defs", inner=E("clipPath", "tankshape", inner=E("rect", x=8, y=8, width=84, height=144, rx=12)))
sym("proc_tank", "Serbatoio", C_PTANK, "Serbatoio", "0 0 100 160",
    tank_clip + E("rect", "body", "Mantello (colore)", x=4, y=4, width=92, height=152, rx=14, fill=FRD, stroke=FR, stroke_width=4)
    + G(None, None, E("rect", "liquid", "Liquido (ty = 144 - livello)", x=8, y=8, width=84, height=144, fill="#0ea5e9", opacity=0.85), clip_path="url(#tankshape)")
    + text(50, 80, "", id="value", desc="Livello % (text)", size=14, fill="#ffffff", weight=700)
    + E("rect", "hi", "Livello alto (visibile)", True, x=88, y=14, width=10, height=10, fill=WARN)
    + E("rect", "lo", "Livello basso (visibile)", True, x=88, y=136, width=10, height=10, fill=WARN)
    + text(50, -9, "TK"),
    {"elements.liquid.ty": B("144 - Math.max(0, Math.min(100, tag('{device}.Level') || 0)) * 1.44"),
     "elements.value.text": B("(tag('{device}.Level') ?? 0).toFixed(0) + '%'"),
     "elements.hi.visible": T("HighLevel"), "elements.lo.visible": T("LowLevel")})
sym("proc_silo", "Silo", C_PTANK, "Serbatoio", "0 0 100 180",
    E("defs", inner=E("clipPath", "siloshape", inner=E("path", d="M10 20 H90 V120 L58 160 H42 L10 120 Z")))
    + E("path", "body", "Mantello (colore)", d="M6 16 Q50 -4 94 16 V122 L60 164 H40 L6 122 Z", fill=FRD, stroke=FR, stroke_width=4)
    + G(None, None, E("rect", "liquid", "Materiale (ty = 140 - livello)", x=6, y=20, width=88, height=144, fill="#d6a35c"), clip_path="url(#siloshape)")
    + E("rect", x=40, y=164, width=20, height=14, fill=FR) + text(50, 70, "", id="value", desc="Livello % (text)", size=14, fill="#ffffff", weight=700)
    + text(50, -9, "SILO"),
    {"elements.liquid.ty": B("144 - Math.max(0, Math.min(100, tag('{device}.Level') || 0)) * 1.44"),
     "elements.value.text": B("(tag('{device}.Level') ?? 0).toFixed(0) + '%'")})
sym("proc_hopper", "Tramoggia", C_PTANK, "Serbatoio", "0 0 120 110",
    E("defs", inner=E("clipPath", "hopshape", inner=E("path", d="M8 8 H112 L72 96 H48 Z")))
    + E("path", "body", "Mantello (colore)", d="M4 4 H116 L74 100 H46 Z", fill=FRD, stroke=FR, stroke_width=4)
    + G(None, None, E("rect", "liquid", "Materiale (ty = 92 - livello)", x=4, y=8, width=112, height=92, fill="#d6a35c"), clip_path="url(#hopshape)")
    + text(60, 40, "", id="value", desc="Livello % (text)", size=13, fill="#ffffff", weight=700) + text(60, -9, "TRM"),
    {"elements.liquid.ty": B("92 - Math.max(0, Math.min(100, tag('{device}.Level') || 0)) * 0.92"),
     "elements.value.text": B("(tag('{device}.Level') ?? 0).toFixed(0) + '%'")})

sym("proc_hx", "Scambiatore di calore", C_PEQ, "Scambiatore", "0 0 180 80",
    E("rect", "body", "Mantello (colore di stato)", x=20, y=14, width=140, height=52, rx=26, fill=BODY, stroke=DARK, stroke_width=3)
    + E("path", d="M30 40 H60 L70 24 L82 56 L94 24 L106 56 L118 24 L128 40 H150", fill="none", stroke="#f97316", stroke_width=3)
    + E("rect", x=0, y=34, width=20, height=12, fill=FR) + E("rect", x=160, y=34, width=20, height=12, fill=FR)
    + text(90, 74, "", id="value", desc="Temperature (text)", size=10, fill=LBL) + fault_box(180, 80) + text(90, -9, "HX"),
    {"elements.value.text": B("(tag('{device}.TempIn') ?? 0).toFixed(1) + ' → ' + (tag('{device}.TempOut') ?? 0).toFixed(1) + ' °C'"),
     "elements.body.fill": B("tag('{device}.Fault') ? '#ef4444' : '#64748b'")})
sym("proc_filter", "Filtro", C_PEQ, "Filtro", "0 0 80 120",
    E("rect", "body", "Corpo (intasato)", x=10, y=10, width=60, height=100, rx=8, fill=BODY, stroke=DARK, stroke_width=3)
    + "".join(E("line", x1=18, y1=y, x2=62, y2=y, stroke=DARK, stroke_dasharray="4 3", stroke_width=2) for y in range(26, 100, 12))
    + E("rect", x=0, y=20, width=10, height=12, fill=FR) + E("rect", x=70, y=88, width=10, height=12, fill=FR)
    + text(40, 118, "", id="value", desc="ΔP (text)", size=10, fill=LBL) + text(40, -9, "FLT"),
    {"elements.body.fill": T("Clogged", kind="map", rules=[{"in": "true", "out": WARN}], default=BODY),
     "elements.value.text": B("'ΔP ' + (tag('{device}.DeltaP') ?? 0).toFixed(2) + ' bar'")})


def instrument(id, title, letters, unit):
    sym(id, title, C_PINS, "Strumento", "0 0 90 90",
        E("line", x1=45, y1=62, x2=45, y2=90, stroke=FR, stroke_width=3)
        + E("circle", "body", "Bolla strumento (colore allarme)", cx=45, cy=34, r=28, fill="#f8fafc", stroke=DARK, stroke_width=2.5)
        + E("line", x1=17, y1=34, x2=73, y2=34, stroke=DARK, stroke_width=1.5)
        + text(45, 23, letters, id=None, desc=None, size=13, fill=DARK, weight=700)
        + text(45, 46, "", id="value", desc=f"Valore in {unit} (text)", size=11, fill="#0f172a", weight=600)
        + text(45, -9, letters),
        {"elements.value.text": B("(tag('{device}.Value') ?? 0).toFixed(1) + ' " + unit + "'"),
         "elements.body.fill": B("tag('{device}.Fault') ? '#fecaca' : (tag('{device}.HH') || tag('{device}.LL')) ? '#fca5a5' : (tag('{device}.H') || tag('{device}.L')) ? '#fde68a' : '#f8fafc'")})


instrument("proc_flow", "Misuratore di portata", "FT", "m³/h")
instrument("proc_pressure", "Trasmettitore di pressione", "PT", "bar")
instrument("proc_level", "Trasmettitore di livello", "LT", "%")
instrument("proc_temp", "Sonda di temperatura", "TT", "°C")

pipe_flow = lambda d: E("path", "flow", "Flusso (flow)", d=d, fill="none", stroke="#38bdf8", stroke_width=4, stroke_dasharray="10 10", opacity=0.9, style="--dl:20")
sym("proc_pipe", "Tubazione dritta", C_PIPE, "Tubazione", "0 0 200 30",
    E("rect", "body", "Tubo (colore)", x=0, y=4, width=200, height=22, fill="#94a3b8", stroke=FR, stroke_width=2)
    + E("rect", x=0, y=8, width=200, height=5, fill="#ffffff", opacity=0.25) + pipe_flow("M0 15 H200"),
    {"elements.flow.flow": T("Flowing"), "elements.flow.visible": T("Flowing")})
sym("proc_elbow", "Curva tubazione 90°", C_PIPE, "Tubazione", "0 0 100 100",
    E("path", "body", "Tubo (colore)", d="M0 39 H40 Q61 39 61 60 V100", fill="none", stroke="#94a3b8", stroke_width=22)
    + pipe_flow("M0 39 H40 Q61 39 61 60 V100"),
    {"elements.flow.flow": T("Flowing"), "elements.flow.visible": T("Flowing"), "elements.body.stroke": B("'#94a3b8'")})
sym("proc_tee", "Raccordo a T", C_PIPE, "Tubazione", "0 0 200 100",
    E("rect", "body", "Tubo (colore)", x=0, y=4, width=200, height=22, fill="#94a3b8", stroke=FR, stroke_width=2)
    + E("rect", "body2", "Derivazione (colore)", x=89, y=26, width=22, height=74, fill="#94a3b8", stroke=FR, stroke_width=2)
    + pipe_flow("M0 15 H200") + E("path", "flow2", "Flusso derivazione (flow)", d="M100 15 V100", fill="none", stroke="#38bdf8", stroke_width=4, stroke_dasharray="10 10", opacity=0.9, style="--dl:20"),
    {"elements.flow.flow": T("Flowing"), "elements.flow.visible": T("Flowing"), "elements.flow2.flow": T("Flowing"), "elements.flow2.visible": T("Flowing")})


# ------------------------------------------------------------------ faceplate

def fp_component(cid, ctype, x, y, w, h, props=None, bindings=None, events=None, name=None):
    return {"id": cid, "type": ctype, "name": name or cid, "x": x, "y": y, "w": w, "h": h, "rotation": 0,
            "props": props or {}, "bindings": bindings or {}, "events": events or {}}


def faceplate(sid, title, fam, members, sym_bindings, vb):
    comps = []
    W = 460
    comps.append(fp_component("bg", "rect", 0, 0, W, 10, {"fill": "#111a2e", "stroke": "#24324f", "radius": 10}))
    comps.append(fp_component("title", "label", 20, 14, 300, 30, {"text": "", "fontSize": 20, "bold": True},
                              {"text": B("param('device') || '(nessun dispositivo)'")}))
    comps.append(fp_component("subtitle", "label", 20, 42, 300, 20, {"text": f"{title} · UDT {fam}", "fontSize": 12, "color": "#8b9bb8"}))
    # anteprima simbolo
    vbw, vbh = [float(v) for v in vb.split()[2:]]
    k = min(140 / vbw, 110 / vbh)
    pw, ph = round(vbw * k), round(vbh * k)
    comps.append(fp_component("preview", "symbol", W - 20 - pw, 24 + (110 - ph) // 2, pw, ph, {"symbol": sid}, dict(sym_bindings)))
    y = 150
    bools = [m for m in members if m[1] == "bool" and not m[2]]
    nums = [m for m in members if m[1] != "bool" and not m[2]]
    cmds = [m for m in members if m[2]]
    if bools:
        comps.append(fp_component("h_state", "label", 20, y, 200, 20, {"text": "STATO", "fontSize": 11, "color": "#8b9bb8", "bold": True}))
        y += 26
        for i, (mn, t, w, u, d) in enumerate(bools[:12]):
            col = "#ef4444" if mn in ("Fault", "Pressed", "HH", "LL", "Clogged", "Full", "NoRead") else "#f59e0b" if mn in ("H", "L", "Moving", "Dirty", "HighLevel", "LowLevel", "Blocked") else "#22c55e"
            cx = 20 + (i % 2) * 215
            cy = y + (i // 2) * 28
            comps.append(fp_component(f"st_{mn}", "lamp", cx, cy, 20, 20, {"label": d, "colorOn": col}, {"on": T(mn)}))
        y += ((min(len(bools), 12) + 1) // 2) * 28 + 10
    if nums:
        comps.append(fp_component("h_val", "label", 20, y, 200, 20, {"text": "MISURE", "fontSize": 11, "color": "#8b9bb8", "bold": True}))
        y += 26
        for i, (mn, t, w, u, d) in enumerate(nums[:8]):
            cx = 20 + (i % 2) * 215
            cy = y + (i // 2) * 52
            comps.append(fp_component(f"v_{mn}", "value", cx, cy + 14, 200, 30,
                                      {"label": d, "unit": u, "decimals": 2 if t == "f32" else 0, "fontSize": 16}, {"value": T(mn)}))
        y += ((min(len(nums), 8) + 1) // 2) * 52 + 10
    if cmds:
        comps.append(fp_component("h_cmd", "label", 20, y, 200, 20, {"text": "COMANDI", "fontSize": 11, "color": "#8b9bb8", "bold": True}))
        y += 26
        for i, (mn, t, w, u, d) in enumerate(cmds[:8]):
            cx = 20 + (i % 3) * 143
            cy = y + (i // 3) * 50
            color = "#b91c1c" if "Stop" in mn or "Close" in mn else "#15803d" if "Start" in mn or "Open" in mn or "Resume" in mn else "#1d4ed8"
            val = "true" if t == "bool" else "prompt"
            if t == "bool" and mn in ("Enabled", "Red", "Yellow", "Green", "Blue", "Buzzer", "Blink"):
                val = "toggle"
            comps.append(fp_component(f"c_{mn}", "button", cx, cy, 133, 40, {"text": d, "fill": color, "fontSize": 13},
                                      {"active": T(mn)} if val == "toggle" else {},
                                      {"onClick": {"actions": [{"kind": "write", "tag": "{device}." + mn, "value": val}],
                                                   **({"confirm": f"{d}?"} if "Stop" in mn or "Close" in mn else {})}}))
        y += ((min(len(cmds), 8) + 2) // 3) * 50 + 6
    trend_m = next((m for m in nums if m[1] == "f32"), None)
    if trend_m:
        comps.append(fp_component("h_trend", "label", 20, y, 300, 20, {"text": f"TREND – {trend_m[4].upper()}", "fontSize": 11, "color": "#8b9bb8", "bold": True}))
        y += 24
        comps.append(fp_component("trend", "trend", 20, y, W - 40, 130, {"tags": "{device}." + trend_m[0], "minutes": 10}))
        y += 140
    comps[0]["h"] = y + 10
    return {"title": f"Faceplate – {title}", "width": W, "height": y + 10, "background": "#0b1220", "home": False,
            "faceplate": True, "params": {"device": ""}, "components": comps}


# ------------------------------------------------------------------ scrittura

def main():
    os.makedirs(f"{OUT}/symbols", exist_ok=True)
    os.makedirs(f"{OUT}/library", exist_ok=True)
    os.makedirs(f"{OUT}/views", exist_ok=True)
    meta = {}
    for sid, title, cat, fam, vb, body, extra in SYMBOLS:
        fam_desc, members = FAMILIES[fam]
        svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" data-category="{escape(cat)}" '
               f'data-udt="{fam}" data-faceplate="fp_{sid}"><title>{escape(title)}</title>{body}</svg>')
        with open(f"{OUT}/symbols/{sid}.svg", "w", encoding="utf-8") as f:
            f.write(svg)
        ids = set()
        import re
        for m in re.finditer(r'\bid="([^"]+)"', body):
            ids.add(m.group(1))
        bindings = {}
        for path, b in {**common(members), **extra}.items():
            el = path.split(".")[1]
            if el in ids:
                bindings[path] = b
        meta[sid] = {"title": title, "category": cat, "udt": fam, "faceplate": f"fp_{sid}", "bindings": bindings}
        fp = faceplate(sid, title, fam, members, bindings, vb)
        with open(f"{OUT}/views/fp_{sid}.json", "w", encoding="utf-8") as f:
            json.dump(fp, f, indent=1, ensure_ascii=False)
    with open(f"{OUT}/library/symbols.json", "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=1, ensure_ascii=False)
    udts = [build_udt(n, d, m) for n, (d, m) in FAMILIES.items()]
    with open(f"{OUT}/library/udts.json", "w", encoding="utf-8") as f:
        json.dump(udts, f, indent=1, ensure_ascii=False)
    cats = sorted({m["category"] for m in meta.values()})
    print(f"{len(meta)} simboli, {len(udts)} UDT, {len(meta)} faceplate in {OUT}/  categorie: {len(cats)}")


if __name__ == "__main__":
    main()
