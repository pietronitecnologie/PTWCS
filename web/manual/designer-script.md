# Designer – Script Python

Il workspace **🐍 Script** gestisce le funzioni Python del progetto: decisioni, eventi HMI, script a timer.

## Un file per funzione

Gli script stanno nella cartella indicata da `scripting.module` in `plant.yaml` (nella demo `scripts/`).

| File | Contenuto |
|---|---|
| `nome.py` | deve definire `def nome(payload, ctx)`: è la funzione richiamabile da punti di decisione, timer ed eventi delle viste |
| `_nome.py` | **modulo condiviso**: costanti, funzioni di supporto e stato comune, importabili con `from _nome import x` |

Lo stato definito a livello di modulo (es. `stats = {...}` in `_linea.py`) resta in memoria tra le chiamate ed è **condiviso** da tutti gli script che lo importano. Si azzera al salvataggio o alla ricarica degli script.

Le funzioni dei punti di decisione girano in un processo Python separato da quello degli eventi HMI e dei timer: uno script a timer lento non rallenta mai le decisioni.

## Menu degli script

Il pannello sinistro elenca gli script raggruppati per **uso**, ricavato automaticamente dalla configurazione:

| Gruppo | Contiene |
|---|---|
| 🔀 Punti di decisione | funzioni usate da un punto di decisione |
| 🖱 Eventi HMI | funzioni richiamate da azioni *Esegui script* delle viste |
| ⏲ Script a timer | funzioni dei timer |
| ○ Non collegati | funzioni non usate da nessuna parte |
| 📦 Moduli condivisi | file `_nome.py` |

- `!` rosso = errore di caricamento; il passaggio del mouse mostra la documentazione e chi usa lo script.
- **+ Nuovo script**: scegli il tipo (punto di decisione, evento HMI, timer, modulo condiviso) e il nome; il file nasce da un modello pronto.
- **Tasto destro**: apri, duplica, elimina. Le versioni eliminate o sovrascritte restano in `scripts/.history/`.
- **⟳**: ricarica tutti gli script.

## Editor

Il codice è **evidenziato**: parole chiave, stringhe, f-string, numeri, commenti, funzioni, builtin e `ctx`.

| Tasti | Azione |
|---|---|
| **Ctrl+Spazio** | suggerimenti |
| Tab / Maiusc+Tab | indenta / rientra (anche su più righe selezionate) |
| Invio | a capo mantenendo il rientro (+4 dopo `:`) |
| Ctrl+/ | commenta / decommenta le righe |
| Ctrl+D | duplica la riga |
| Ctrl+S | controlla, salva e ricarica |
| Ctrl+Invio | esegue il test |
| Ctrl+Z / Ctrl+Y | annulla / ripeti |

Parentesi e virgolette si chiudono automaticamente.

### Suggerimenti

Appaiono durante la scrittura, oppure con Ctrl+Spazio:
- dopo `ctx.` → API del contesto, con firma e descrizione;
- dentro `ctx.tag("…")` o `ctx.write("…")` → **nomi dei tag** del progetto, con tipo e descrizione;
- dopo `from ` / `import ` → moduli condivisi e librerie standard;
- dopo `from _modulo import ` → simboli definiti nel modulo;
- altrimenti → snippet, parole chiave, builtin e nomi già usati nel file.

↑ ↓ per scegliere, Invio o Tab per inserire, Esc per chiudere.

### Snippet

Il pulsante **✂ Snippet** inserisce modelli pronti, che si possono anche scrivere per nome (es. `evento` + Invio):
- funzione punto di decisione, evento HMI, script a timer con stato;
- leggi tag, scrivi tag, log, messaggio, naviga, apri faceplate, esito di scarto;
- fronte di salita di un tag, instradamento per prefisso, import da modulo condiviso;
- if/else, for, try/except, ciclo su più dispositivi.

### Controllo in tempo reale

Mentre scrivi, il codice viene controllato:
- **errori di sintassi** (● rosso sulla riga): impediscono il salvataggio;
- **funzione mancante**: il file `nome.py` deve definire `def nome(...)`;
- **avvisi** (▲ giallo): nomi non definiti, import non usati.

L'elenco sotto l'editor è cliccabile e porta alla riga.

### Salvataggio

**Salva**:
1. ricontrolla il codice;
2. salva una copia della versione precedente in `.history`;
3. ricarica gli script nei processi Python.

Se un altro file non si carica, il salvataggio viene segnalato con l'errore.

## Test

Il riquadro **Test** esegue la funzione in un **interprete isolato**:
- usa il codice dell'editor, anche **non salvato**, insieme agli altri script;
- le scritture **non** vengono inviate al PLC;
- il codice in produzione non viene toccato.

| Campo | Significato |
|---|---|
| **Payload / req (JSON)** | argomento passato alla funzione. Per i punti di decisione è precompilato con gli input del telegramma |
| **Tag simulati (JSON)** | valori che sostituiscono quelli live, es. `{"Line.EStop": true}`. **⇩ Tag usati** inserisce i tag letti dallo script con il loro valore attuale |
| **▶ Esegui test** | esegue (anche con Ctrl+Invio) |

Il risultato contiene:
- **esito** e durata;
- **valore restituito**;
- **scritture richieste** (non applicate);
- messaggi di **ctx.log**;
- output delle **print()**.

In caso di errore compaiono l'eccezione, il **link alla riga** del file e la traccia completa. Payload e tag del test vengono ricordati per ogni script.

## API del contesto (`ctx`)

| Membro | Descrizione |
|---|---|
| `ctx.tag(nome, default=None)` | valore di un tag (snapshot al momento della chiamata) |
| `ctx.write(nome, valore)` | scrive un tag sul PLC (eseguita dal WCS a fine funzione) |
| `ctx.log(messaggio)` | messaggio nel log eventi |
| `ctx.tags` | dict di tutti i tag |
| `ctx.now` | timestamp (secondi epoch) |
| `ctx.point` | punto di decisione o evento che ha generato la chiamata |
| `ctx.user` | utente (eventi HMI) |

## Valori di ritorno

| Tipo di script | Ritorna |
|---|---|
| Punto di decisione | dict delle uscite, es. `{"destination": 2, "result": 0}`; `None` = nessuna decisione (si passa al fallback) |
| Evento HMI | `{"message": "testo"}` (messaggio all'operatore), `{"navigate": "vista"}`, `{"popup": "fp_x", "params": {"device": "CV1"}}` |
| Script a timer | niente; agisce con `ctx.write` |

## Esempio

```python
"""Punto di decisione SC1: destinazione del collo letto dallo scanner."""
from _linea import ROUTES, stats, reject, NOREAD, OK


def decide_sc1(req, ctx):
    stats["total"] += 1
    barcode = (req.get("barcode") or "").strip()
    if not barcode or barcode == "NOREAD":
        return reject(NOREAD, ctx, "lettura fallita")
    dest = next((d for p, d in ROUTES.items() if barcode.startswith(p)), None)
    return {"destination": dest or 99, "result": OK}
```
