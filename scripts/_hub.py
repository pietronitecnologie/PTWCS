"""Modulo condiviso dell'hub di smistamento: instradamento per pre-smistatore e sorter.

Barcode nel formato PAESE-CITTÀ-NUMERO (es. IT-MI-004512). Il pre-smistatore manda il
collo al sorter Nord (1) o Sud (2); ogni sorter lo assegna a uno scivolo 1..6.
Mancata lettura, destinazione sconosciuta o sovrappeso → 99 (scarto).
"""

OK, NOREAD, OVERWEIGHT, UNKNOWN_DEST = 0, 1, 2, 3
REJECT = 99
MAX_WEIGHT = 3000  # g/10 = 30 kg

# prefisso → (sorter, scivolo)
ROUTES = {
    "IT-MI": (1, 1), "IT-TO": (1, 2), "IT-BO": (1, 3), "IT-VE": (1, 4), "IT-GE": (1, 5),
    "DE": (1, 6), "FR": (1, 6), "CH": (1, 6),
    "IT-RM": (2, 1), "IT-NA": (2, 2), "IT-FI": (2, 3), "IT-BA": (2, 4), "IT-PA": (2, 5),
    "ES": (2, 6), "GR": (2, 6),
}

stats = {"presort": 0, "nord": 0, "sud": 0, "rejected": 0}


def route(barcode):
    return next((r for p, r in ROUTES.items() if barcode.startswith(p)), None)


def check(req, ctx, where):
    """Controlli comuni: ritorna (barcode, None) oppure (None, risposta di scarto)."""
    barcode = (req.get("barcode") or "").strip()
    if not barcode or barcode == "NOREAD":
        return None, reject(NOREAD, ctx, f"{where}: lettura fallita")
    if (req.get("weight") or 0) > MAX_WEIGHT:
        return None, reject(OVERWEIGHT, ctx, f"{where}: {barcode} sovrappeso")
    if route(barcode) is None:
        return None, reject(UNKNOWN_DEST, ctx, f"{where}: {barcode} destinazione sconosciuta")
    return barcode, None


def reject(code, ctx, why):
    stats["rejected"] += 1
    ctx.log(f"scarto: {why}")
    return {"destination": REJECT, "result": code}
