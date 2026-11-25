"""Evento HMI."""


def stop_line(payload, ctx):
    for cv in ("CV1", "CV2", "CV3"):
        ctx.write(f"{cv}.Run", False)
    return {"message": "Linea fermata"}
