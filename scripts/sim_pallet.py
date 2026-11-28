"""Simulazione PLC del magazzino pallet MP1 (script a timer).

Simula ingressi IN1/IN2 (telegramma di richiesta), rulliere di distribuzione,
3 trasli C1..C3 (telegramma missioni Start/Busy/Done) e baie BU1/BU2.
La logica di magazzino (locazioni, allocazione, missioni) è nel WCS, non qui.
"""
import random

from _magpallet import _mp, _mp_target
from _sim_util import _approach


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
            w(f"{inp}.Sku", f"SKU{int(60 * random.random() ** 2.0) + 1:03d}"); w(f"{inp}.Qty", random.randint(10, 100))
            req = (int(g(f"{inp}.ReqID") or 0) % 65000) + 1   # calcolato una sola volta: ctx.write aggiorna ctx.tags
            w(f"{inp}.Occupied", True)
            w(f"{inp}.ReqID", req)
            s["pending"][inp] = {"lu": lu, "since": t, "req": req}
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

    # --- trasli (singola o doppia UdC: campi con suffisso "2" per il secondo posto)
    # Con il layer fisico attivo (MP.SimActive) gli assi X/Y/Z li muove la fisica: questo
    # script fa il PLC di corsia (quote obiettivo, forche, sollevamento) e legge le posizioni reali.
    phys = bool(g("MP.SimActive"))
    for a in (1, 2, 3):
        c = f"C{a}"
        if phys:
            _mirror_axes(a, c, g, w)
        slots = 2 if g(f"{c}.Mission2") is not None else 1
        st = s["crane"].setdefault(a, {"phase": "idle"})
        if g(f"{c}.Cmd_Reset"):
            w(f"{c}.Fault", False); w(f"{c}.Cmd_Reset", False)
            if phys:
                w(f"AX{a}.Reset", True)
                st["ph"] = "retract"
        elif phys and g(f"AX{a}.Reset"):
            w(f"AX{a}.Reset", False)
        if phys and g(f"AX{a}.Fault") and not g(f"{c}.Fault"):
            w(f"{c}.Fault", True)   # guasto rilevato dalla fisica (interblocco, azionamento)
        w(f"{c}.Auto", True)
        x0, y0 = g(f"{c}.PctX") or 0.0, g(f"{c}.PctY") or 0.0
        if g(f"{c}.Fault"):
            w(f"{c}.Moving", False)
            continue
        if st["phase"] == "idle":
            restore = g(f"{c}.Busy") and not g(f"{c}.Start")   # missione in corso persa (script ricaricato)
            if not ((g(f"{c}.Start") and not g(f"{c}.Busy")) or restore):
                w(f"{c}.Moving", False)
                continue
            jobs = []
            for slot in range(1, slots + 1):
                sfx = "" if slot == 1 else "2"
                mid = int(g(f"{c}.Mission{sfx}") or 0)
                if mid <= 0 or (restore and int(g(f"{c}.Done{sfx}") or 0) == mid):
                    continue
                jobs.append({"slot": slot, "sfx": sfx, "mission": mid, "type": int(g(f"{c}.Type{sfx}") or 0),
                             "src": (int(g(f"{c}.SrcSide{sfx}") or 0), int(g(f"{c}.SrcCol{sfx}") or 0), int(g(f"{c}.SrcLvl{sfx}") or 0)),
                             "dst": (int(g(f"{c}.DstSide{sfx}") or 0), int(g(f"{c}.DstCol{sfx}") or 0), int(g(f"{c}.DstLvl{sfx}") or 0)),
                             "out": int(g(f"{c}.DstOut{sfx}") or 1)})
            if not jobs:
                w(f"{c}.Busy", False)
                continue
            # fermate: prima le prese, poi i depositi in ordine di vicinanza
            stops = [("pick", j) for j in jobs]
            last = _mp_target(*jobs[-1]["src"][1:])
            rest = list(jobs)
            while rest:
                j = min(rest, key=lambda j: max(abs(_mp_target(*j["dst"][1:])[0] - last[0]) / 2.2, abs(_mp_target(*j["dst"][1:])[1] - last[1]) / 3.5))
                rest.remove(j)
                stops.append(("drop", j))
                last = _mp_target(*j["dst"][1:])
            st.update(phase="run", stops=stops, i=0, fork=0.0, ret=False, wait=0)
            w(f"{c}.Busy", True)
        kind, j = st["stops"][st["i"]]
        side, col, lvl = j["src"] if kind == "pick" else j["dst"]
        if phys:
            _physical_stop(a, c, st, kind, j, side, col, lvl, s, g, w, t)
            continue
        tx, ty = _mp_target(col, lvl)
        x, y = _approach(x0, tx, 2.2), _approach(y0, ty, 3.5)
        w(f"{c}.PctX", x); w(f"{c}.PctY", y)
        w(f"{c}.PosX", round(x * 0.62, 2)); w(f"{c}.PosY", round(y * 0.16, 2))
        at = abs(x - tx) < 0.01 and abs(y - ty) < 0.01
        w(f"{c}.Moving", not at)
        if not at:
            continue
        if kind == "pick" and j["type"] == 1 and col == 0 and not st["ret"] and st["fork"] == 0 and not s["pd_in"][a]:
            st["wait"] += 1
            if st["wait"] < 300:              # attende il pallet al P&D di ingresso (max 60 s in simulazione)
                continue
        st["wait"] = 0
        target = (-100.0 if side == 1 else 100.0) if col > 0 else (-100.0 if kind == "pick" else 100.0)
        if not st["ret"]:
            st["fork"] = _approach(st["fork"], target, 34)
            if st["fork"] == target:
                st["ret"] = True
                if kind == "pick":
                    w(f"{c}.Loaded{j['sfx']}", True)
                    if j["type"] == 1 and col == 0 and s["pd_in"][a]:
                        s["pd_in"][a].pop(0)
                else:
                    w(f"{c}.Loaded{j['sfx']}", False)
                    if j["type"] == 2:
                        s["out_transit"].append({"aisle": a, "bay": f"BU{j['out']}", "eta": t + 20, "mission": j["mission"]})
        else:
            st["fork"] = _approach(st["fork"], 0.0, 34)
            if st["fork"] == 0:
                st["ret"] = False
                if kind == "drop":
                    w(f"{c}.Done{j['sfx']}", j["mission"])
                    w(f"{c}.Missions", int(g(f"{c}.Missions") or 0) + 1)
                st["i"] += 1
                if st["i"] >= len(st["stops"]):
                    w(f"{c}.Busy", False)
                    st["phase"] = "idle"
        w(f"{c}.Fork{j['sfx']}", st["fork"])

    # --- P&D di uscita → baie BU1/BU2
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


# ------------------------------------------------------------------ traslo con assi (layer fisico)
PITCH_X, PITCH_Y, Y0, LIFT, REACH = 1240.0, 1778.0, 100.0, 50.0, 1300.0


def _mirror_axes(a, c, g, w):
    """Posizioni reali degli assi → tag del telegramma e della grafica esistente."""
    x, y, z = g(f"AX{a}.ActX") or 0.0, g(f"AX{a}.ActY") or Y0, g(f"AX{a}.ActZ") or 0.0
    w(f"{c}.PosX", round(x / 1000.0, 2)); w(f"{c}.PosY", round((y - Y0) / 1000.0, 2))
    w(f"{c}.PctX", round(x / (50 * PITCH_X) * 100.0, 2)); w(f"{c}.PctY", round(max(0.0, y - Y0) / (9 * PITCH_Y) * 100.0, 2))
    w(f"{c}.Moving", bool(g(f"AX{a}.Moving")))


def _physical_stop(a, c, st, kind, j, side, col, lvl, s, g, w, t):
    """Una fermata (presa o deposito) eseguita comandando gli assi:
    forche dentro → X/Y alla cella (sotto per prendere, sopra per depositare) → forche fuori →
    sollevamento / abbassamento → forche dentro → verifica del carico con il sensore del carro."""
    ax = f"AX{a}"
    pick = kind == "pick"
    xc, yc = col * PITCH_X, Y0 + max(lvl - 1, 0) * PITCH_Y
    y_a, y_b = (yc - LIFT, yc + LIFT) if pick else (yc + LIFT, yc - LIFT)
    zs = (-1.0 if side == 1 else 1.0) if col > 0 else (-1.0 if pick else 1.0)
    inpos = lambda *axes: all(g(f"{ax}.InPos{x}") for x in axes)
    ph = st.setdefault("ph", "retract")
    if g(f"{c}.Fault"):
        return
    if ph == "retract":
        w(f"{ax}.TgtZ", 0.0)
        if inpos("Z") and abs(g(f"{ax}.ActZ") or 0.0) < 2:
            st["ph"] = "move"
    elif ph == "move":
        w(f"{ax}.TgtX", xc); w(f"{ax}.TgtY", y_a); w(f"{ax}.TgtZ", 0.0)
        if inpos("X", "Y") and abs((g(f"{ax}.ActX") or 0) - xc) < 2 and abs((g(f"{ax}.ActY") or 0) - y_a) < 2:
            # presa al P&D di ingresso: attende il pallet (max 60 s)
            if pick and j["type"] == 1 and col == 0 and not s["pd_in"][a]:
                st["wait"] = st.get("wait", 0) + 1
                if st["wait"] < 300:
                    return
            st["wait"] = 0
            st["ph"] = "out"
    elif ph == "out":
        w(f"{ax}.TgtZ", zs * REACH)
        if inpos("Z") and abs(abs(g(f"{ax}.ActZ") or 0.0) - REACH) < 2:
            st["ph"] = "lift"
    elif ph == "lift":
        w(f"{ax}.TgtY", y_b)
        if inpos("Y") and abs((g(f"{ax}.ActY") or 0) - y_b) < 2:
            st["ph"] = "in"
    elif ph == "in":
        w(f"{ax}.TgtZ", 0.0)
        if inpos("Z") and abs(g(f"{ax}.ActZ") or 0.0) < 2:
            st["ph"] = "move"
            loaded = bool(g(f"{ax}.Load"))
            if pick:
                if not loaded:
                    w(f"{c}.Fault", True)   # cella vuota: il WCS segnala il guasto, l'operatore verifica
                    st["ph"] = "retract"
                    return
                w(f"{c}.Loaded{j['sfx']}", True)
                if j["type"] == 1 and col == 0 and s["pd_in"][a]:
                    s["pd_in"][a].pop(0)
            else:
                w(f"{c}.Loaded{j['sfx']}", False)
                if j["type"] == 2:
                    s["out_transit"].append({"aisle": a, "bay": f"BU{j['out']}", "eta": t + 20, "mission": j["mission"]})
                w(f"{c}.Done{j['sfx']}", j["mission"])
                w(f"{c}.Missions", int(g(f"{c}.Missions") or 0) + 1)
            st["i"] += 1
            if st["i"] >= len(st["stops"]):
                w(f"{c}.Busy", False)
                st["phase"] = "idle"
    w(f"{c}.Fork{j['sfx']}", round((g(f"{ax}.ActZ") or 0.0) / REACH * 100.0, 1))
