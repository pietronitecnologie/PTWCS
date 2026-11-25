def start_line(payload, ctx):
    """Avvio linea in sequenza (valle -> monte)."""
    if ctx.tag("Line.EStop"):
        return {"message": "Impossibile avviare: emergenza attiva"}
    for cv in ("CV3", "CV2", "CV1"):
        ctx.write(f"{cv}.Run", True)
    ctx.log("linea avviata da " + str(ctx.user))
    return {"message": "Linea avviata"}
