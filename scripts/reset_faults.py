"""Evento HMI."""


def reset_faults(payload, ctx):
    ctx.write("Line.ResetFaults", True)
    return {"message": "Reset guasti inviato"}
