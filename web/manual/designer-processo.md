# Designer – Processo

Il workspace **⚗ Processo** configura i **tipi di ricetta** e le **sequenze** a passi. Le modifiche si attivano con **Salva progetto**.

Le ricette vere e proprie, cioè gli insiemi di valori, sono dati di produzione: si creano e si modificano dal runtime (**Processo → Ricette**) e stanno nel database del server.

## Tipi di ricetta

Un tipo di ricetta descrive **quali parametri** ha una ricetta e **su quale tag** viene scritto ciascuno quando la ricetta è caricata.

| Campo | Significato | Esempio |
|---|---|---|
| **Nome** | identificativo, usato nelle sequenze (azione *Carica ricetta*) | `Batch` |
| **Descrizione** | testo mostrato nel runtime | |
| **Area** | area di appartenenza | `Processo` |
| **Unità** | unità identiche su cui caricare la stessa ricetta; nei tag si usa `{unit}`. Vuoto = unità singola | `R1, R2` |

### Parametri

| Colonna | Significato | Esempio |
|---|---|---|
| **Parametro** | nome visualizzato nella ricetta | `Temperatura` |
| **Tag** | tag su cui scrivere; può contenere `{unit}` | `{unit}.SP_Temp` |
| **Unità** | unità di misura | `°C` |
| **Min / Max** | limiti, verificati al salvataggio e al caricamento della ricetta | `25` / `90` |
| **Default** | valore per le nuove ricette e per i parametri mancanti | `60` |
| **Descrizione** | aiuto per l'operatore | |

I tag dei parametri devono essere **scrivibili**. Se un tag ha *Ruoli scrittura*, solo quei ruoli possono caricare la ricetta.

## Sequenze

Una sequenza è una successione di **passi**, in stile SFC/GRAFCET:
1. entrando in un passo, la sequenza esegue le sue **azioni**;
2. poi attende una delle sue **transizioni**;
3. la prima transizione vera porta al passo indicato.

| Campo | Significato | Esempio |
|---|---|---|
| **Nome** | identificativo | `Batch_R1` |
| **Descrizione**, **Area** | testo e area; gli allarmi della sequenza finiscono in questa area | |
| **Unità** | sostituisce `{unit}` in tag, valori e ricette: la stessa sequenza si duplica per unità uguali cambiando solo questo campo | `R1` |
| **Avvio da PLC** | tag booleano: sul fronte di salita la sequenza parte (se non è già in esecuzione) | `R1.Start` |
| **Parametri di avvio** | parametri con valore predefinito, chiesti all'operatore all'avvio e usabili come `{nome}` | `recipe=Standard` |

### Passi e transizioni

Ogni passo ha:

| Campo | Significato |
|---|---|
| **Nome**, **Descrizione** | come appare nel diagramma e nello storico |
| **T. minimo (s)** | le transizioni vengono valutate solo dopo questo tempo |
| **Timeout (s)** | tempo massimo nel passo (vuoto = nessuno) |
| **in timeout →** | passo a cui andare in timeout. *errore + allarme* = la sequenza va in errore e genera l'allarme `SEQ.<nome>` nell'area |

**⚡ Azioni all'ingresso**, eseguite in ordine:

| Azione | Campi | Effetto |
|---|---|---|
| **Scrivi tag** | tag, valore (`true`, `false`, numero, testo, `{parametro}`) | scrive il tag |
| **Carica ricetta** | tipo di ricetta, nome (es. `{recipe}`) | scrive tutti i parametri della ricetta sull'unità della sequenza |
| **Script Python** | funzione | esegue uno [script](designer-script.md) con payload `{sequence, unit, params}` |
| **Messaggio nel log** | testo | scrive nel log eventi |

Se un'azione fallisce (tag non scrivibile, ricetta inesistente, errore nello script), la sequenza va in **errore** con allarme.

**➜ Transizioni**: valutate in ordine; vince la prima vera.

| Campo | Significato |
|---|---|
| **tutte / almeno una** | combinazione delle condizioni |
| **vai a** | passo successivo: *(passo seguente)* in elenco, un passo per nome (anche all'indietro, per i cicli) o **FINE** |
| **etichetta** | descrizione mostrata nel diagramma e nello storico |
| **Condizioni** | tag, operatore (`==`, `!=`, `>`, `>=`, `<`, `<=`) e valore. Il valore può essere una costante oppure `tag:Nome` (es. livello `>=` `tag:{unit}.SP_Level`) |

Pseudo-tag nelle condizioni:

| Pseudo-tag | Valore |
|---|---|
| `@elapsed` | secondi trascorsi nel passo, pause escluse (es. `@elapsed >= tag:{unit}.SP_MixTime`) |
| `@confirm` | vero dopo la **conferma dell'operatore** dal runtime (es. controllo qualità) |

Una transizione senza condizioni scatta subito.

I pulsanti di ogni passo:
- **↑ ↓** spostano il passo;
- **+** inserisce un passo dopo;
- **✕** elimina il passo;
- **▸** espande o chiude il passo.

Rinominando un passo, i riferimenti delle transizioni si aggiornano da soli.

### Azioni di arresto

Eseguite quando l'operatore **arresta** la sequenza: servono a portare l'impianto in stato sicuro (chiudere valvole, fermare motori e riscaldatori).

## Esempio: reattore batch

| # | Passo | Azioni | Transizione |
|---|---|---|---|
| 1 | Carico ricetta | carica ricetta `Batch` `{recipe}` | subito |
| 2 | Riempimento | `{unit}.FillValve` ← vero | `{unit}.Level >= tag:{unit}.SP_Level` (timeout 180 s) |
| 3 | Riscaldamento | valvola chiusa, riscaldatore e agitatore accesi | `{unit}.Temp >= tag:{unit}.SP_Temp` |
| 4 | Mantenimento | — | `@elapsed >= tag:{unit}.SP_MixTime` |
| 5 | Controllo qualità | riscaldatore spento | `@confirm == true` |
| 6 | Scarico | agitatore spento, scarico aperto | `{unit}.Level <= 1` |
| 7 | Fine | scarico chiuso | → FINE |

La demo si ricrea con `python3 tools/build_demo_process.py`: reattori R1 e R2 simulati, ricette *Standard*, *Rapida* e *Intensiva*, sequenze `Batch_R1` e `Batch_R2`.
