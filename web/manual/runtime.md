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
| **Intestazione** | nome impianto, menu delle viste generali e delle pagine di sistema (WCS, Aree, Allarmi, Decisioni, Tag, Trend, Diagnostica), stato della connessione (● verde = collegato), ora, link al Designer |
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
