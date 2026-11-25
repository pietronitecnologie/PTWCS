"""Funzioni di supporto per le simulazioni demo."""


def _approach(cur, target, step):
    cur = cur or 0
    return target if abs(target - cur) <= step else cur + step * (1 if target > cur else -1)
