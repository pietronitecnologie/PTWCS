from _magpallet import _mp


def mp_debug(payload, ctx):
    """Diagnostica della simulazione del magazzino pallet."""
    return {"t": _mp["t"], "pending": _mp["pending"], "next_in": _mp["next_in"], "transit": _mp["transit"],
            "pd_in": {str(k): v for k, v in _mp["pd_in"].items()},
            "crane": {str(k): {x: y for x, y in v.items() if x != "src" and x != "dst"} for k, v in _mp["crane"].items()},
            "tags": {k: ctx.tag(k) for k in ("IN1.Occupied", "IN1.ReqID", "IN1.RespID", "IN1.Destination", "IN2.Occupied", "IN2.ReqID", "IN2.RespID")}}
