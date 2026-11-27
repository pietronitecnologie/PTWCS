# Designer – Fisica

Il **layer fisico** serve al **collaudo virtuale**. Il layout disegnato per lo SCADA diventa il modello dell'impianto: ai componenti delle viste si aggiunge un po' di fisica (lunghezze, velocità, accelerazioni, sensori, materiali).

```
PLC (PLCSIM Advanced o reale) ──attuatori──▶ tag ──▶ LAYER FISICO ──sensori──▶ tag ──▶ ingressi simulati del PLC
                │                                  unità che si muovono, si accumulano,
                └──▶ tag ◀── SCADA / WCS (invariati)   vengono lette, deviate, escono
```

Ognuno fa la sua parte:
- **Il PLC esegue la sua logica vera.** Avvia i nastri, posiziona i deviatori, comanda i fermi.
- **Il layer fisico crea i movimenti.** Le unità di carico avanzano con la velocità e l'accelerazione dei nastri e si accumulano senza sovrapporsi. Passano da un nastro all'altro e attraversano fotocellule e scanner.
- **SCADA e WCS non cambiano.** Leggono e scrivono gli stessi tag, come con l'impianto reale.

La simulazione riguarda **componenti e materiali**, non le logiche: quelle restano nel PLC e nel WCS, che così vengono collaudati.

## Attuatori e sensori

| Ruolo | Chi scrive il tag | Esempi |
|---|---|---|
| **▶ Attuatore** | il PLC; la simulazione lo legge | marcia nastro, setpoint di velocità, posizione del deviatore, comando di un fermo |
| **◀ Sensore** | la simulazione, negli **ingressi simulati** del PLC | fotocellule, barcode letto e contatore letture, peso, velocità reale, finecorsa, contatori |

I tag sensore devono essere **scrivibili** e su un driver **abilitato alla simulazione**; i driver interni `sim` sono sempre ammessi. È una protezione: la simulazione non scrive mai su un PLC che non è stato dichiarato come virtuale o in collaudo.

## Componenti fisici

Nel workspace **🖼 Viste**:
1. spunta **⚛ Fisica** nella barra in alto;
2. seleziona un componente;
3. nella sezione **⚛ Fisica** delle proprietà scegli il **Tipo**.

Lo stesso simbolo dello SCADA (nastro, rulliera, fotocellula, scanner, deviatore) acquista così un comportamento fisico.

Con il layer attivo, sul disegno compaiono:
- l'**asse** di ogni nastro (freccia azzurra), con lunghezza, velocità e tag di marcia;
- i **collegamenti** tra componenti (tratteggio viola, con il valore di posizione sulle uscite dei deviatori);
- la posizione di **fotocellule, scanner e fermi** sul nastro, in metri.

Tasto destro sul foglio → **Collega automaticamente i nastri**: collega ogni nastro senza successivo al nastro, deviatore o pozzo che inizia vicino alla sua fine.

### Nastro / rulliera

| Campo | Significato |
|---|---|
| **Marcia** ▶ | tag bool comandato dal PLC |
| **Successivo** | nastro, deviatore o pozzo in cui passano le unità alla fine del nastro. Per un componente di un'altra vista: **↗ in un'altra vista…** e `vista/id` (es. `hub_nord/SN_IN`) |
| **Uscita dall'impianto** | le unità escono alla fine del nastro (senza contatore) |
| **Lunghezza** | in metri; vuoto = larghezza del disegno × scala della vista |
| **Velocità max**, **Accelerazione**, **Decelerazione** | m/s e m/s²: le rampe valgono all'avvio e all'arresto |
| **Distanza in accumulo** | spazio minimo tra due unità ferme in coda (m) |
| **Setpoint velocità** ▶, **Valore setpoint** | per gli inverter: velocità = setpoint / valore × velocità max |
| **Velocità reale** ◀, **Scala** | velocità scritta nel PLC, moltiplicata per la scala (es. 600 = m/min × 10) |

Il verso di marcia segue l'asse del componente: da sinistra a destra, oppure al contrario con **Direzione = left**. La rotazione del componente conta.

Come si muovono le unità:
- un'unità avanza con il nastro su cui poggia per la maggior parte;
- se il nastro successivo è fermo, viene spinta finché è a metà, poi si ferma **a cavallo**: la fotocellula in fondo al primo nastro resta occupata.

### Fotocellula

Si accende quando un'unità interrompe il raggio, anche un'unità a cavallo tra due nastri.

| Campo | Significato |
|---|---|
| **Segnale** ◀ | tag bool |
| **Contatto NC** | il segnale è vero a fotocellula libera |
| **Montata su**, **Posizione** | vuoti = il nastro più vicino nel disegno e il punto in cui è disegnata |

### Scanner barcode

Quando il **fronte** di un'unità supera la posizione dello scanner, la simulazione scrive in quest'ordine:
1. il **barcode**;
2. il **peso**;
3. il **contatore letture**, per ultimo.

Il PLC rileva la nuova lettura dal cambio del contatore, come con uno scanner reale su bus di campo. Con **Mancate letture %** una parte delle letture restituisce il testo di mancata lettura (default `NOREAD`).

### Deviatore

È un nodo di trasferimento: va indicato come *Successivo* del nastro di ingresso.

| Campo | Significato |
|---|---|
| **Posizione** ▶ | tag del PLC: il suo valore sceglie l'uscita |
| **Uscite** | coppie *valore → nastro o pozzo* (es. `1 → OUT1`, `99 → REJ`) |
| **Tempo di commutazione** | mentre il deviatore si sposta, e con un valore che non corrisponde a nessuna uscita, le unità aspettano in fondo al nastro di ingresso |
| **Posizione raggiunta** ◀ | il valore della posizione, scritto a fine commutazione |

### Sorgente

Genera unità all'inizio di un nastro, con un intervallo casuale tra *min* e *max*, solo se c'è spazio.

| Campo | Significato |
|---|---|
| **Materiali** | i materiali generati (vuoto = tutti), scelti con il peso nel mix |
| **Barcode** | un elenco separato da virgole, scelto a caso, oppure un modello con contatore: `PKG{n:6}` → `PKG000001`, `PKG000002`… |
| **Abilitazione** | tag opzionale, per esempio un consenso del PLC |
| **Numero massimo** | dopo questo numero di unità la sorgente si ferma |

### Pozzo

Le unità che arrivano qui lasciano l'impianto. Un **contatore** ◀ opzionale viene incrementato a ogni unità.

### Fermo / attuatore

È un attuatore lineare: un fermo, un cilindro, una barriera.

| Campo | Significato |
|---|---|
| **Comando** ▶ | vero = fuori |
| **Tempo di corsa** | tempo per passare da dentro a fuori e viceversa |
| **Finecorsa fuori / dentro** ◀ | sensori di posizione |

Quando è fuori **blocca le unità** in quella posizione del nastro: serve per zone di accumulo, varchi, stazioni.

### Impianti su più viste

Un impianto grande si divide in pagine per area o linea, e i nastri proseguono da una vista all'altra:
- nel campo *Successivo* (o nelle uscite di un deviatore) si indica `vista/id` del componente nell'altra vista;
- nel disegno il collegamento compare come **↗ vista/id**;
- nel runtime ogni unità è disegnata sulla vista del nastro su cui si trova.

## Fluidi

Per gli impianti di processo il layer fisico simula anche **serbatoi, tubazioni, valvole, pompe, riscaldatori e agitatori**. Il modello è a parametri concentrati, calcolato a ogni passo.

| Tipo | Cos'è | Campi principali |
|---|---|---|
| **Serbatoio** | nodo con volume: livello dal bilancio di massa, temperatura dal bilancio di energia | volume, altezza utile, quota del fondo, sensori di livello, temperatura e pressione sul fondo, stato iniziale, dispersione termica |
| **Alimentazione** | sorgente a pressione costante (rete acqua, vapore, collettore) | pressione (bar), temperatura |
| **Scarico** | uscita a pressione atmosferica | — |
| **Giunzione / collettore** | nodo senza volume dove si incontrano più tubazioni | — |
| **Valvola** | in serie su una tubazione, on/off o modulante | comando (o apertura %), Kv, tempo di corsa, finecorsa, posizione |
| **Pompa** | in serie su una tubazione, centrifuga | marcia, setpoint di velocità, prevalenza a portata nulla, portata massima, retroazione di marcia |
| **Tubazione** | ramo tra due nodi, con valvole e al più una pompa | da, a, elementi in serie, Kv del tubo, sensore di portata |
| **Riscaldatore** | potenza ceduta al contenuto di un serbatoio | serbatoio, comando, potenza (kW), setpoint e isteresi del termostato locale |
| **Motore / agitatore** | motore con rampa | marcia, setpoint, velocità massima, rampa, velocità reale |

### Come si calcola

- **Pressioni**:
  - serbatoio: ρ·g·(quota + livello) sul fondo;
  - alimentazione: la sua pressione;
  - scarico: 0 bar;
  - giunzione: la pressione per cui la somma delle portate è zero.
- **Portata di una tubazione**: Q = Kv·√(ΔP / densità relativa), in m³/h e bar.
  - Il Kv equivalente somma in serie il tubo e le valvole: 1/Kv² = 1/Kv_tubo² + Σ 1/(Kv·apertura)².
  - Una valvola chiusa blocca il ramo.
- **Pompa**: aggiunge la prevalenza H(Q) = H0·s²·(1 − (Q / (Qmax·s))²), dove s è la velocità.
  - Ferma, blocca il ramo; non fa passare flusso all'indietro.
  - Se aspira da un serbatoio vuoto la pompa risulta **a secco**.
- **Serbatoi**:
  - il livello segue ingressi, uscite e perdite; oltre il volume il liquido trabocca (troppo pieno);
  - la temperatura segue la miscelazione degli ingressi, i riscaldatori (sopra il 2 % di livello) e la dispersione verso l'ambiente;
  - i travasi per gravità si fermano a livelli uguali, come nei vasi comunicanti.
- **Termostato**: con il setpoint, il riscaldatore resta acceso finché la temperatura è sotto il setpoint meno metà isteresi e si spegne sopra il setpoint più metà isteresi.

Per disegnare i fluidi:
- le **tubazioni** si disegnano con componenti *Linea*;
- nel runtime un tubo con portata si **anima** in azzurro, insieme ai suoi tratti di disegno chiamati `id_seg1`, `id_seg2`…;
- serbatoi e livelli si mostrano con il componente *Serbatoio* legato al tag di livello.

Densità e calore specifico del fluido e temperatura ambiente si impostano nel workspace **⚛ Fisica** (default: acqua a 20 °C).

## Scala della vista

Le proprietà della vista, con il layer ⚛ Fisica attivo, contengono la **Scala mm/px**. Con 10 mm/px un nastro largo 300 px è lungo 3 m. Senza un valore vale la scala predefinita del progetto.

Un nastro con una **Lunghezza** esplicita usa quella: il disegno viene "stirato" sulla lunghezza reale.

## Impostazioni

Sono nel workspace **⚛ Fisica** e si applicano con **Salva progetto**.

| Campo | Significato |
|---|---|
| **Passo di simulazione** | ms tra due cicli (default 20). Internamente il passo fisico non supera 20 ms simulati, anche ad alto fattore tempo |
| **Scala predefinita** | mm per pixel delle viste senza una scala propria |
| **Avvio automatico** | la simulazione parte con il server: solo per banchi di prova dedicati |
| **Driver simulabili** | i driver su cui la simulazione può scrivere i sensori: **solo PLC virtuali o in collaudo** |
| **Tag heartbeat** | contatore incrementato ogni 0,5 s mentre la simulazione gira (watchdog nel PLC). Con più PLC: più tag separati da virgola |
| **Tag simulazione attiva** | vero mentre la simulazione gira: il PLC usa gli ingressi simulati. Viene riscritto a ogni heartbeat, così un PLC riavviato lo riceve. Anche qui più tag separati da virgola |

## Materiali

Le unità di carico generate dalle sorgenti:

| Campo | Significato |
|---|---|
| **Lunghezza** | nel senso di marcia, in metri |
| **Larghezza** | in metri |
| **Massa min / max** | ogni unità ha una massa casuale nell'intervallo; lo scanner la scrive come peso |
| **Peso nel mix** | frequenza relativa nelle sorgenti (es. 3 = tre volte più frequente di 1) |
| **Colore** | colore dell'unità nel runtime |

## Verifica del modello

Il workspace **⚛ Fisica** mostra il modello compilato dalle **viste salvate** e dal **progetto applicato**: i componenti per vista, con lunghezze, collegamenti e posizioni.

Errori tipici:
- tag sconosciuti;
- sensori su driver non abilitati o su tag non scrivibili;
- *Successivo* inesistenti;
- fotocellule lontane da qualunque nastro.

I componenti con errori sono esclusi; il resto del modello funziona.

## PLC S7-1500 e PLCSIM Advanced

Il PLC non deve leggere i sensori direttamente da `%I`, ma da un'immagine degli ingressi che in collaudo arriva dalla simulazione. Il file **`plc/tia/SimIO.scl`** del progetto contiene:

| Blocco | Contenuto |
|---|---|
| DB **`SimIO`** (accesso ottimizzato disattivato) | `Active`, `Heartbeat` e gli ingressi simulati `Sim.*`, scritti da PTWCS |
| DB **`In`** | l'immagine degli ingressi usata dal programma |
| **`FB_SimIO`** | copia in `In` gli ingressi simulati se la simulazione è abilitata e attiva e l'heartbeat è vivo, altrimenti gli ingressi reali |

Passi:
1. In TIA Portal importa `SimIO.scl` (*Origini esterne → Genera blocchi dall'origine*).
2. Adatta la struttura `SimIO_Inputs` ai sensori della macchina.
3. Chiama `FB_SimIO` all'inizio di OB1 e usa `"In".V.*` al posto degli ingressi.
4. Nella CPU abilita **Consenti accesso PUT/GET**.
5. Avvia **PLCSIM Advanced** con l'interfaccia di rete virtuale e carica il programma.
6. In PTWCS crea un driver `s7` verso l'indirizzo della CPU virtuale e i tag sugli indirizzi del DB:
   - attivo `DB…DBX0.0`;
   - heartbeat `DB…DBW2`;
   - fotocellule `DB…DBX4.x`;
   - barcode `DB…DBB8` (stringa).
7. In **⚛ Fisica**:
   - spunta il driver del PLCSIM tra i **driver simulabili**;
   - imposta il tag heartbeat e il tag simulazione attiva;
   - collega i sensori dei componenti ai tag `SimIO.*`.

Sicurezza:
- su un PLC di produzione lascia **`SimEnable = FALSE`**: gli ingressi simulati vengono ignorati anche se qualcuno scrive il DB;
- se PTWCS si ferma, il **watchdog** (2 s senza heartbeat) riporta il programma sugli ingressi reali.

Lo stesso schema funziona con un **PLC reale in collaudo** (driver S7 verso la CPU): il fattore tempo deve restare ×1.

### Ponte PLCSIM Advanced (I/O diretto)

Con PLCSIM Advanced si può fare a meno del DB `SimIO`: il **ponte** `plc/plcsim-bridge` (programma Windows in C#) scrive direttamente l'immagine di processo della CPU virtuale. Il programma PLC resta identico a quello di produzione.

| Driver `modbus` di PTWCS verso il ponte | CPU virtuale |
|---|---|
| `coil:N` | `%I (N/8).(N%8)`: sensori bool scritti dalla fisica |
| `di:N` | `%Q (N/8).(N%8)`: attuatori bool del PLC |
| `hr:N` | `%IW(2·N)`: sensori analogici |
| `ir:N` | `%QW(2·N)`: uscite analogiche |
| `hr:60000` | fattore tempo × 100 dell'istanza (ScaleFactor) |

Passi:
1. Avvia il ponte sul PC con PLCSIM Advanced.
2. Aggiungi in `plant.yaml` un driver `modbus` verso il ponte (porta 5030) e spuntalo tra i **driver simulabili**.
3. Imposta **Tag fattore tempo PLC** su un tag `hr:60000` scrivibile. Così, cambiando il fattore tempo nella pagina Simulazione, anche la CPU virtuale accelera o rallenta e i tempi del PLC (timer, timeout) restano coerenti con la fisica.
4. Per lo SCADA (DB, allarmi, diagnostica) continua a usare il driver `s7` verso la CPU virtuale.

Istruzioni di compilazione e uso: `plc/plcsim-bridge/README.md`.

## Demo

Ci sono due demo: la linea di smistamento (piccola) e l'hub di smistamento pacchi (grande).

### Linea di smistamento

`python3 tools/build_demo_physics.py` aggiunge il layer fisico alla linea di smistamento della demo:
- **tag** `SimIO.*` sul PLC di linea;
- **materiali**: collo, pacco, busta;
- **vista Linea**: ingresso colli → CV1 → CV2 con scanner SC1 e fotocellula → CV3 con fotocellula → deviatore DV1 → uscite 1–3 e scarto, ciascuna con la fotocellula di fine.

Il PLC simulato `plc_sim`, con la simulazione attiva, fa la stessa cosa di `FB_SimIO` con il watchdog ed esegue la logica vera:
- a ogni lettura dello scanner manda la richiesta al WCS;
- insegue i colli in una coda;
- comanda il deviatore quando il collo arriva alla fotocellula;
- conta i colli alle uscite;
- va in guasto se un collo resta fermo su una fotocellula.

WCS e regole di smistamento decidono le uscite come in produzione. Le prove si fanno da runtime → **Simulazione**.

### Hub di smistamento pacchi

Si genera con `python3 tools/build_demo_hub.py` (server avviato), poi si avvia il suo PLC:

```bash
cargo run --release --bin plc_hub
```

Ha 69 nastri, 53 fotocellule, 3 scanner, 15 deviatori e 6 linee, su quattro viste:

| Vista | Area | Contenuto |
|---|---|---|
| **Hub – panoramica** | Hub | stato delle linee, avvio e arresto di tutte le linee, conteggi per baia, scivolo e ricircolo, navigazione |
| **Hub – Ricevimento** | Hub-Ricevimento (Baia 1–3, Collettore) | 3 baie di scarico camion con merge sul collettore, pre-smistatore con scanner SC0 → Nord / Sud / scarti, linea scarti con postazione di codifica manuale |
| **Hub – Sorter Nord** / **Sud** | Hub-SorterNord / Hub-SorterSud | sorter ad anello: immissione con merge del ricircolo, scanner, 7 deviatori (6 scivoli per città + scarti), ricircolo |

Il PLC `plc_hub` legge la sua configurazione da `config/hub/plc_hub.json`, generato dal builder insieme a tag e viste. La logica:

| Funzione | Comportamento |
|---|---|
| **Linee** | ogni linea si avvia e si ferma con il suo pulsante; un nastro è in marcia se la linea è abilitata e il nastro non è in guasto |
| **Merge** | l'immissione attende se la fotocellula della linea principale è occupata. Per le baie la principale è il collettore, per il sorter il ricircolo. Dopo 6 s passa comunque (anti-blocco) |
| **Controllo motori** | marcia comandata da 2,5 s senza velocità reale → guasto e allarme. Si prova con **⚡ Avaria motore** nella pagina Simulazione |
| **Scanner** | a ogni lettura parte la richiesta al WCS (punti di decisione `HSC0`, `HSCN`, `HSCS`, script `hub_presort`, `hub_sort_nord`, `hub_sort_sud`) e il collo viene inseguito in una coda per ogni deviatore |
| **Deviatori** | il pre-smistatore va sulla destinazione ricevuta. Nel sorter ogni deviatore devia se il collo è per il suo scivolo e lo scivolo non è pieno (fotocellula d'ingresso occupata da più di 1,5 s); altrimenti il collo prosegue e, dopo l'ultimo, ricircola e viene riletto |
| **Risincronizzazione** | le code si azzerano quando la simulazione riparte. Le voci più vecchie di 30 s vengono scartate |

Il sorter separa i colli: il nastro dello scanner richiede 35 cm di distanza tra le unità e le sezioni del sorter sono più veloci dell'immissione.

### Impianto di processo (fluidi)

Si genera con `python3 tools/build_demo_fluids.py`, dopo `build_demo_process.py`. Crea la vista **Impianto di processo**:

```
acqua di rete ─V_IN─▶ T1 (5 m³) ─P1─▶ collettore ─R1 carico─▶ R1 (0,5 m³, quota 3 m) ─R1 scarico─▶ T2 (6 m³) ─P2─V_OUT─▶ autobotte
                                              └─R2 carico─▶ R2                     ─R2 scarico─┘
```

| Parte | Chi la comanda |
|---|---|
| Valvole di carico e scarico, riscaldatori, agitatori dei reattori | le **fasi ISA-88** dei batch (Carico, Riscaldo, Agitazione, Scarico) |
| Reintegro di T1, pompa P1, svuotamento di T2 | lo script a timer `proc_logic`, che fa da PLC di servizio |
| Livelli, temperature, portate, velocità degli agitatori | il **layer fisico** |

Avviando un batch da **Processo → Batch** con la simulazione attiva:
1. la pompa P1 parte e il reattore si riempie fino al livello della ricetta;
2. il riscaldatore da 250 kW porta il contenuto al setpoint e il termostato lo mantiene;
3. lo scarico svuota il reattore per gravità nel serbatoio prodotto.

Con la simulazione ferma, lo script `sim_reactor` torna a simulare i reattori in modo semplificato.
