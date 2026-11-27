"""Hub – sorter Sud: scivolo 1..6 in base al barcode, 99 = scivolo scarti."""
from _hub import OK, UNKNOWN_DEST, check, reject, route, stats


def hub_sort_sud(req, ctx):
    stats["sud"] += 1
    barcode, rej = check(req, ctx, "sorter Sud")
    if rej:
        return rej
    sorter, chute = route(barcode)
    if sorter != 2:
        return reject(UNKNOWN_DEST, ctx, f"sorter Sud: {barcode} appartiene all'altro sorter")
    return {"destination": chute, "result": OK}
