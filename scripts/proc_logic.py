"""Logica di servizio dell'impianto di processo (demo fluidi): fa la parte del PLC.

- reintegro del serbatoio acqua T1 dalla rete (isteresi 40–85 %);
- pompa P1 in marcia quando una valvola di carico dei reattori è aperta e T1 non è vuoto;
- svuotamento del serbatoio prodotto T2 verso il carico autobotte (sopra 60 %, fino all'8 %).
Le fasi ISA-88 (Carico, Riscaldo, Agitazione, Scarico) comandano valvole, riscaldatori e agitatori.
"""


def proc_logic(payload, ctx):
    g, w = ctx.tag, ctx.write
    if not g("Proc.Auto", True):
        return
    t1 = g("Proc.T1.Level") or 0.0
    t2 = g("Proc.T2.Level") or 0.0
    if t1 < 40 and not g("Proc.V_IN"):
        w("Proc.V_IN", True)
    elif t1 > 85 and g("Proc.V_IN"):
        w("Proc.V_IN", False)
    need = bool((g("R1.FillValve") or g("R2.FillValve")) and t1 > 3)
    if bool(g("Proc.P1.Run")) != need:
        w("Proc.P1.Run", need)
    if t2 > 60 and not g("Proc.P2.Run"):
        w("Proc.V_OUT", True)
        w("Proc.P2.Run", True)
    elif t2 < 8 and g("Proc.P2.Run"):
        w("Proc.P2.Run", False)
        w("Proc.V_OUT", False)
