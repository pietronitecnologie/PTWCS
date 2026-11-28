# Designer – Processo

Il controllo di processo segue il modello ISA-88 (come SIMATIC BATCH):

| Concetto | Cos'è | Dove si configura |
|---|---|---|
| **Dispositivo** (unità) | una parte d'impianto: reattore, serbatoio, linea. Ha una **classe** | Designer → ⚗ Processo → Dispositivi |
| **Sequenza** (fase) | un SFC con **parametri propri** che scrive e legge i tag del dispositivo. Esempi: *Carico tank*, *Alza temperatura* | Designer → ⚗ Processo → Sequenze |
| **Ricetta** (procedura) | uno SFC che **schedula le sequenze**: le avvia sui dispositivi passando i parametri, anche in parallelo | Runtime → Processo → Ricette |
| **Batch** | una ricetta in esecuzione: **alloca in esclusiva** i suoi dispositivi | Runtime → Processo → Batch |

Dispositivi e sequenze fanno parte del progetto (**Salva progetto**). Le ricette sono dati di produzione: si creano dal runtime e stanno nel database, con versioni.

## Dispositivi

| Colonna | Significato | Esempio |
|---|---|---|
| **Nome** | identificativo; sostituisce `{unit}` nei tag delle sequenze | `R1` |
| **Classe** | tipo di unità: sequenze e ricette chiedono un dispositivo di una classe | `Reattore` |
| **Area** | area di appartenenza: gli allarmi di batch e fasi finiscono qui | `Processo` |
| **Descrizione** | testo libero | |

**Allocazione**: quando parte un batch, ogni dispositivo che usa diventa suo fino alla fine del batch. Nel frattempo nessun altro batch può allocarlo e nessuna fase manuale può partire su di esso.

## Sequenze (fasi)

| Campo | Significato |
|---|---|
| **Nome**, **Descrizione**, **Area** | identificazione |
| **Classe di dispositivo** | la fase lavora su un dispositivo di questa classe; nei tag si scrive `{unit}` (es. `{unit}.FillValve`). Vuoto = fase senza dispositivo |
| **Dispositivo predefinito** | proposto all'avvio manuale e usato dall'avvio da PLC |
| **Avvio da PLC** | tag booleano: sul fronte di salita la fase parte sul dispositivo predefinito, con i parametri di default |

La stessa sequenza può girare contemporaneamente su dispositivi diversi: c'è un'istanza per dispositivo, es. `Carico@R1` e `Carico@R2`.

### Parametri della sequenza

| Colonna | Significato |
|---|---|
| **Parametro** | nome, usato come `{nome}` in tag, valori e condizioni |
| **Unità**, **Descrizione** | per l'operatore e l'editor delle ricette |
| **Min / Max** | limiti verificati all'avvio (dalla ricetta o dall'operatore) |
| **Default** | valore se non viene passato; senza default il parametro è obbligatorio |

Esempio: la sequenza *Riscaldo* ha i parametri `Temperatura` e `Mantenimento`.
- Il primo passo scrive `{unit}.SP_Temp ← {Temperatura}` e accende il riscaldatore.
- La transizione attende `{unit}.Temp >= {Temperatura}`.
- Il passo successivo attende `@elapsed >= {Mantenimento}`.

### Passi, rami paralleli e transizioni

Lo stesso editor serve per sequenze e ricette.

| Elemento | Significato |
|---|---|
| **Passo** | nome, descrizione, tempo minimo, timeout (vuoto = nessuno) e passo in caso di timeout (*errore + allarme* se non indicato) |
| **⚡ Azioni all'ingresso** | *Scrivi tag* (valore, `true`/`false`, `{parametro}`), *Script Python*, *Messaggio nel log*; nelle ricette anche *Avvia fase* |
| **⫴ Rami paralleli** | divergenza simultanea (AND): all'ingresso nel passo partono tutti i rami, ognuno con i suoi passi. Il passo prosegue solo quando **tutti** i rami sono arrivati in fondo (convergenza) |
| **➜ Transizioni** | valutate in ordine, vince la prima vera: più transizioni = **divergenza alternativa**. *vai a* = passo seguente, un passo per nome (anche all'indietro, per i cicli) o FINE |

Una transizione è vera quando:
- le sue **condizioni** sono vere tutte (*tutte*) o almeno una (*almeno una*);
- ogni condizione confronta tag e valore con `==`, `!=`, `>`, `>=`, `<` o `<=`;
- il valore può essere una costante, `{parametro}` oppure `tag:Nome` (il valore di un altro tag).

Pseudo-tag:

| Pseudo-tag | Valore |
|---|---|
| `@elapsed` | secondi nel passo, tempo di hold escluso |
| `@confirm` | conferma dell'operatore (pulsante **✓ Conferma** nel runtime) |
| `@phases_done` | (ricette) le fasi avviate nel passo sono concluse |

Senza transizioni il passo prosegue subito, oppure quando rami e fasi sono conclusi. Nei rami, *FINE* chiude il ramo.

### Editor SFC a flowchart

Fasi e ricette si programmano con lo stesso editor grafico, nello stile del CFC/SFC di PCS7: l'SFC si legge dall'alto in basso.

| Simbolo | Elemento |
|---|---|
| riquadro `S1`, `S2`… (il primo a doppio bordo = passo iniziale) | **passo**: nome, azioni all'ingresso (⚡ numero), rami (⫴), tempo minimo (⧗), timeout (⏱). Nei rami la numerazione segue il percorso: `S2.1.1` = passo 1 del ramo 1 del passo 2 |
| barra orizzontale `T…` con la condizione a fianco | **transizione** verso il passo seguente. Grigia in corsivo = implicita (*subito*, *fasi concluse*, *rami conclusi*) |
| diramazione viola con `↷ Passo` | **transizione alternativa**: salto a un altro passo (anche all'indietro, per i cicli) o a FINE. Le transizioni di un passo si valutano in ordine e vince la prima vera |
| doppia linea con le etichette `ramo 1`, `ramo 2`… | **rami paralleli**: partono insieme; il passo prosegue quando tutti sono arrivati in fondo |
| `◀ da …` accanto a un passo | il passo è destinazione di un salto |
| `⏱ 600s → errore` a sinistra | timeout del passo e destinazione |

Come si programma:
- **clic** su un passo, una transizione o l'etichetta di un ramo: lo seleziona (bordo arancione) e apre le sue proprietà nel pannello a destra;
- **+** azzurro sui collegamenti (visibile al passaggio del mouse): inserisce un passo in quel punto;
- **barra strumenti**:
  - **＋ Passo** inserisce dopo la selezione (o in fondo);
  - **⫴ Ramo parallelo** aggiunge un ramo al passo selezionato;
  - **↷ Alternativa** aggiunge una transizione con salto;
  - **⏶ ⏷** spostano passo o transizione;
  - **✕** elimina (anche il tasto Canc);
  - **− / +** cambiano lo zoom;
- **tasto destro**: menu con le stesse operazioni, più *Inserisci passo prima* e *Aggiungi condizione di transizione*.

Nel pannello proprietà:

| Selezione | Campi |
|---|---|
| **Passo** | nome (rinominando, i salti che lo citano si aggiornano), descrizione, tempo minimo, timeout e destinazione; azioni all'ingresso (scrittura tag, script, log; nelle ricette **Avvia fase** con dispositivo e valori dei parametri della fase) |
| **Transizione** | etichetta, logica delle condizioni (*tutte* = E, *almeno una* = O), destinazione (*passo seguente*, un passo per nome o FINE), elenco delle condizioni |
| **Ramo** | aggiungi un passo in fondo al ramo, nuovo ramo, elimina ramo |

Nel runtime lo stesso disegno mostra l'esecuzione:
- **passi attivi in verde** con il tempo trascorso;
- passi già eseguiti con il bordo blu e la loro durata.

Lo si vede nel dettaglio di batch e fasi e nell'anteprima delle ricette.

### Stati e azioni di stato

Fasi e batch seguono il modello di stato ISA-88:

| Comando | Da → a | Azioni eseguite |
|---|---|---|
| **Hold** | in esecuzione → in hold (le transizioni si fermano, i tempi si congelano) | *Hold*: es. chiudere le valvole |
| **Restart** | in hold o in errore → in esecuzione (dopo un errore riprova il passo fallito) | *Restart*: es. riaprire la valvola |
| **Stop** | → arrestata | *Stop*: arresto controllato |
| **Abort** | → abortita | *Abort* (vuoto = quelle di stop): arresto immediato |
| **Reset** | da completata, arrestata, abortita o in errore → inattiva | — |

Timeout o azioni fallite portano in **errore**, con l'allarme `SEQ.<istanza>` nell'area.

Comandi, errori e avvii si propagano tra batch e fasi:
- hold, restart, stop e abort di un batch valgono per tutte le sue fasi;
- l'errore di una fase manda il batch in errore e mette in hold le altre fasi.

## Ricette

Le ricette si creano nel runtime (**Processo → 🧪 Ricette → + Nuova**, permesso `recipe.edit`):

| Sezione | Contenuto |
|---|---|
| **Dispositivi richiesti** | alias (es. `reattore`), classe (`Reattore`), dispositivo predefinito. Nelle fasi si scrive `{reattore}` |
| **Parametri della ricetta** | valori di formula con limiti, modificabili all'avvio del batch; passati alle fasi come `{nome}` |
| **Procedura** | SFC i cui passi avviano le fasi: *Avvia fase* → sequenza, dispositivo `{alias}`, valori dei parametri della sequenza (costanti o `{parametro_ricetta}`). Un passo con fasi attende che siano concluse; i rami paralleli fanno lavorare più fasi insieme |
| **Azioni di stato** | come per le sequenze |

Ogni salvataggio con modifiche crea una nuova **versione**. Un batch in corso continua a usare la versione con cui è partito.

### Esempio: ricetta *Crema*

| # | Passo | Cosa fa |
|---|---|---|
| 1 | Carico | fase `Carico` su `{reattore}` con `Livello = {Livello}` |
| 2 | Trattamento | **rami paralleli**: fase `Riscaldo` (`Temperatura`, `Mantenimento`) ‖ fase `Agitazione` (`Velocita`, `Tempo = 60`) |
| 3 | Controllo qualità | attende la conferma dell'operatore (`@confirm`) |
| 4 | Scarico | fase `Scarico` |

La demo si ricrea con `python3 tools/build_demo_process.py`:
- reattori R1 e R2 simulati;
- sequenze `Carico`, `Riscaldo`, `Agitazione` e `Scarico`;
- ricette *Crema* e *Base*.
