"""Stato condiviso della simulazione del magazzino pallet MP1 (usato da sim_pallet e mp_debug)."""
import time as _time

_mp = {"t": 0, "seq": int(_time.time()) % 90000, "next_in": {"IN1": 10, "IN2": 40}, "pending": {}, "transit": [],
       "pd_in": {1: [], 2: [], 3: []}, "out_transit": [], "bay_clear": {}, "crane": {}}
MP_COLS, MP_LEVELS = 50, 10


def _mp_target(col, lvl):
    return (col / MP_COLS * 100.0, 0.0 if lvl <= 0 else (lvl - 1) / (MP_LEVELS - 1) * 100.0)
