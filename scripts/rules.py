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
