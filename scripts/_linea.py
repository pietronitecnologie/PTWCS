"""Modulo condiviso della linea di smistamento: codici esito, instradamento e statistiche.

I moduli che iniziano con "_" non sono funzioni richiamabili: si importano dagli
script (es. `from _linea import stats`) e il loro stato è condiviso tra gli script
dello stesso processo.
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


def reject(code, ctx, why):
    stats["rejected"] += 1
    ctx.log(f"scarto: {why}")
    return {"destination": REJECT, "result": code}
