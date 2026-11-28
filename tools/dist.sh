#!/usr/bin/env bash
# Crea un pacchetto di distribuzione del runtime PTWCS per un'edizione.
#
#   tools/dist.sh <edizione> [--demo] [--client] [--out DIR]
#
# Edizioni:  tag | scada | wcs | scada-sim | scada-wcs | full
#   tag        solo tag: driver, allarmi, storico, trend, script (gateway/historian)
#   scada      Tag + SCADA: viste HMI, faceplate, processo batch
#   wcs        Tag + WCS: decisioni, smistamento, magazzini automatici
#   scada-sim  Tag + SCADA + simulazione fisica
#   scada-wcs  Tag + SCADA + WCS
#   full       tutto
#
# L'edizione è fissata nel binario (PTWCS_EDITION): il plant.yaml del cliente può solo
# ridurre i moduli. --demo include l'impianto demo (config/ del repository) al posto
# del modello vuoto; --client aggiunge il client desktop PTWCS Client.
set -euo pipefail

usage() { sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'; exit 1; }
[ $# -ge 1 ] || usage
ED="$1"; shift
DEMO=0; CLIENT=0; OUT=dist
while [ $# -gt 0 ]; do
  case "$1" in
    --demo) DEMO=1 ;;
    --client) CLIENT=1 ;;
    --out) OUT="$2"; shift ;;
    *) usage ;;
  esac
  shift
done
case "$ED" in
  tag|scada|wcs|scada-sim|scada-wcs|full) ;;
  *) echo "edizione '$ED' non valida"; usage ;;
esac

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OS=$(uname -s | tr '[:upper:]' '[:lower:]'); ARCH=$(uname -m)
EXE=""; case "$OS" in mingw*|msys*|cygwin*) OS=windows; EXE=.exe ;; esac
NAME="ptwcs-$ED-$VER-$OS-$ARCH"
PKG="$OUT/$NAME"
TARGET="$ROOT/target/dist"   # cartella separata: non tocca la build di sviluppo

echo "▶ compilazione edizione $ED"
PTWCS_EDITION="$ED" CARGO_TARGET_DIR="$TARGET" cargo build --release --bin wcs_scada
if [ $CLIENT = 1 ]; then
  CARGO_TARGET_DIR="$TARGET" cargo build --release -p ptwcs-client
fi

rm -rf "$PKG"
mkdir -p "$PKG/bin" "$PKG/config"
cp "$TARGET/release/wcs_scada$EXE" "$PKG/bin/"
[ $CLIENT = 1 ] && cp "$TARGET/release/ptwcs-client$EXE" "$PKG/bin/"
cp -r web stdlib "$PKG/"

has() { case "$ED" in full) return 0 ;; *"$1"*) return 0 ;; *) return 1 ;; esac; }

if [ $DEMO = 1 ]; then
  # impianto demo completo; l'edizione limita comunque i moduli attivi
  cp -r config/. "$PKG/config/"
  cp -r scripts "$PKG/scripts"
  rm -rf "$PKG/config/backups" "$PKG/scripts/.history"
  find "$PKG" \( -name '*.db' -o -name '*.db-*' -o -name '*.bak' -o -name '__pycache__' \) -prune -exec rm -rf {} +
  { echo "edition: $ED"; grep -v '^edition:' "$PKG/config/plant.yaml"; } > "$PKG/config/plant.yaml.new"
  mv "$PKG/config/plant.yaml.new" "$PKG/config/plant.yaml"
else
  mkdir -p "$PKG/config/views" "$PKG/config/symbols" "$PKG/config/rules" "$PKG/scripts"
  cat > "$PKG/config/plant.yaml" <<EOF
# PTWCS – file di impianto. Il progetto (tag, UDT, aree, allarmi…) è in project.yaml
# e si modifica dal Designer: http://<server>:8080/designer.html
edition: $ED               # moduli attivi; il pacchetto ($ED) non può essere ampliato

plant:
  name: Nuovo impianto
  description: ""

server:
  bind: 0.0.0.0:8080
  database: ptwcs.db
  web_root: ../web
  library_dir: ../stdlib
  views_dir: views
  symbols_dir: symbols
  designer: true             # in produzione: false oppure login con ruoli
  retention_days: 30

scripting:
  python: python3
  module: ../scripts         # un file per funzione: nome.py → def nome(payload, ctx)

drivers:
  # - name: PLC1
  #   protocol: s7           # s7 | modbus | opcua | eip | sim
  #   host: 192.168.0.10
  #   rack: 0
  #   slot: 1
  #   poll_ms: 100
  - name: INTERNO            # variabili interne (setpoint, modalità…)
    protocol: sim
    poll_ms: 250

project_file: project.yaml
rules_dir: rules
EOF
  printf 'tags: []\n' > "$PKG/config/project.yaml"
  if has scada; then
    cat > "$PKG/config/views/home.json" <<'EOF'
{"title": "Pagina iniziale", "width": 1600, "height": 900, "background": "#0b1220", "home": true,
 "components": [{"id": "title", "name": "title", "type": "label", "x": 40, "y": 30, "w": 900, "h": 40, "rotation": 0,
   "props": {"text": "Nuovo impianto", "fontSize": 28, "bold": true}, "bindings": {}, "events": {}}]}
EOF
  fi
  cat > "$PKG/scripts/esempio.py" <<'EOF'
def esempio(payload, ctx):
    """Funzione di esempio: ctx['tags'] contiene i valori correnti dei tag."""
    return {"ok": True}
EOF
fi

LABEL=$(case "$ED" in tag) echo "Tag";; scada) echo "Tag + SCADA";; wcs) echo "Tag + WCS";; scada-sim) echo "Tag + SCADA + Simulazione";; scada-wcs) echo "Tag + SCADA + WCS";; full) echo "Completa (Tag + SCADA + WCS + Simulazione)";; esac)

cat > "$PKG/avvia.sh" <<'EOF'
#!/usr/bin/env bash
# Avvia il runtime PTWCS con la configurazione di questa cartella.
cd "$(dirname "$0")/config" && exec ../bin/wcs_scada plant.yaml "$@"
EOF
chmod +x "$PKG/avvia.sh"
cat > "$PKG/avvia.cmd" <<'EOF'
@echo off
cd /d "%~dp0config"
"..\bin\wcs_scada.exe" plant.yaml
EOF
cat > "$PKG/ptwcs.service" <<EOF
# Servizio systemd: copia la cartella in /opt/ptwcs, poi
#   sudo cp /opt/ptwcs/ptwcs.service /etc/systemd/system/ && sudo systemctl enable --now ptwcs
[Unit]
Description=PTWCS runtime ($LABEL)
After=network-online.target

[Service]
WorkingDirectory=/opt/ptwcs/config
ExecStart=/opt/ptwcs/bin/wcs_scada plant.yaml
Restart=always
RestartSec=3
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF
cat > "$PKG/LEGGIMI.txt" <<EOF
PTWCS $VER – edizione $LABEL ($ED)
=================================================

Avvio:     ./avvia.sh   (Windows: avvia.cmd)
Runtime:   http://localhost:8080/
Designer:  http://localhost:8080/designer.html
Manuale:   http://localhost:8080/manual.html

Contenuto
  bin/        runtime (wcs_scada)$( [ $CLIENT = 1 ] && echo " e client desktop (ptwcs-client)")
  config/     plant.yaml (impianto, driver, edizione) e project.yaml (progetto)
  web/        interfaccia runtime e Designer
  stdlib/     libreria standard di simboli, faceplate e tipi UDT
  scripts/    funzioni Python (richiede python3)

Moduli inclusi
  Tag          sì   driver, tag, allarmi, storico, trend, script, sicurezza, audit
  SCADA        $(has scada && echo "sì " || echo "no ")  viste HMI, faceplate, processo batch
  WCS          $(has wcs && echo "sì " || echo "no ")  punti di decisione, smistamento, magazzini
  Simulazione  $(has sim && echo "sì " || echo "no ")  fisica sul layout delle viste

L'edizione è fissata nel programma: in config/plant.yaml la voce 'edition' può
togliere moduli ma non aggiungerne. Le funzioni escluse non compaiono né nel
runtime né nel Designer e le relative API rispondono 404.

Primo accesso: utente admin, password admin (cambio obbligatorio), oppure la
password indicata nella variabile PTWCS_ADMIN_PASSWORD al primo avvio.
EOF

mkdir -p "$OUT"
tar -C "$OUT" -czf "$OUT/$NAME.tar.gz" "$NAME"
echo "✓ $OUT/$NAME.tar.gz ($(du -h "$OUT/$NAME.tar.gz" | cut -f1))"
