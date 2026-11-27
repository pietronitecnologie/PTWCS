# Designer – Progetto

Il workspace **⚙ Progetto** ha sei sezioni:
- [Aree](#aree)
- [Scan class](#scan-class)
- [Script a timer](#script-a-timer)
- [Magazzini](#magazzini-automatici)
- [Sicurezza](#sicurezza): login, ruoli, permessi, utenti
- [Driver PLC](#driver-plc)

Le modifiche si attivano con **Salva progetto**.

## Aree

Un'area è una parte dell'impianto: raccoglie allarmi ed errori e organizza le viste nella barra delle aree del runtime.

| Colonna | Significato |
|---|---|
| **Nome** | identificativo, mostrato nella barra delle aree |
| **Descrizione** | tooltip nel runtime |
| **Area padre** | per annidare le aree: l'area padre comprende gli allarmi delle figlie |

Cosa finisce in un'area:
- gli allarmi dei tag dell'area;
- la qualità BAD dei tag (`<area>.QUALITY`);
- la perdita di comunicazione dei driver assegnati all'area (`COMM.<driver>`);
- gli errori dei punti di decisione dell'area.

Le viste con quell'**Area** compaiono nel suo menu.

## Scan class

| Colonna | Significato |
|---|---|
| **Nome** | identificativo, assegnabile a tag, membri UDT e istanze |
| **Periodo (ms)** | ogni quanto vengono letti i tag |
| **Descrizione** | testo libero |

I tag senza scan class usano il `poll_ms` del driver. I gruppi in scadenza nello stesso ciclo vengono letti insieme, a blocchi.

> **Consiglio** – usa 100–250 ms per telegrammi e sensori veloci, 1–5 s per misure lente e contatori.

## Script a timer

Funzioni Python eseguite periodicamente nel processo HMI, separato dalle decisioni in tempo reale.

| Colonna | Significato |
|---|---|
| **Nome** | identificativo del timer |
| **Funzione** | script del workspace [Script](designer-script.md) |
| **Intervallo (ms)** | periodo di esecuzione |
| **Attivo** | abilita o disabilita |

Usi tipici: calcolo KPI, logiche di supporto, simulazioni demo (`sim_pallet`).

## Magazzini automatici

Configura i magazzini a trasloelevatori gestiti dal [WCS](wcs.md). **+ Nuovo** ne crea uno, **Elimina** lo rimuove (le locazioni restano nel database).

### Geometria e generali

| Campo | Significato | Esempio |
|---|---|---|
| **Nome** | identificativo, usato in `store:<nome>` nelle regole | `MP1` |
| **Descrizione**, **Area** | testo e area di appartenenza | |
| **Lati per corsia** | 1 o 2 scaffalature per corsia | `2` |
| **Colonne** | posizioni lungo la corsia (colonna 1 = vicino al P&D) | `50` |
| **Livelli** | piani (livello 1 = basso) | `10` |
| **Colonne per blocco** | aggregazione della mappa nella vista d'insieme | `5` |
| **Tempo per colonna / per livello (s)** | tempi di traslazione per il calcolo delle zone e dei guadagni | `0.6` / `1.2` |
| **Altezza max per livello (mm)** | dal livello 1, separati da virgola; vuoto = nessun limite | `1800, 1800, 1500` |
| **Ingressi** / **Baie d'uscita** | nomi di ingressi e baie (le baie sono numerate da 1 in `DstOut`) | `IN1, IN2` / `BU1, BU2` |
| **Prefisso UdC demo**, **Riempimento iniziale %**, **Articoli demo** | solo per la demo, alla prima creazione | |
| **Prelievo / Ordine casuale ogni (s)** | generatori demo; 0 = disattivi | |

Le locazioni totali sono corsie × lati × colonne × livelli.

### Corsie e trasloelevatori

| Colonna | Significato |
|---|---|
| **Corsia** | nome della corsia |
| **Traslo (istanza UDT)** | istanza con il telegramma missione: `Mission`, `Type` (1 deposito, 2 prelievo, 3 movimentazione), `SrcSide/SrcCol/SrcLvl`, `DstSide/DstCol/DstLvl`, `DstOut`, `Start`, `Busy`, `Done`, `Fault` (+ `Auto`) |
| **Posti UdC** | 1, oppure 2 per i trasli a **doppia UdC**: servono i membri con suffisso `2` (`Mission2`, `Done2`…) |
| **Attesa compagna (s)** | solo doppia UdC: quanto aspettare una seconda missione da abbinare prima di partire con una sola |

Sequenza del telegramma:
1. il WCS scrive la missione e alza `Start`;
2. il PLC alza `Busy`, e il WCS abbassa `Start`;
3. a fine missione il PLC scrive `Done = id missione` e abbassa `Busy`.

Una corsia con `Fault` alto oppure `Auto` basso non riceve missioni.

### Strategia di stoccaggio

| Campo | Significato | Default |
|---|---|---|
| **Classi ABC** | attiva zone e classi | sì |
| **Zona A / Zona B (quota celle)** | quota delle celle più vicine al P&D in zona A e B; il resto è zona C | 0.2 / 0.3 |
| **Classe A / B (quota articoli)** | quota degli articoli più prelevati in classe A e B; il resto, e gli articoli mai prelevati, in classe C | 0.2 / 0.3 |
| **Finestra frequenze (h)** | periodo su cui contare i prelievi | 168 |
| **Distribuisci articolo su più corsie** | penalizza le corsie che hanno già lo stesso articolo, così un traslo fermo non blocca tutto l'articolo | sì |

### Riorganizzazione automatica

| Campo | Significato | Default |
|---|---|---|
| **Attiva** | stato iniziale; commutabile dal runtime | no |
| **Inattività corsia (s)** | secondi senza missioni prima di una movimentazione | 15 |
| **Mosse max per ora/corsia** | limite di movimentazioni | 60 |
| **Guadagno minimo (s)** | riduzione minima del tempo di traslazione per una compattazione | 2 |

Vedi [Riorganizzazione](wcs.md#riorganizzazione) per l'algoritmo e gli indicatori.

## Sicurezza

Login, ruoli e permessi. Si configurano in **⚙ Progetto → 🔐 Sicurezza**; per modificarli serve il permesso `users.manage`.

| Cosa | Dove viene salvato | Quando ha effetto |
|---|---|---|
| Impostazioni e ruoli | nel progetto (`project.yaml`) | con **Salva progetto** |
| Utenti e password | nel database del server, mai nel progetto | subito |

### Impostazioni

| Campo | Significato |
|---|---|
| **Login e permessi attivi** | se disattivato non c'è login e tutti possono tutto (installazioni di prova). Prima di attivarlo verifica di conoscere la password di un amministratore |
| **Ruolo senza login** | permessi di chi usa il runtime senza accedere, tipicamente *visualizzatore* per un monitor di reparto. *(nessuno)* = login obbligatorio |
| **Durata sessione (min)** | dopo questo tempo serve un nuovo login |
| **Logout per inattività (min)** | la sessione scade se resta inutilizzata; 0 = mai |

**Primo avvio**: se nel database non c'è nessun utente, il server crea `admin` con password `admin` e cambio obbligatorio al primo accesso. In alternativa la password iniziale si imposta con la variabile d'ambiente `PTWCS_ADMIN_PASSWORD`.

### Ruoli e permessi

La tabella ha una colonna per ruolo e una riga per permesso: spunta i permessi di ogni ruolo.
- Il nome del ruolo si modifica in testa alla colonna, ✕ elimina il ruolo, **+ Ruolo** ne aggiunge uno.
- **\* tutti i permessi** dà ogni permesso, compresi quelli futuri.

| Permesso | Consente |
|---|---|
| `runtime.view` | aprire il runtime e le viste (salvo restrizioni della singola vista) |
| `tag.write` | scrivere tag: comandi da pulsanti e faceplate, pagina Tag (salvo ruoli del tag) |
| `alarm.ack` | riconoscere gli allarmi |
| `script.call` | eseguire script Python dai pulsanti delle viste |
| `wh.operate` | WCS: missioni, ordini, prelievi, depositi, movimentazioni, blocco celle |
| `wh.config` | WCS: riorganizzazione automatica e rettifiche dell'anagrafica UdC |
| `seq.operate` | fasi (sequenze) in manuale: avvio, hold, restart, stop, abort, reset |
| `batch.operate` | avviare un batch (allocando i dispositivi) e comandarlo, comprese le sue fasi |
| `recipe.edit` | creare, modificare ed eliminare le ricette (procedure) |
| `sim.operate` | simulazione fisica: avvio, arresto, fattore tempo, guasti e interventi sulle unità |
| `audit.view` | consultare ed esportare l'audit trail |
| `designer` | aprire il Designer in consultazione |
| `designer.views` | modificare viste, faceplate e simboli |
| `designer.project` | modificare e salvare il progetto (tag, UDT, aree, protocollo, classi, WMS, magazzini) |
| `rules.edit` | modificare le regole di smistamento |
| `scripts.edit` | modificare, testare e ricaricare gli script Python |
| `users.manage` | gestire utenti, ruoli e impostazioni di sicurezza; solo con questo permesso si possono modificare ruoli e permessi nel progetto |

Ruoli predefiniti:

| Ruolo | Uso |
|---|---|
| amministratore | tutto (`*`) |
| progettista | Designer completo e runtime |
| manutentore | runtime e WCS completi, Designer in sola lettura |
| operatore | conduzione: comandi, allarmi, missioni WCS, fasi e batch |
| visualizzatore | sola visualizzazione |

Deve sempre esistere almeno un ruolo con `users.manage` (o `*`): il progetto non si salva senza.

### Utenti

| Azione | Note |
|---|---|
| **+ Nuovo utente** | nome utente (lettere, numeri, `. _ - @`), nome completo, ruoli, attivo, password (min. 6 caratteri), *cambio al primo accesso* |
| **Modifica** | nome, ruoli, attivo/disattivato. Un utente disattivato perde subito le sue sessioni |
| **Password** | imposta una nuova password (con cambio al prossimo accesso) e chiude le sessioni aperte dell'utente |
| **Elimina** | non puoi eliminare te stesso né l'ultimo amministratore attivo |

**Sessioni attive** elenca chi è collegato in questo momento.

Protezioni:
- password salvate con hash Argon2;
- dopo 5 tentativi errati l'utente resta bloccato per un tempo crescente;
- cookie di sessione `HttpOnly` / `SameSite=Strict`;
- login, logout, tentativi falliti e modifiche agli utenti finiscono nell'audit, e ogni azione nell'audit riporta l'utente che l'ha fatta.

### Ruoli di viste, tag e comandi

Oltre ai permessi generali si possono restringere i singoli elementi. Nessun ruolo selezionato = nessuna restrizione.

| Dove | Campo | Effetto |
|---|---|---|
| Proprietà della **vista** (Designer → Viste) | **Ruoli accesso** | solo questi ruoli vedono la vista nei menu e possono aprirla |
| Proprietà della **vista** | **Ruoli comando** | solo questi ruoli possono usare pulsanti e comandi della vista; gli altri la vedono in sola lettura |
| **Azione** di un evento click | **Ruoli** | solo questi ruoli possono eseguire quell'azione (es. *Reset* solo per manutentore) |
| **Tag** (Tag e UDT → tag di progetto) | **Ruoli scrittura** | solo questi ruoli possono scrivere il tag, da qualsiasi vista, faceplate o pagina |
| **Membro UDT** / **istanza UDT** | **Ruoli scrittura** | come sopra per i tag generati; il membro ha la precedenza sull'istanza |

Il server verifica sempre permessi, ruoli dei tag e ruoli di comando delle viste: nascondere un pulsante non basta ad aggirarli. I ruoli della singola azione sono un controllo dell'interfaccia; per una protezione completa metti i ruoli sul **tag**.

## Driver PLC

Elenco in sola lettura dei driver definiti in `plant.yaml`, con stato live: connessione, ciclo, letture, scritture, errori.

Esempio di driver in `plant.yaml`:

```yaml
drivers:
  - name: PLC_LINEA
    protocol: s7          # s7 | modbus | opcua | eip | sim
    host: 192.168.0.10
    rack: 0
    slot: 1
    poll_ms: 200
    area: Linea
```
