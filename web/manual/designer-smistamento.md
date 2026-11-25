# Designer – Smistamento

Il workspace **🔀 Smistamento** configura come il WCS risponde alle richieste del PLC: quale uscita deve prendere un collo, in quale corsia va un pallet, se un'UdC va scartata.

È diviso in quattro sezioni (menu a sinistra):

| Sezione | Cosa configura |
|---|---|
| [📡 Protocollo request/reply](#protocollo-requestreply) | i **punti di decisione**: quali dati manda il PLC, quali risposte scrive il WCS e come si sincronizzano |
| [🔀 Regole di smistamento](#regole-di-smistamento) | le **regole grafiche** che calcolano le risposte |
| [🏷 Classi di smistamento](#classi-di-smistamento) | gruppi di barcode con una destinazione, definiti nel WCS o forniti dal WMS |
| [🔌 Connessione WMS](#connessione-wms) | la connessione REST al WMS per chiedere la destinazione di un collo |

Le modifiche di Protocollo, Classi e WMS fanno parte del **progetto**: diventano attive con **Salva progetto**, in alto a destra, e vengono applicate a caldo. Le regole hanno un pulsante **Salva regole** dedicato.

## Catena di decisione

Ogni volta che il PLC invia una richiesta, il WCS valuta in quest'ordine:

1. **Regole grafiche** del punto, dall'alto verso il basso. Vince la prima regola le cui condizioni sono vere e le cui uscite sono tutte risolvibili.
2. **Default** delle regole, se attivo.
3. **Funzione Python** del punto (campo *Funzione Python*), se configurata.
4. **Fallback** del punto.

Se nessun passo dà una risposta valida e il fallback è vuoto, il WCS **non risponde**: al timeout il PLC applica il proprio default.

> **Esempio** – scanner SC1: la regola *NOREAD* manda allo scarto i colli senza barcode, la regola *Classe Milano* li manda all'uscita 1, il default chiede al WMS (`wms:barcode`); se il WMS non conosce il collo, decide la funzione Python `decide_sc1`.

## Protocollo request/reply

La barra in alto elenca i punti di decisione: click per selezionarne uno.
- **+ Nuovo punto di decisione** crea un punto vuoto.
- **+ Modello telegramma** crea un tipo UDT telegramma con i ruoli già impostati, da usare poi per le istanze (vedi [Tag e UDT](designer-tag.md#tipi-udt)).

### Campi del punto di decisione

| Campo | Significato | Come compilarlo |
|---|---|---|
| **Nome** | Identificativo del punto, usato nelle regole, nei log e nelle statistiche | Breve e univoco, es. `SC1`, `IN1` |
| **Descrizione** | Testo libero | Es. "Scanner ingresso linea" |
| **Area** | Area di appartenenza: errori e timeout del punto finiscono negli allarmi di quest'area | Scegli dall'elenco ([Aree](designer-progetto.md#aree)) |
| **Definizione telegramma** | *da istanza UDT* (consigliato) oppure *tag manuali* | Con l'istanza UDT i dati vengono presi dai **ruoli** dei membri |
| **Istanza telegramma** | Istanza UDT che contiene il telegramma | Compaiono solo le istanze il cui tipo ha almeno un membro `request` e uno `response` |
| **Tag richiesta (PLC)** | Solo con *tag manuali*: tag che il PLC incrementa o alza per chiedere una decisione | Es. `SC1.ReqID` |
| **Tag risposta (WCS)** | Solo con *tag manuali*: tag che il WCS scrive per confermare la risposta | Es. `SC1.RespID` |
| **Handshake** | Sincronizzazione tra PLC e WCS: *Contatore* o *Flag* | Vedi [Handshake](#handshake) |
| **Funzione Python** | Script chiamato se nessuna regola decide | Una funzione del workspace [Script](designer-script.md); *(nessuna)* = solo regole |
| **Timeout script (ms)** | Tempo massimo per la funzione Python; oltre, si passa al fallback | Tipico 300 ms. Deve stare sotto il timeout del PLC |

### Ruoli dei membri del telegramma

Nel tipo UDT ogni membro ha un **Ruolo protocollo**:

| Ruolo | Scritto da | Uso |
|---|---|---|
| `request` | PLC | contatore (`ReqID`) o flag (`Req`) della richiesta; uno solo |
| `response` | WCS | eco del contatore (`RespID`) o flag di conferma (`Ack`); uno solo |
| `input` | PLC | dati della richiesta (barcode, peso, altezza…): nelle regole diventano i **campi** (nome del membro in minuscolo) |
| `output` | WCS | risposte (destinazione, esito…): nelle regole diventano le **uscite** |

I membri `output` e `response` devono essere **scrivibili** (colonna R/W).

### Input e Output (tag manuali)

Con *tag manuali* si compilano due tabelle:
- **Alias (campo regole)**: il nome usato nelle regole e negli script, es. `barcode`.
- **Tag**: il tag da cui leggere (input) o su cui scrivere (output).

### Fallback

Coppie *uscita → valore* scritte quando nessun passo della catena risponde, es. `destination = 99`, `result = 4`.

Se lasci vuoto il fallback, il WCS non risponde e il PLC va in timeout. Usa questa scelta solo se il PLC gestisce il timeout in sicurezza.

### Protocollo risolto

Mostra i tag effettivi del punto (richiesta, risposta, input, output) ricavati dall'istanza UDT. Gli errori (membri mancanti, tag inesistenti, ruoli doppi) compaiono qui in rosso e impediscono il salvataggio del progetto.

### Handshake

**Contatore** (consigliato, robusto ai riavvii):
1. il PLC scrive i dati, poi `ReqID = ReqID + 1` (mai 0);
2. il WCS vede `ReqID ≠ RespID` e calcola la decisione;
3. il WCS scrive le uscite e poi `RespID = ReqID`;
4. per il PLC le uscite sono valide quando `RespID = ReqID`; se scade il timeout, usa il proprio default.

**Flag**:
1. il PLC scrive i dati e alza `Req`;
2. il WCS decide, scrive le uscite e alza `Ack`;
3. il PLC legge e abbassa `Req`; il WCS abbassa `Ack`.

Il diagramma *Sequenza dell'handshake* in fondo alla pagina mostra la sequenza del metodo scelto.

## Regole di smistamento

In alto:
- **Punto di decisione**: le regole sono per punto.
- **Regole attive**: se disattivato, le regole vengono ignorate e si passa a script e fallback.
- **Azzera contatori**: azzera i contatori ▶ delle regole.
- **Annulla modifiche**: ricarica le regole salvate.
- **Salva regole** (● = modifiche non salvate): convalida e attiva subito, senza riavvii.

### Struttura di una regola

Ogni regola è una scheda con:

| Elemento | Significato |
|---|---|
| ☑ (casella) | regola abilitata; se disabilitata viene saltata |
| **Nome** | descrizione leggibile, compare nel log delle decisioni |
| **tutte / almeno una** | combinazione delle condizioni: tutte vere (AND) oppure almeno una vera (OR) |
| ▶ *n* | colli decisi da questa regola dall'avvio |
| ↑ ↓ ⧉ ✕ | sposta, duplica, elimina. **L'ordine conta**: vince la prima regola vera |
| **Se** | elenco delle condizioni |
| **Allora** | uscite, script e messaggio di log |

### Condizioni

Ogni condizione ha **campo**, **operatore** e **valore**.

- **Campo**: un input del telegramma (es. `barcode`, `weight`) oppure `tag:Nome` per confrontare il valore di un tag qualsiasi.
- **Valore**: costante oppure `tag:Nome` (es. peso maggiore di `tag:MP.MaxWeight`).

| Operatore | Vero quando | Esempio di valore |
|---|---|---|
| `=` / `≠` | uguale / diverso (numeri confrontati come numeri) | `0` |
| `>` `≥` `<` `≤` | confronto numerico | `tag:WCS.MaxWeight` |
| compreso tra | `min..max`, estremi inclusi | `100..500` |
| inizia con / finisce con / contiene | confronto di testo | `IT-MI` |
| è uno di / non è uno di | lista separata da virgole | `NOREAD, ,ERR` |
| regex | espressione regolare | `^IT-(MI\|TO)-\d+$` |
| è vuoto / non è vuoto | campo assente o stringa vuota | — |
| appartiene alla classe / non appartiene | il valore appartiene a una delle [classi](#classi-di-smistamento) indicate | `MILANO, TORINO` |
| ha una classe | il valore appartiene ad almeno una classe | — |
| il WMS conosce la destinazione | il [WMS](#connessione-wms) ha una destinazione per il valore | — |

Il pallino accanto a ogni condizione mostra l'esito nell'ultima **prova** (verde vero, rosso falso).

### Uscite ("Allora")

Per ogni uscita del punto (es. `destination`, `result`) puoi scrivere:

| Valore | Significato |
|---|---|
| *(vuoto)* | l'uscita non viene scritta da questa regola |
| `3` | valore fisso |
| `1\|2` | **rotazione**: a ogni collo il valore successivo (bilanciamento tra uscite) |
| `tag:Nome` | valore corrente di un tag |
| `class:barcode` | destinazione della [classe](#classi-di-smistamento) a cui appartiene il campo `barcode` |
| `wms:barcode` | destinazione restituita dal [WMS](#connessione-wms) per il campo `barcode` |
| `store:MP1` | **deposito in magazzino automatico**: assegna cella e missione nel magazzino `MP1` e risponde con la corsia (vedi [WCS](wcs.md)) |
| `store:MP1:lu` | come sopra, usando il campo `lu` come identificativo UdC (default `barcode`) |

Se un'uscita `class:`, `wms:` o `store:` **non è risolvibile** (classe sconosciuta, WMS che non conosce il collo, magazzino pieno), **la regola viene saltata** e la valutazione passa alla regola successiva.

- **script**: funzione Python a cui delegare la decisione quando la regola scatta. Il risultato dello script sostituisce le uscite.
- **log**: messaggio nel log eventi. `{campo}` viene sostituito dal valore, es. `{barcode} → uscita 2`.

### Default

È la regola senza condizioni, valutata se nessun'altra scatta. Se non è attivo, decidono la funzione Python e poi il fallback.

### Prova regole

Il riquadro a destra valuta la **bozza corrente**, anche non salvata:
1. inserisci i valori di esempio degli input (barcode, peso…);
2. premi **Prova**: le condizioni sono valutate con i tag live e la regola vincente viene evidenziata in verde.

Nessuna scrittura viene inviata al PLC e nessun deposito viene allocato (`store:` lavora in simulazione).

### Ultime decisioni

Elenco delle decisioni reali del punto: input, uscite, regola o script che ha deciso, durata.

## Classi di smistamento

Una classe raggruppa dei barcode e ha una destinazione.

| Colonna | Significato | Esempio |
|---|---|---|
| **Nome** | identificativo, usato nelle regole | `MILANO` |
| **Descrizione** | testo libero | Filiale Milano |
| **Destinazione** | uscita della classe; `1\|2` = a rotazione | `1` |
| **Origine** | *WCS (locale)*: barcode e pattern definiti qui. *WMS*: appartenenza fornita dal WMS e sincronizzata periodicamente | WCS |
| **Pattern** | modelli con `*` (qualsiasi sequenza) e `?` (un carattere), separati da virgola | `IT-MI*, ??-MI-*` |
| **Barcode espliciti** | elenco esatto, separato da virgola | `ABC123, ABC124` |

Nelle regole:
- **condizione**: *appartiene alla classe* `MILANO`;
- **uscita**: `class:barcode` assegna la destinazione della classe del collo.

Se un barcode appartiene a più classi, vale la prima nell'ordine della tabella.

**Prova un barcode** mostra le classi del barcode e, se spunti *chiedi anche al WMS*, la risposta del WMS. La prova usa il progetto **salvato**.

## Connessione WMS

| Campo | Significato | Esempio |
|---|---|---|
| **abilitata** | attiva la connessione | ☑ |
| **URL base** | indirizzo del servizio REST del WMS | `http://wms.local/api` |
| **Destinazione collo (GET)** | percorso per chiedere la destinazione; `{barcode}` è sostituito | `/parcels/{barcode}/destination` |
| **Campo destinazione** | campo JSON della risposta con la destinazione | `destination` |
| **Campo classe** | campo JSON con la classe (facoltativo) | `class` |
| **Elenco classi (GET)** | percorso che restituisce le classi `[{name, destination, barcodes, patterns}]` per le classi con origine WMS; vuoto = nessuna sincronizzazione | `/sort-classes` |
| **Sincronizzazione classi (s)** | intervallo di aggiornamento delle classi | `60` |
| **Timeout richiesta (ms)** | tempo massimo di una richiesta | `800` |
| **Cache risposte (s)** | per quanto tempo una risposta viene riusata senza richiamare il WMS | `300` |
| **Header HTTP** | header aggiunti alle richieste | `Authorization` = `Bearer ${WMS_TOKEN}` |

> **Sicurezza** – non scrivere token nel progetto: usa `${NOME_VARIABILE}`. Il valore viene letto dalle variabili d'ambiente del server.

Risposte del WMS:
- **200** con JSON: destinazione (ed eventuale classe);
- **404**: collo sconosciuto, quindi la regola che usa `wms:barcode` viene saltata;
- errori e timeout sono contati in **Stato → Richieste / errori**, e la regola viene saltata.

Il riquadro **Stato** mostra connessione, numero di richieste ed errori, dimensione della cache e ultima sincronizzazione delle classi.

## Esempi completi

**Scarto dei NOREAD e bilanciamento**

| Regola | Se | Allora |
|---|---|---|
| NOREAD | `barcode` è uno di `NOREAD,` | `destination = 99`, `result = 1` |
| Sovrappeso | `weight` > `tag:WCS.MaxWeight` | `destination = 99`, `result = 2` |
| Esteri | `barcode` inizia con `DE` *oppure* inizia con `FR` | `destination = 3` |
| Nazionali | `barcode` appartiene alla classe `ITALIA` | `destination = 1\|2` |
| Default | — | `destination = wms:barcode` |

**Ingresso magazzino pallet**

| Regola | Se | Allora |
|---|---|---|
| Fuori sagoma | `height` > `tag:MP.MaxHeight` | `destination = 0`, `result = 3` |
| Stoccaggio | `barcode` non è vuoto | `destination = store:MP1`, `result = 0` |

Il deposito legge dal telegramma anche `sku`, `qty`, `lot`, `height` e `weight` per l'anagrafica dell'UdC (vedi [WCS](wcs.md#anagrafica-udc)).
