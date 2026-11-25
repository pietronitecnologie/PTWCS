"""Punto di decisione SC1: destinazione del collo letto dallo scanner.

req: barcode, weight (+ request_id). Ritorna {"destination", "result"}.
"""
from _linea import OK, NOREAD, OVERWEIGHT, UNKNOWN_DEST, ROUTES, stats, reject


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
