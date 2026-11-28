# Runtime

Il runtime è l'interfaccia operatore: si apre con `http://<server>:8080/`.

## Accesso

Se la sicurezza è attiva:
- **🔐 Accedi** in alto apre il login. Se il progetto lo prevede, senza login il runtime resta visibile in sola lettura;
- se il login è obbligatorio, la schermata di accesso compare subito;
- **👤 utente ▾** mostra i ruoli e permette *Cambia password*, *Cambia utente* ed *Esci*;
- al primo accesso, o dopo un reset della password, può essere richiesto il **cambio password**;
- a sessione scaduta (durata massima o inattività) ricompare il login.

Cosa vedi dipende dai tuoi ruoli:
- le viste non consentite non compaiono nei menu;
- i comandi non consentiti mostrano *Permesso negato* (il cursore 🚫 indica una vista in sola lettura);
- nella pagina Tag i tag riservati hanno 🔒 *Scrivi* disattivato;
- nel WCS i pulsanti operativi compaiono solo con il permesso `wh.operate`.

## Struttura della pagina

| Zona | Contenuto |
|---|---|
| **Intestazione** | nome impianto, menu delle viste generali e delle pagine di sistema (WCS, Processo, Aree, Allarmi, Decisioni, Tag, Trend, Diagnostica, Audit), stato della connessione (● verde = collegato), ora, tema ☀/🌙, utente, manuale, link al Designer |
| **Barra delle aree** | sempre visibile, stile PCS7: un pulsante per area con lo stato degli allarmi |
| **Pagina** | la vista o la pagina di sistema aperta |
| **Barra allarmi** | in basso: allarme più recente, numero di allarmi attivi e da riconoscere, link alla pagina Allarmi |

## Barra delle aree

Ogni pulsante mostra:
- una **striscia colorata** a sinistra con la gravità peggiore attiva nell'area: verde OK, azzurro info, arancio avviso, rosso allarme, rosso scuro critico;
- i **contatori** per gravità: C critici, A allarmi, W avvisi, I info, e **Q** se ci sono tag con qualità BAD (PLC non raggiungibile, indirizzo errato…);
- il **lampeggio** se ci sono allarmi da riconoscere.

Click su un'area apre il **menu delle viste** dell'area e delle sue sotto-aree, con lo stato di ognuna, più *Panoramica area* e *Allarmi dell'area*. **⌂ Impianto** torna alla pagina iniziale.

## Navigazione

- **Menu in alto**: viste generali (non assegnate a un'area) e pagine di sistema.
- **Barra delle aree**: viste per area.
- **Click sui simboli**: aprono il faceplate o un'altra vista (azioni configurate nel Designer).
- **Tasto destro** in qualsiasi punto: apre il [menu di navigazione](#menu-del-tasto-destro).
- **Indietro/Avanti** del browser funzionano: ogni vista ha il suo indirizzo (`#/view/<id>`), copiabile nei preferiti.

### Menu del tasto destro

| Voce | Funzione |
|---|---|
| *(sopra un simbolo)* **Apri faceplate** | apre il faceplate del dispositivo, con il suo tag |
| *(sopra un simbolo)* **Vai a …** | apre la vista collegata al simbolo |
| *(sopra un simbolo)* **Tag di …** | pagina Tag filtrata sul dispositivo |
| **Indietro / Avanti** | cronologia di navigazione |
| **Pagina iniziale** | vista home dell'impianto |
| **Viste** | tutte le viste generali |
| **Aree** | per ogni area: stato, sottomenu con le viste dell'area e delle sotto-aree, panoramica, allarmi |
| **Pagine di sistema** | Aree, Allarmi, WCS, Decisioni, Tag, Trend, Diagnostica |
| **Aggiorna pagina** | ricarica la vista |
| **Schermo intero** | attiva o disattiva lo schermo intero (F11) |
| **Modifica vista nel Designer** | apre il Designer sulla vista corrente (se il Designer è abilitato) |

Sui campi di testo il menu aggiunge Taglia, Copia, Incolla, Elimina e Seleziona tutto; sulle tabelle aggiunge *Copia testo* e *Copia riga*; sulle selezioni aggiunge *Copia*. La voce **Manuale utente** (F1) apre questo manuale.

Maiusc + tasto destro apre il menu normale del browser.

## Faceplate

Il faceplate è una finestra flottante con i dati del dispositivo:
- **stato**: lampade di marcia, guasto, automatico…;
- **misure**: velocità, corrente, temperatura…;
- **comandi**: marcia, arresto, reset;
- **trend** degli ultimi minuti.

Si sposta trascinando la barra del titolo e si chiude con ✕. Si possono aprire più faceplate insieme. I comandi scrivono sul PLC: le scritture sono registrate nell'audit.

## Pagine di sistema

### Aree
Una scheda per area con allarmi attivi, da riconoscere e tag in errore. Il click apre la panoramica dell'area: allarmi, viste associate e sotto-aree.

### Allarmi
Elenco degli allarmi attivi e dello storico, filtrabile per area.
- **Riconosci** (o *Riconosci tutti*): l'allarme resta in elenco finché la condizione è attiva.
- Colonne: gravità, area, messaggio, valore, attivazione, rientro, riconoscimento.

### Decisioni
Log in tempo reale delle decisioni dei punti di smistamento: input ricevuti, uscite scritte, regola o script che ha deciso, durata. È utile per capire perché un collo è andato in una certa uscita.

### Tag
Tutti i tag con valore, qualità e ultimo aggiornamento. **Filtra** per nome. **Scrivi** imposta un tag scrivibile (richiede conferma; l'operazione va nell'audit).

### Trend
Grafico storico dei tag con storico attivo. Scegli i tag (spunte) e l'intervallo (5 min … 24 ore). Il grafico si aggiorna ogni 5 secondi.

### Diagnostica
Stato dei driver PLC (connessione, tempi di ciclo, letture, scritture, errori), degli script Python e log eventi del server.

### WCS
Gestione dei magazzini automatici: vedi [WCS magazzini automatici](wcs.md).

## Processo: batch e fasi

Menu **Processo** (modello ISA-88, vedi [Designer – Processo](designer-processo.md)). Ha cinque schede.

### Batch

Ogni batch attivo ha una scheda con:
- ricetta e versione, lotto;
- dispositivi allocati;
- passi attivi con il tempo (più passi se ci sono rami paralleli);
- stato e comandi.

**▶ Nuovo batch** (permesso `batch.operate`):
1. scegli la ricetta;
2. scrivi il lotto;
3. scegli un dispositivo per ogni alias richiesto: quelli allocati ad altri batch o con fasi manuali in corso non sono selezionabili;
4. imposta i parametri, entro i limiti.

Il dettaglio del batch selezionato mostra:
- il **diagramma SFC** della procedura (flowchart stile PCS7), con i passi attivi in verde e il tempo trascorso, e le durate di quelli eseguiti;
- le **fasi** avviate, con stato, passo corrente e parametri ricevuti;
- lo storico dei passi.

| Comando | Effetto |
|---|---|
| **❚❚ Hold** | mette in attesa batch e fasi (azioni di hold) |
| **▶ Restart** | riprende; dopo un errore riprova il passo fallito senza ripetere le fasi già concluse |
| **✓ Conferma** | compare quando un passo attende la conferma dell'operatore |
| **■ Stop** / **⛔ Abort** | arresto controllato o immediato di batch e fasi; i dispositivi vengono rilasciati |
| **↺ Reset** | toglie dall'elenco un batch concluso o in errore (in errore rilascia i dispositivi) |
| **▶ Riprova** (su una fase in errore) | riavvia la fase |

### Fasi

Le sequenze in **manuale** (permesso `seq.operate`):
- **▶ Avvia…** chiede il dispositivo (quelli allocati a un batch non sono disponibili) e i parametri;
- ogni istanza (`Sequenza@dispositivo`) ha i comandi hold, restart, stop, abort e reset;
- **◫** mostra il diagramma con il passo attivo.

Le fasi avviate da un batch compaiono con l'etichetta del batch e si comandano dal batch.

### Ricette

Elenco delle ricette con versione:
- **✎ Modifica**, **⧉ Duplica**, **Elimina** (permesso `recipe.edit`);
- **▶ Avvia batch**.

L'editor comprende dispositivi richiesti, parametri, **procedura** e azioni di stato. La procedura si programma con l'editor SFC a flowchart (vedi [Designer – Processo](designer-processo.md#editor-sfc-a-flowchart)):
- i passi avviano le fasi sui dispositivi con i loro parametri;
- le transizioni attendono *fasi concluse*, condizioni o la conferma dell'operatore;
- ci sono rami paralleli e salti.

### Dispositivi

Le unità per classe, **libere** o **allocate**, con il batch e la ricetta che le usano e le fasi in corso.

### Storico

I batch conclusi: ricetta e versione, lotto, dispositivi, esito, durata, utente. Il click apre il **report del batch**: parametri, passi della procedura con durate, fasi con parametri ed esito.

### Riavvio del server

Batch e fasi in corso vengono salvati di continuo. Dopo un riavvio del server tornano **in hold**, con le stesse allocazioni dei dispositivi: l'operatore verifica l'impianto e dà **Restart**.

## Simulazione fisica

Menu **Simulazione** (o tasto destro → **Simulazione fisica**). Compare quando le viste hanno componenti con proprietà fisiche; vedi [Designer – Fisica](designer-fisica.md).

Mentre la simulazione è in corso:
- l'intestazione mostra **⚛ SIMULAZIONE**;
- sulle viste le **unità di carico** si muovono sopra il layout. Passando il mouse su un'unità si vedono barcode e numero.

| Comando (permesso `sim.operate`) | Effetto |
|---|---|
| **▶ Avvia** / **■ Ferma** | avvia o ferma la fisica. All'arresto il tag *simulazione attiva* torna falso e il PLC torna agli ingressi reali |
| **❚❚ Pausa** / **▶ Riprendi** | congela le unità e i sensori |
| **⌫ Svuota** | rimuove tutte le unità e azzera il tempo |
| **Fattore tempo** | accelera o rallenta la simulazione: solo con PLC virtuali che lo seguono; con un PLC reale resta ×1 |

Il pannello mostra:
- unità generate, uscite, in impianto, letture e mancate letture;
- per i trasloelevatori: posizione degli assi, cella davanti alle forche, carico, prelievi e depositi, guasto o interblocco;
- per i fluidi: livello, volume, temperatura e pressione dei serbatoi, portate delle tubazioni, aperture delle valvole, velocità di pompe e agitatori, pompe a secco, troppo pieni;
- lo stato di ogni componente;
- l'elenco delle unità (barcode, materiale, peso, nastro, posizione, età);
- gli eventi.

### Guasti e interventi

| Componente | Intervento |
|---|---|
| Nastro | **⚡ Avaria motore**: il nastro non parte anche con la marcia comandata |
| Fotocellula | **bloccata occupata** / **bloccata libera** (fotocellula sporca o guasta) |
| Scanner | **⚡ Guasto** (nessuna lettura) o percentuale di mancate letture |
| Deviatore, fermo | **⚡ Blocca**: ignora i comandi |
| Sorgente | intervallo, **sempre attiva** / **ferma**, **+1** unità con un barcode a scelta |
| Unità | **Incastra** (si ferma e blocca le successive), **Libera**, **✕** rimuovi |
| Serbatoio | **⚡ Perdita** con la sua portata; **Livello…** per impostare livello e temperatura (prove) |
| Valvola | **⚡ Blocca** nella posizione attuale |
| Pompa | **⚡ Scatto termico**: si ferma e la retroazione di marcia cade; **Riarma** |
| Riscaldatore, agitatore | **⚡ Guasto** / **⚡ Avaria** |
| Trasloelevatore | **⚡ Guasto asse** X, Y o Z (azionamento fermo); **⚡ Cella vuota** (il prossimo prelievo non trova il carico); **↺ Reset** dei guasti e degli interblocchi |

Ogni comando e intervento finisce nell'audit trail (categoria *simulazione*).

## Audit trail

Menu **Audit** (permesso `audit.view`): il registro di chi ha fatto cosa, quando e da dove.

| Colonna | Contenuto |
|---|---|
| **Data e ora** | al millisecondo |
| **Utente** | utente autenticato e origine (hmi, designer, ricette, sequenza, auth…) |
| **Categoria** | 🖱 comandi, ⚙ configurazione, 🔐 sicurezza, 🔔 allarmi, 🧪 ricette, 🔁 sequenze e batch, ⚛ simulazione, 🏬 wcs |
| **Azione** | es. Scrittura tag, Riconoscimento allarmi, Progetto salvato, Ricetta caricata, Avvio sequenza, Accesso fallito |
| **Oggetto** | tag, ricetta, sequenza, vista… |
| **Valore precedente / nuovo** | per le scritture: valore prima e dopo; per ricette e batch: versione, stato, dispositivi e parametri |
| **Dettaglio** | es. per il progetto le sezioni modificate (`tags: +R1.Level ~CV1.Speed −OLD`) |
| **Client** | indirizzo IP del posto operatore (o `X-Forwarded-For` dietro reverse proxy) |

- **Filtri**: periodo (dal/al), utente, categoria, azione e testo libero (tag, valori, dettagli).
- **Pagine**: ◀ ▶ scorrono a pagine di 100 righe.
- **Aggiornamento in tempo reale**: le nuove registrazioni compaiono in cima appena avvengono.
- **⬇ CSV**: esporta con i filtri correnti (fino a 100 000 righe), nel formato per Excel con `;` come separatore. Anche l'esportazione viene registrata.

L'audit non viene mai cancellato automaticamente.

## Tema chiaro e scuro

Il pulsante **☀ / 🌙** in alto (anche nel Designer e nel manuale, o con il tasto destro → *Tema chiaro/scuro*) cambia il tema dell'interfaccia. La scelta viene ricordata dal browser; al primo accesso segue il tema del sistema operativo. Le viste grafiche mantengono i colori di sfondo scelti nel Designer.
