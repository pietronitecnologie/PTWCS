# Edizioni e distribuzione

Il runtime PTWCS si distribuisce in più **edizioni**: ogni pacchetto contiene solo i moduli acquistati o necessari all'impianto. Il nucleo **Tag** è sempre presente.

| Modulo | Contenuto |
|---|---|
| **Tag** (sempre) | driver PLC, tag e UDT, allarmi e aree, storico e trend, script Python, sicurezza, audit |
| **SCADA** | viste HMI, simboli e faceplate, processo ISA-88 (fasi, ricette, batch) |
| **WCS** | punti di decisione (request/reply), regole di smistamento, classi e WMS, magazzini automatici |
| **Simulazione** | layer fisico sul layout delle viste (nastri, fluidi, trasli). Richiede SCADA |

## Edizioni

| Edizione | Nome | Moduli |
|---|---|---|
| Tag | `tag` | solo nucleo: gateway, storico, allarmi |
| Tag + SCADA | `scada` | supervisione e processo |
| Tag + WCS | `wcs` | controllo dei flussi e dei magazzini, senza grafica di impianto |
| Tag + SCADA + Simulazione | `scada-sim` | supervisione e collaudo virtuale |
| Tag + SCADA + WCS | `scada-wcs` | supervisione e controllo |
| Completa | `full` | tutto |

L'edizione attiva si vede nel runtime, accanto al nome dell'impianto, e in **Diagnostica → Edizione**.

## Cosa cambia nelle edizioni ridotte

Le funzioni escluse spariscono dall'interfaccia e il server non le esegue:

| Senza… | Runtime | Designer | Server |
|---|---|---|---|
| **SCADA** | niente viste né *Processo*: la pagina iniziale è *WCS* (o *Decisioni*) | niente *Viste*, *Processo*, *Fisica* | sequenziatore fermo; API di viste, simboli, processo e ricette → 404 |
| **WCS** | niente *WCS* e *Decisioni* | niente *Smistamento* né *Progetto → Magazzini* | punti di decisione e magazzini non partono; API `wh`, `rules`, `sorting`, `decisions` → 404 |
| **Simulazione** | niente *Simulazione* | niente *Fisica* | motore fisico non avviato; API `physics` → 404 |

Il progetto non viene modificato: tag, punti di decisione o viste presenti in `project.yaml` restano e tornano attivi con un'edizione che li include. I tag dei telegrammi e dei dispositivi continuano a essere letti dai driver.

## Scegliere l'edizione

In `plant.yaml`:

```yaml
edition: scada-sim      # tag | scada | wcs | scada-sim | scada-wcs | full
plant:
  name: Linea 3
```

Sono accettate anche le forme `tag+scada`, `Tag + WCS`, `scada+wcs+sim`. Senza la voce `edition` vale l'edizione del pacchetto (o `full` nella build di sviluppo).

## Creare un pacchetto

Dalla cartella del progetto:

```
tools/dist.sh <edizione> [--demo] [--client] [--out DIR]
```

Esempi:
- `tools/dist.sh scada`: pacchetto Tag + SCADA con un impianto vuoto;
- `tools/dist.sh wcs --demo`: Tag + WCS con l'impianto demo;
- `tools/dist.sh scada-sim --client`: Tag + SCADA + Simulazione con il client desktop.

Il risultato è `dist/ptwcs-<edizione>-<versione>-<sistema>-<arch>.tar.gz` con:

| Percorso | Contenuto |
|---|---|
| `bin/wcs_scada` | runtime, con l'edizione **fissata nel programma** |
| `bin/ptwcs-client` | client desktop (con `--client`) |
| `config/plant.yaml` | impianto, driver ed `edition` |
| `config/project.yaml` | progetto (vuoto o demo) |
| `web/`, `stdlib/` | interfaccia e libreria standard |
| `scripts/` | funzioni Python |
| `avvia.sh`, `avvia.cmd` | avvio |
| `ptwcs.service` | servizio systemd (installazione in `/opt/ptwcs`) |
| `LEGGIMI.txt` | istruzioni e moduli inclusi |

L'edizione è compilata nel binario (variabile `PTWCS_EDITION`): nel `plant.yaml` del cliente si possono **togliere** moduli ma non **aggiungerne**. Se il file chiede un modulo non incluso, il runtime parte comunque con i soli moduli del pacchetto e lo segnala nel log e in *Diagnostica*.

La compilazione usa la cartella `target/dist`, separata dalla build di sviluppo.

## Primo avvio

Il pacchetto non contiene il database: al primo avvio viene creato l'utente `admin` con password `admin`, da cambiare al primo accesso (oppure con la password della variabile `PTWCS_ADMIN_PASSWORD`). Gli altri utenti si creano dal Designer → Progetto → Sicurezza.
