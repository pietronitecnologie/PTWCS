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

## Sequenze

Menu **Processo**: una scheda per ogni sequenza con stato, passo corrente, tempo nel passo e avanzamento.

| Stato | Significato |
|---|---|
| inattiva | mai avviata o dopo un reset |
| in esecuzione | sta eseguendo i passi |
| in pausa | le transizioni e i timeout sono sospesi |
| completata | ha raggiunto la FINE |
| arrestata | fermata dall'operatore (azioni di arresto eseguite) |
| in errore | timeout o azione fallita: c'è l'allarme `SEQ.<nome>` |

Comandi (permesso `seq.operate`):

| Comando | Effetto |
|---|---|
| **▶ Avvia** | chiede i parametri di avvio (es. la **ricetta**) e, facoltativamente, il passo da cui partire |
| **❚❚ Pausa / ▶ Riprendi** | sospende e riprende; il tempo in pausa non conta per timeout e `@elapsed` |
| **✓ Conferma** | compare nei passi che attendono la conferma dell'operatore (es. controllo qualità) |
| **⏭ Avanti** | forza il passo successivo senza attendere la transizione |
| **↪** (sul diagramma) | salta a un passo qualsiasi |
| **■ Arresta** | ferma la sequenza ed esegue le azioni di arresto (stato sicuro) |
| **↺ Reset** | riporta a *inattiva* una sequenza conclusa, arrestata o in errore, e fa rientrare l'allarme |

Una sequenza può partire anche dal PLC, con il tag di avvio configurato nel Designer.

Il **diagramma dei passi** mostra azioni e transizioni di ogni passo: il passo corrente è evidenziato in verde con il suo tempo, quelli già eseguiti hanno la durata. A destra ci sono l'**esecuzione corrente** (chi l'ha avviata, parametri, storico dei passi con durate e motivo di uscita) e le **ultime esecuzioni** con esito e durata.

## Ricette

**Processo → 🧪 Ricette**: scegli il tipo di ricetta; a sinistra ci sono le ricette e la **ricetta attiva** di ogni unità (ultima caricata).

| Azione | Permesso | Effetto |
|---|---|---|
| **+ Nuova** | `recipe.edit` | nuova ricetta con i valori predefiniti |
| **Salva** / **Salva come…** | `recipe.edit` | registra i valori: limiti min/max verificati; la versione aumenta se i valori cambiano |
| **Elimina** | `recipe.edit` | elimina la ricetta |
| **⇩ Leggi dal PLC** | `recipe.edit` | copia nei campi i valori attuali dell'unità (poi *Salva* per registrarli) |
| **⇄ Confronta** | — | mostra accanto a ogni parametro il valore sul PLC; ⚠ = diverso |
| **⬆ Carica su …** | `recipe.load` | scrive tutti i parametri sull'unità scelta (con conferma) |

Ogni salvataggio, eliminazione e caricamento finisce nell'audit trail con i valori prima e dopo.

## Audit trail

Menu **Audit** (permesso `audit.view`): il registro di chi ha fatto cosa, quando e da dove.

| Colonna | Contenuto |
|---|---|
| **Data e ora** | al millisecondo |
| **Utente** | utente autenticato e origine (hmi, designer, ricette, sequenza, auth…) |
| **Categoria** | 🖱 comandi, ⚙ configurazione, 🔐 sicurezza, 🔔 allarmi, 🧪 ricette, 🔁 sequenze, 🏬 wcs |
| **Azione** | es. Scrittura tag, Riconoscimento allarmi, Progetto salvato, Ricetta caricata, Avvio sequenza, Accesso fallito |
| **Oggetto** | tag, ricetta, sequenza, vista… |
| **Valore precedente / nuovo** | per le scritture: valore prima e dopo; per le ricette: i parametri cambiati |
| **Dettaglio** | es. per il progetto le sezioni modificate (`tags: +R1.Level ~CV1.Speed −OLD`) |
| **Client** | indirizzo IP del posto operatore (o `X-Forwarded-For` dietro reverse proxy) |

- **Filtri**: periodo (dal/al), utente, categoria, azione e testo libero (tag, valori, dettagli).
- **Pagine**: ◀ ▶ scorrono a pagine di 100 righe.
- **Aggiornamento in tempo reale**: le nuove registrazioni compaiono in cima appena avvengono.
- **⬇ CSV**: esporta con i filtri correnti (fino a 100 000 righe), nel formato per Excel con `;` come separatore. Anche l'esportazione viene registrata.

L'audit non viene mai cancellato automaticamente.

## Tema chiaro e scuro

Il pulsante **☀ / 🌙** in alto (anche nel Designer e nel manuale, o con il tasto destro → *Tema chiaro/scuro*) cambia il tema dell'interfaccia. La scelta viene ricordata dal browser; al primo accesso segue il tema del sistema operativo. Le viste grafiche mantengono i colori di sfondo scelti nel Designer.
