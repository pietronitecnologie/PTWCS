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
| **Successivo** | nastro, deviatore o pozzo in cui passano le unità alla fine del nastro |
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
| **Tag heartbeat** | contatore incrementato ogni 0,5 s mentre la simulazione gira (watchdog nel PLC) |
| **Tag simulazione attiva** | vero mentre la simulazione gira: il PLC usa gli ingressi simulati |

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

## Demo

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
