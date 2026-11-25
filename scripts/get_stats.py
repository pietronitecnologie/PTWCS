"""Evento HMI: statistiche della linea."""
from _linea import stats


def get_stats(payload, ctx):
    total = stats["total"] or 1
    return {"message": f"Colli: {stats['total']} · scartati: {stats['rejected']} "
                       f"({100 * stats['rejected'] / total:.1f}%)"}
