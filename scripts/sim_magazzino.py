"""Simulazione demo (script a timer): anima i dispositivi della vista "Magazzino"
(driver interno WCS_INTERNO). In un impianto reale questo lavoro lo fanno i PLC.
"""
import math
import random

from _sim_util import _approach

_sim = {"t": 0, "crane": None, "lift_next": 0, "dock_next": 0}


def sim_magazzino(payload, ctx):
    s = _sim
    s["t"] += 1
    t = s["t"]
    w = ctx.write
    g = ctx.tag

    # nastri: comandi marcia/arresto/reset (avvio automatico all'inizio della demo)
    for cv in ("CVM1", "CVM2"):
        if t == 3:
            w(f"{cv}.Cmd_Start", True)
        if g(f"{cv}.Cmd_Start"):
            w(f"{cv}.Run", True); w(f"{cv}.Cmd_Start", False)
        if g(f"{cv}.Cmd_Stop"):
            w(f"{cv}.Run", False); w(f"{cv}.Cmd_Stop", False)
        if g(f"{cv}.Cmd_Reset"):
            w(f"{cv}.Fault", False); w(f"{cv}.Cmd_Reset", False)
        run = bool(g(f"{cv}.Run"))
        w(f"{cv}.Auto", True)
        w(f"{cv}.Speed", round(30 + random.uniform(-0.5, 0.5), 2) if run else 0.0)
        w(f"{cv}.Current", round(4.2 + random.uniform(-0.2, 0.2), 2) if run else 0.0)
        if run and t % 7 == 0:
            w(f"{cv}.Occupied", not g(f"{cv}.Occupied"))

    # lift: si muove verso il piano richiesto (0..2)
    if g("LIFT1.Cmd_Reset"):
        w("LIFT1.Fault", False); w("LIFT1.Cmd_Reset", False)
    target = int(g("LIFT1.Target") or 0)
    pos = _approach(g("LIFT1.PosPct"), target * 50.0, 4.0)
    w("LIFT1.PosPct", pos)
    w("LIFT1.Position", round(pos * 60, 1))
    moving = abs(pos - target * 50.0) > 0.01
    w("LIFT1.Moving", moving)
    if not moving:
        w("LIFT1.Level", target)
        if t >= s["lift_next"]:
            s["lift_next"] = t + 25
            w("LIFT1.Target", random.choice([x for x in (0, 1, 2) if x != target]))
            w("LIFT1.Occupied", random.random() > 0.4)
    w("LIFT1.Auto", True)

    # porte: comandi apri/chiudi; la baia di carico cicla da sola
    if t >= s["dock_next"]:
        s["dock_next"] = t + 40
        w("DOCK1.Cmd_Open" if not g("DOCK1.Open") else "DOCK1.Cmd_Close", True)
    for d in ("DOOR1", "DOCK1"):
        pct = g(f"{d}.OpenPct") or 0
        if g(f"{d}.Cmd_Open"):
            pct = _approach(pct, 100, 8)
            if pct >= 100:
                w(f"{d}.Cmd_Open", False)
        elif g(f"{d}.Cmd_Close"):
            pct = _approach(pct, 0, 8)
            if pct <= 0:
                w(f"{d}.Cmd_Close", False)
        w(f"{d}.OpenPct", pct)
        w(f"{d}.Open", pct >= 100)
        w(f"{d}.Closed", pct <= 0)
        w(f"{d}.Moving", 0 < pct < 100)

    # traslo: missioni casuali con presa/deposito nelle celle
    c = s["crane"]
    if c is None:
        c = s["crane"] = {"x": random.uniform(5, 95), "y": random.uniform(0, 100), "phase": "go", "fork": 0}
    x = _approach(g("CRANE1.PctX"), c["x"], 3)
    y = _approach(g("CRANE1.PctY"), c["y"], 4)
    w("CRANE1.PctX", x); w("CRANE1.PctY", y)
    w("CRANE1.PosX", round(x * 0.6, 2)); w("CRANE1.PosY", round(y * 0.18, 2))
    arrived = abs(x - c["x"]) < 0.01 and abs(y - c["y"]) < 0.01
    if c["phase"] == "go" and arrived:
        c["phase"] = "fork"
    elif c["phase"] == "fork":
        c["fork"] = _approach(c["fork"], 100, 25)
        if c["fork"] >= 100:
            c["phase"] = "back"
            w("CRANE1.Loaded", not g("CRANE1.Loaded"))
            cell = random.randint(1, 12)
            w(f"RACK1.C{cell}", not g(f"RACK1.C{cell}"))
    elif c["phase"] == "back":
        c["fork"] = _approach(c["fork"], 0, 25)
        if c["fork"] <= 0:
            s["crane"] = None
            w("CRANE1.Mission", int(g("CRANE1.Mission") or 0) + 1)
    w("CRANE1.Fork", c["fork"])
    w("CRANE1.Moving", not arrived)
    w("CRANE1.Auto", True)
    occ = sum(1 for i in range(1, 13) if g(f"RACK1.C{i}"))
    w("RACK1.Cells", 12); w("RACK1.OccupiedCells", occ); w("RACK1.FillPct", round(occ / 12 * 100, 1))

    # AGV: batteria e ricarica
    batt = g("AGV1.Battery")
    batt = 80.0 if batt in (None, 0) else batt
    charging = bool(g("AGV1.Charging"))
    if charging:
        batt = min(100.0, batt + 1.5)
        if batt >= 95:
            charging = False
    else:
        batt = max(0.0, batt - 0.15)
        if batt < 20:
            charging = True
    w("AGV1.Battery", round(batt, 1)); w("AGV1.Charging", charging)
    w("AGV1.Moving", not charging and (t // 20) % 3 != 2)
    w("AGV1.State", 0 if charging else 1)
    w("AGV1.Loaded", (t // 30) % 2 == 0)

    # processo: serbatoio con pompa di riempimento e valvola di scarico
    lvl = g("TK1.Level")
    lvl = 50.0 if lvl is None else lvl
    pump = bool(g("P1.Run"))
    if g("P1.Cmd_Start"):
        pump = True; w("P1.Cmd_Start", False)
    if g("P1.Cmd_Stop"):
        pump = False; w("P1.Cmd_Stop", False)
    if lvl < 25:
        pump = True
    if lvl > 85:
        pump = False
    lvl = max(0.0, min(100.0, lvl + (0.8 if pump else 0) - (0.5 if g("XV1.Open") else 0.1)))
    if g("XV1.Cmd_Open"):
        w("XV1.Open", True); w("XV1.Closed", False); w("XV1.Cmd_Open", False)
    if g("XV1.Cmd_Close"):
        w("XV1.Open", False); w("XV1.Closed", True); w("XV1.Cmd_Close", False)
    if t % 60 == 0:
        w("XV1.Cmd_Open" if not g("XV1.Open") else "XV1.Cmd_Close", True)
    w("TK1.Level", round(lvl, 1)); w("TK1.Volume", round(lvl * 0.2, 2)); w("TK1.Temp", round(20 + 3 * math.sin(t / 50), 2))
    w("TK1.HighLevel", lvl > 80); w("TK1.LowLevel", lvl < 20)
    w("P1.Run", pump); w("P1.Speed", 100.0 if pump else 0.0); w("P1.Current", round(7.5 + random.uniform(-0.3, 0.3), 2) if pump else 0.0)
    w("LT1.Value", round(lvl, 1)); w("LT1.H", lvl > 80); w("LT1.L", lvl < 20)
    return None
