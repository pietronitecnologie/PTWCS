"""Regole di business del WCS.

Funzioni dei punti di decisione:  def f(req, ctx) -> dict
  req  : dict con gli input configurati (es. barcode, weight) + request_id
  ctx  : ctx.tag("Nome") legge un tag, ctx.write("Nome", v) scrive un tag,
         ctx.log("msg") scrive nel log eventi, ctx.tags = snapshot di tutti i tag
  ritorno: dict con le uscite (es. {"destination": 2, "result": 0})

Funzioni evento HMI (pulsanti del Designer):  def f(payload, ctx) -> qualsiasi
  il valore ritornato può contenere {"message": "..."} (mostrato all'operatore)
  oppure {"navigate": "id_vista"}.

Lo stato globale del modulo resta in memoria tra le chiamate
(si azzera con "Ricarica script").
"""

# Codici esito verso il PLC
OK, NOREAD, OVERWEIGHT, UNKNOWN_DEST = 0, 1, 2, 3
REJECT = 99

# Tabella di instradamento: prefisso barcode -> uscita
ROUTES = {
    "IT-MI": 1, "IT-TO": 1, "IT-BO": 1,
    "IT-RM": 2, "IT-NA": 2, "IT-FI": 2,
    "DE": 3, "FR": 3, "ES": 3,
}

stats = {"total": 0, "rejected": 0}


def decide_sc1(req, ctx):
    stats["total"] += 1
    barcode = (req.get("barcode") or "").strip()
    weight = req.get("weight") or 0
    max_weight = ctx.tag("WCS.MaxWeight") or 3000

    if not barcode or barcode == "NOREAD":
        return reject(NOREAD, ctx, "lettura fallita")
    if weight > max_weight:
        return reject(OVERWEIGHT, ctx, f"{barcode} sovrappeso {weight / 100:.2f} kg")

    dest = next((d for p, d in ROUTES.items() if barcode.startswith(p)), None)
    if dest is None:
        return reject(UNKNOWN_DEST, ctx, f"{barcode} destinazione sconosciuta")
    if dest == 3 and not ctx.tag("WCS.Out3Enabled", True):
        dest = 2  # uscita 3 disabilitata: devia su uscita 2
    return {"destination": dest, "result": OK}


def reject(code, ctx, why):
    stats["rejected"] += 1
    ctx.log(f"scarto: {why}")
    return {"destination": REJECT, "result": code}


# ---------------------------------------------------------------- eventi HMI

def start_line(payload, ctx):
    """Avvio linea in sequenza (valle -> monte)."""
    if ctx.tag("Line.EStop"):
        return {"message": "Impossibile avviare: emergenza attiva"}
    for cv in ("CV3", "CV2", "CV1"):
        ctx.write(f"{cv}.Run", True)
    ctx.log("linea avviata da " + str(ctx.user))
    return {"message": "Linea avviata"}


def stop_line(payload, ctx):
    for cv in ("CV1", "CV2", "CV3"):
        ctx.write(f"{cv}.Run", False)
    return {"message": "Linea fermata"}


def reset_faults(payload, ctx):
    ctx.write("Line.ResetFaults", True)
    return {"message": "Reset guasti inviato"}


def get_stats(payload, ctx):
    total = stats["total"] or 1
    return {"message": f"Colli: {stats['total']} · scartati: {stats['rejected']} "
                       f"({100 * stats['rejected'] / total:.1f}%)"}


# ---------------------------------------------------------------- simulazione demo (script a timer)
# Anima i dispositivi della vista "Magazzino" (driver interno WCS_INTERNO).
# In un impianto reale questo lavoro lo fanno i PLC: lo script serve solo per la demo.
import math
import random

_sim = {"t": 0, "crane": None, "lift_next": 0, "dock_next": 0}


def _approach(cur, target, step):
    cur = cur or 0
    return target if abs(target - cur) <= step else cur + step * (1 if target > cur else -1)


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


# ---------------------------------------------------------------- simulazione magazzino pallet MP1
# Simula i PLC: ingressi IN1/IN2 (telegramma di richiesta), rulliere di
# distribuzione, 3 trasli C1..C3 (telegramma missioni Start/Busy/Done) e baie BU1/BU2.
# La logica di magazzino (locazioni, allocazione, missioni) è nel WCS, non qui.

import time as _time
_mp = {"t": 0, "seq": int(_time.time()) % 90000, "next_in": {"IN1": 10, "IN2": 40}, "pending": {}, "transit": [],
       "pd_in": {1: [], 2: [], 3: []}, "out_transit": [], "bay_clear": {}, "crane": {}}
MP_COLS, MP_LEVELS = 50, 10


def _mp_target(col, lvl):
    return (col / MP_COLS * 100.0, 0.0 if lvl <= 0 else (lvl - 1) / (MP_LEVELS - 1) * 100.0)


def sim_pallet(payload, ctx):
    s = _mp
    s["t"] += 1
    t = s["t"]
    g, w = ctx.tag, ctx.write

    # --- ingressi: arrivo pallet e richiesta al WCS
    for inp in ("IN1", "IN2"):
        # dopo un ricaricamento dello script: ricostruisce la richiesta pendente dai tag
        if g(f"{inp}.Occupied") and inp not in s["pending"]:
            s["pending"][inp] = {"lu": g(f"{inp}.Barcode") or "", "since": t, "req": int(g(f"{inp}.ReqID") or 0)}
        if not g(f"{inp}.Occupied") and t >= s["next_in"][inp] and inp not in s["pending"]:
            s["seq"] += 1
            r = random.random()
            lu = "NOREAD" if r < 0.04 else f"PX{s['seq']:05d}"
            weight = round(random.uniform(1250, 1500) if 0.04 <= r < 0.07 else random.uniform(150, 1000), 1)
            height = random.randint(1850, 2000) if 0.07 <= r < 0.09 else random.randint(900, 1700)
            w(f"{inp}.Barcode", lu); w(f"{inp}.Weight", weight); w(f"{inp}.Height", height)
            w(f"{inp}.Occupied", True)
            w(f"{inp}.ReqID", (int(g(f"{inp}.ReqID") or 0) % 65000) + 1)
            s["pending"][inp] = {"lu": lu, "since": t, "req": (int(g(f"{inp}.ReqID") or 0) % 65000) + 1}
        p = s["pending"].get(inp)
        if p and g(f"{inp}.RespID") == p["req"] and t > p["since"] + 2:
            dest = int(g(f"{inp}.Destination") or 0)
            if dest in (1, 2, 3):
                s["transit"].append({"lu": p["lu"], "aisle": dest, "eta": t + 15 + dest * 8})
                w(f"{inp}.Occupied", False)
                del s["pending"][inp]
                s["next_in"][inp] = t + random.randint(55, 110)
            elif t > p["since"] + 30:           # scarto: l'operatore rimuove il pallet
                w(f"{inp}.Occupied", False)
                w(f"{inp}.Rejected", int(g(f"{inp}.Rejected") or 0) + 1)
                del s["pending"][inp]
                s["next_in"][inp] = t + random.randint(40, 80)

    # --- rulliera di distribuzione → P&D di ingresso della corsia
    for it in list(s["transit"]):
        if t >= it["eta"]:
            s["transit"].remove(it)
            s["pd_in"][it["aisle"]].append(it["lu"])
    w("MPDIST.Occupied", len(s["transit"]) > 0)
    w("MPDIST.Count", len(s["transit"]))
    for a in (1, 2, 3):
        w(f"PD{a}.In", len(s["pd_in"][a]) > 0)

    # --- trasli
    for a in (1, 2, 3):
        c = f"C{a}"
        st = s["crane"].setdefault(a, {"phase": "idle"})
        if g(f"{c}.Cmd_Reset"):
            w(f"{c}.Fault", False); w(f"{c}.Cmd_Reset", False)
        w(f"{c}.Auto", True)
        x0, y0 = g(f"{c}.PctX") or 0.0, g(f"{c}.PctY") or 0.0
        if g(f"{c}.Fault"):
            w(f"{c}.Moving", False)
            continue
        if st["phase"] == "idle":
            restore = g(f"{c}.Busy") and not g(f"{c}.Start")   # missione in corso persa (script ricaricato)
            if (g(f"{c}.Start") and not g(f"{c}.Busy")) or restore:
                st.update(phase="to_src", mission=int(g(f"{c}.Mission")), type=int(g(f"{c}.Type")),
                          src=(int(g(f"{c}.SrcSide")), int(g(f"{c}.SrcCol")), int(g(f"{c}.SrcLvl"))),
                          dst=(int(g(f"{c}.DstSide")), int(g(f"{c}.DstCol")), int(g(f"{c}.DstLvl"))),
                          out=int(g(f"{c}.DstOut") or 1), fork=0.0)
                w(f"{c}.Busy", True)
            else:
                w(f"{c}.Moving", False)
                continue
        side, col, lvl = st["src"] if st["phase"] in ("to_src", "fork_src") else st["dst"]
        tx, ty = _mp_target(col, lvl)
        x, y = _approach(x0, tx, 2.2), _approach(y0, ty, 3.5)
        w(f"{c}.PctX", x); w(f"{c}.PctY", y)
        w(f"{c}.PosX", round(x * 0.62, 2)); w(f"{c}.PosY", round(y * 0.16, 2))
        at = abs(x - tx) < 0.01 and abs(y - ty) < 0.01
        w(f"{c}.Moving", not at)
        if not at:
            continue
        fdir = -100.0 if side == 1 else 100.0
        if st["phase"] == "to_src":
            if st["type"] == 1 and col == 0 and not s["pd_in"][a]:
                st["wait"] = st.get("wait", 0) + 1
                if st["wait"] < 300:          # attende il pallet al P&D di ingresso (max 60 s in simulazione)
                    continue
            st["wait"] = 0
            st["phase"] = "fork_src"
        elif st["phase"] == "fork_src":
            target = fdir if col > 0 else -100.0
            if not st.get("ret"):
                st["fork"] = _approach(st["fork"], target, 34)
                if st["fork"] == target:
                    st["ret"] = True
                    w(f"{c}.Loaded", True)
                    if st["type"] == 1 and col == 0 and s["pd_in"][a]:
                        s["pd_in"][a].pop(0)
            else:
                st["fork"] = _approach(st["fork"], 0.0, 34)
                if st["fork"] == 0:
                    st["ret"] = False
                    st["phase"] = "to_dst"
        elif st["phase"] == "to_dst":
            st["phase"] = "fork_dst"
        elif st["phase"] == "fork_dst":
            target = fdir if col > 0 else 100.0
            if not st.get("ret"):
                st["fork"] = _approach(st["fork"], target, 34)
                if st["fork"] == target:
                    st["ret"] = True
                    w(f"{c}.Loaded", False)
                    if st["type"] == 2:
                        s["out_transit"].append({"aisle": a, "bay": f"BU{st['out']}", "eta": t + 20, "mission": st["mission"]})
            else:
                st["fork"] = _approach(st["fork"], 0.0, 34)
                if st["fork"] == 0:
                    st["ret"] = False
                    w(f"{c}.Done", st["mission"]); w(f"{c}.Busy", False)
                    w(f"{c}.Missions", int(g(f"{c}.Missions") or 0) + 1)
                    st["phase"] = "idle"
        w(f"{c}.Fork", st["fork"])

    # --- P&D di uscita → baie OUT1/OUT2
    for a in (1, 2, 3):
        w(f"PD{a}.Out", any(o["aisle"] == a and o["eta"] - t > 12 for o in s["out_transit"]))
    for o in list(s["out_transit"]):
        if t >= o["eta"] and not g(f"{o['bay']}.Occupied"):
            s["out_transit"].remove(o)
            w(f"{o['bay']}.Occupied", True)
            w(f"{o['bay']}.Mission", o["mission"])
            w(f"{o['bay']}.Count", int(g(f"{o['bay']}.Count") or 0) + 1)
            s["bay_clear"][o["bay"]] = t + 45
    for bay, until in list(s["bay_clear"].items()):
        if t >= until:
            w(f"{bay}.Occupied", False)
            del s["bay_clear"][bay]
    w("MPCOLL.Occupied", len(s["out_transit"]) > 0)
    w("MPCOLL.Count", len(s["out_transit"]))

    # --- simulazione guasti (comando da HMI)
    if g("SimMP.FaultC2"):
        w("C2.Fault", True); w("SimMP.FaultC2", False)
    return None
