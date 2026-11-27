"""Simulazione demo (script a timer): due reattori batch R1/R2.

Con la simulazione fisica attiva (tag Proc.SimActive) non fa nulla: livelli, temperature
e agitatori li calcola il layer fisico (vista "Impianto di processo").

Fisica semplificata: la valvola di carico aumenta il livello, quella di scarico
lo riduce; il riscaldatore porta la temperatura al setpoint, altrimenti si
raffredda verso l'ambiente; l'agitatore segue la velocità impostata.
"""
from _sim_util import _approach


def sim_reactor(payload, ctx):
    g, w = ctx.tag, ctx.write
    if g("Proc.SimActive"):
        return  # il layer fisico (fluidi) simula i reattori: questo script si fa da parte
    for u in ("R1", "R2"):
        lvl = g(f"{u}.Level") or 0.0
        temp = g(f"{u}.Temp") or 20.0
        if g(f"{u}.FillValve"):
            lvl += 1.2
        if g(f"{u}.DrainValve"):
            lvl -= 1.8
        lvl = max(0.0, min(100.0, lvl))
        sp = g(f"{u}.SP_Temp") or 60.0
        if g(f"{u}.Heater") and lvl > 5:
            temp = _approach(temp, sp + 0.5, 0.35)
        else:
            temp = _approach(temp, 20.0, 0.05)
        speed = _approach(g(f"{u}.AgitSpeed") or 0.0, (g(f"{u}.SP_AgitSpeed") or 0.0) if g(f"{u}.Agitator") else 0.0, 8)
        w(f"{u}.Level", round(lvl, 1))
        w(f"{u}.Temp", round(temp, 2))
        w(f"{u}.AgitSpeed", round(speed, 1))
