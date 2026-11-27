"""Hub – pre-smistatore SC0: sorter Nord (1), Sud (2) o scarto (99)."""
from _hub import OK, check, route, stats


def hub_presort(req, ctx):
    stats["presort"] += 1
    barcode, rej = check(req, ctx, "pre-smistamento")
    if rej:
        return rej
    return {"destination": route(barcode)[0], "result": OK}
