"""Hub – sorter Nord: scivolo 1..6 in base al barcode, 99 = scivolo scarti."""
from _hub import OK, UNKNOWN_DEST, check, reject, route, stats


def hub_sort_nord(req, ctx):
    stats["nord"] += 1
    barcode, rej = check(req, ctx, "sorter Nord")
    if rej:
        return rej
    sorter, chute = route(barcode)
    if sorter != 1:
        return reject(UNKNOWN_DEST, ctx, f"sorter Nord: {barcode} appartiene all'altro sorter")
    return {"destination": chute, "result": OK}
