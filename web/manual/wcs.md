# WCS – Magazzini automatici a trasloelevatori

Runtime → **WCS** (oppure tasto destro → *Pagine di sistema* → *WCS magazzini*).

In alto ci sono la scelta del magazzino e i dati di sintesi:
- saturazione e celle libere;
- missioni in coda, in corso e sospese;
- ordini aperti (e quanti in attesa di giacenza);
- missioni dell'ultima ora (⬇ depositi, ⬆ prelievi, ⇄ movimentazioni);
- stato della riorganizzazione automatica.

## Missioni

Elenco delle missioni aperte (in corso prima, poi in coda per priorità) e delle ultime eseguite.

| Colonna | Significato |
|---|---|
| **Tipo** | ⬇ deposito (P&D → cella), ⬆ prelievo (cella → baia d'uscita), ⇄ movimentazione (cella → cella, stessa corsia) |
| **Prio** | priorità da 1 (bassa) a 9 (urgente). Il dispatcher aggiunge 1 punto ogni 2 minuti di attesa, così nessuna missione resta ferma |
| **UdC / Articolo** | unità di carico e articolo; click sull'UdC apre l'anagrafica |
| **Da / A** | celle in formato `Ccc-Ll-col-liv` (corsia, lato, colonna, livello); `P&D` = testa corsia |
| **Baia / motivo** | baia d'uscita (prelievi), ingresso (depositi) o motivo della movimentazione |
| **Ordine** | ordine di prelievo a cui appartiene la missione |
| **Stato** | in coda, inviata, in corso (con il posto UdC del traslo), sospesa |

**Azioni** sulle missioni in coda:
- **▲ / ▼**: alza o abbassa la priorità.
- **❚❚ / ▶**: sospende o rilascia. Una missione sospesa resta in coda ma non viene inviata.
- **✕**: annulla. Le celle tornano libere (deposito) oppure occupate (prelievo).

Sulle missioni **inviate / in corso**:
- **↻ Rimetti in coda**: da usare solo se il traslo ha perso la missione, ad esempio dopo un reset del PLC.
- **✓ Conferma manuale**: l'operatore conferma che la missione è stata eseguita a mano; le celle vengono aggiornate di conseguenza.

Pulsanti in alto:
- **Deposito manuale**: UdC, articolo, quantità, lotto, altezza; la cella è scelta dalla strategia.
- **Prelievo UdC**: per codice, oppure casuale se il campo è vuoto.
- **Movimentazione**: sposta un'UdC nella cella indicata, oppure nella migliore secondo la strategia.

Filtri: tipo, corsia, testo (UdC, articolo, cella).

## Ordini

Un ordine di prelievo ha righe *articolo + quantità* ed è coperto con **UdC intere** in ordine **FIFO** (data di ingresso), a parità per vicinanza al P&D.

| Stato | Significato |
|---|---|
| in attesa giacenza | manca merce: l'ordine viene ricoperto da solo all'arrivo di nuove UdC |
| in prelievo | missioni in corso |
| evaso | tutte le righe prelevate |
| chiuso con mancanze | chiuso dall'operatore con **Chiudi** |
| annullato | **Annulla** cancella le missioni ancora in coda |

**+ Nuovo ordine**:
- **riferimento** (facoltativo): codice WMS/ERP;
- **baia d'uscita**: vuoto = bilanciata;
- **priorità**;
- **righe**: suggerimento degli articoli con la disponibilità.

## Giacenze

- A sinistra, gli **articoli**: classe ABC, prelievi nella finestra, UdC, quantità, distribuzione per corsia, distribuzione nelle zone A/B/C, UdC più vecchia. Click su un articolo per vederne le UdC.
- A destra, le **UdC** in ordine FIFO, con cella, zona, stato e data di ingresso. ⚠ = UdC fuori dalla zona della propria classe. Click sull'UdC apre l'[anagrafica](#anagrafica-udc); click sulla cella apre il dettaglio cella (preleva, ottimizza posizione, blocca).

### Anagrafica UdC

Articolo, quantità, lotto, altezza, peso, data di ingresso.
- Al deposito da protocollo sono letti dal telegramma di ingresso: campi `sku`/`articolo`, `qty`/`qta`, `lot`/`lotto`, `height`/`altezza`, `weight`/`peso`.
- Dal dialogo UdC si fa la **rettifica inventariale** (**Salva rettifica**), oppure si prepara una movimentazione o un prelievo.

## Riorganizzazione

La riorganizzazione (defrag) riporta il magazzino in ordine con **missioni di movimentazione** (tipo 3) eseguite nei tempi morti dei trasli. Le mosse seguono quest'ordine:
1. **Promozione**: le UdC in una zona peggiore della propria classe vanno nella zona giusta (prima classe A, prima le più lontane).
2. **Liberazione**: se la zona giusta è piena, si sposta altrove un'UdC di classe inferiore che la occupa.
3. **Compattazione**: le UdC più lontane vanno nella cella libera più vicina della stessa zona, se il guadagno supera la soglia.

Comandi:
- **Automatica nei tempi morti**: una mossa per corsia dopo N secondi senza missioni, con un massimo all'ora (parametri nel Designer → [Magazzini](designer-progetto.md#magazzini-automatici)).
- **Simula piano**: calcola le mosse senza eseguirle (per corsia o per tutto il magazzino, fino a *Mosse max*).
- **Crea movimentazioni**: crea subito le prime N mosse del piano, a priorità 1.

Indicatori (attuale → dopo il piano):

| Indicatore | Significato |
|---|---|
| **Indice di frammentazione** | celle libere dentro la regione compatta ideale / celle occupate (0% = compatto) |
| **UdC fuori zona ABC** | UdC la cui zona non corrisponde alla classe dell'articolo |
| **Costo di accesso pesato** | Σ peso della classe (A 3, B 2, C 1) × tempo di traslazione dal P&D |
| **Efficienza layout** | costo ideale / costo attuale (100% = ottimo) |

La **Mappa ABC** mostra un lato di corsia:
- sfondo = zona della cella (verde A, blu B, viola C), P&D a sinistra in basso;
- pallino = classe dell'UdC;
- bordo giallo = UdC fuori zona.

Click su una cella apre il dettaglio.

## Trasli / KPI

Per ogni traslo, sull'ultima ora:

| KPI | Significato |
|---|---|
| **Utilizzo** | quota di tempo in ciclo |
| **Disponibilità** | quota di tempo senza guasto |
| **Missioni/h** | missioni eseguite all'ora |
| **Ciclo medio** | durata media di un ciclo (da Start a completamento) |
| **⬇ / ⬆ / ⇄** | missioni per tipo |
| **Cicli (doppi)** | cicli eseguiti; per i trasli a doppia UdC, quanti con due missioni |
| **In coda** | missioni in attesa sulla corsia |

Il grafico mostra le missioni eseguite a intervalli di 5 minuti.

## Strategia

Riepilogo della strategia di stoccaggio configurata e delle regole di assegnazione:
- **Corsia**: la meno carica, distribuendo lo stesso articolo su più corsie; sono escluse le corsie con traslo in guasto o non in automatico.
- **Cella**:
  - classe A → zona A, poi B, poi C;
  - classe B → B, C, A;
  - classe C → C, B, A;
  - dentro la zona, la cella libera più vicina al P&D.
- **Sagoma**: rispetto dell'altezza massima di ogni livello.
- **Dispatcher**: priorità con invecchiamento, missioni sospese escluse, accoppiamento dei cicli doppi.
