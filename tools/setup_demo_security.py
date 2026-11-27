#!/usr/bin/env python3
"""Attiva login e permessi nel progetto demo e crea gli utenti di prova.

Uso (con il WCS avviato e la sicurezza ancora disattivata, oppure con
PTWCS_USER/PTWCS_PASSWORD di un amministratore):
    python3 tools/setup_demo_security.py [http://127.0.0.1:8080]

Utenti demo (password = nome utente + "123", da cambiare in produzione):
    operatore     ruolo operatore      comandi, allarmi, missioni WCS
    manutentore   ruolo manutentore    + setpoint riservati, WCS avanzato, Designer in lettura
    progettista   ruolo progettista    Designer completo
L'utente admin (password iniziale "admin", cambio obbligatorio) esiste già.
Senza login il runtime è visibile in sola lettura (ruolo visualizzatore).
"""
import json
import os
import sys
import urllib.request

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://127.0.0.1:8080"
COOKIE = None


def call(method, path, body=None):
    global COOKIE
    req = urllib.request.Request(BASE + path, method=method, data=json.dumps(body).encode() if body is not None else None,
                                 headers={"Content-Type": "application/json", **({"Cookie": COOKIE} if COOKIE else {})})
    with urllib.request.urlopen(req) as r:
        sc = r.headers.get("Set-Cookie")
        if sc:
            COOKIE = sc.split(";")[0]
        return json.loads(r.read() or b"null")


me = call("GET", "/api/auth/me")
if me["enabled"] and not me.get("all"):
    call("POST", "/api/auth/login", {"username": os.environ.get("PTWCS_USER", "admin"), "password": os.environ["PTWCS_PASSWORD"]})

for user, role, name in [("operatore", "operatore", "Operatore di turno"), ("manutentore", "manutentore", "Manutenzione"),
                         ("progettista", "progettista", "Progettista impianto")]:
    call("PUT", f"/api/users/{user}", {"full_name": name, "roles": [role], "enabled": True, "password": user + "123", "must_change": False})
    print("utente", user)

p = call("GET", "/api/project")["project"]
# setpoint riservati alla manutenzione
reserved = ["manutentore", "progettista", "amministratore"]
for t in p["tags"]:
    if t["name"] in ("WCS.MaxWeight", "WCS.Out3Enabled", "MP.MaxWeight", "MP.MaxHeight", "SimMP.FaultC2"):
        t["write_roles"] = reserved
p["security"] = {**p.get("security", {}), "enabled": True, "anonymous_role": "visualizzatore", "session_minutes": 720, "idle_logout_minutes": 0}
print("progetto:", call("PUT", "/api/project", p).get("ok"))
