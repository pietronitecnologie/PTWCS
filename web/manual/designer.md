# Designer – Panoramica e viste grafiche

Il Designer (`http://<server>:8080/designer.html`) configura l'intero progetto. In alto ci sono i **workspace**:

| Workspace | Manuale |
|---|---|
| 🖼 **Viste** | questa pagina |
| 🏷 **Tag e UDT** | [Tag e UDT](designer-tag.md) |
| 🔀 **Smistamento** | [Smistamento](designer-smistamento.md) |
| ⚙ **Progetto** | [Progetto](designer-progetto.md) |
| 🐍 **Script** | [Script Python](designer-script.md) |

Come si salva:
- **Salva** (Ctrl+S) salva la vista aperta;
- **Salva progetto** (● = modifiche) convalida e applica **a caldo** tag, UDT, aree, protocollo, classi, WMS e magazzini, riavviando solo i driver coinvolti;
- **Runtime ↗** apre l'interfaccia operatore.

I pannelli sinistro e destro si **ridimensionano** trascinando il bordo (doppio click = larghezza predefinita).

## Elenco viste

Nella scheda **Viste** del pannello sinistro:
- **+ Nuova vista**: chiede id e titolo.
- Click apre la vista; ⧉ duplica; ✕ elimina.
- **Faceplate / popup**: gruppo con le viste aperte come finestre.
  - 📚 = faceplate della [libreria standard](#libreria-standard) (sola lettura);
  - ✎ = faceplate standard personalizzato nel progetto;
  - ↺ = ripristina lo standard.
- **Tasto destro** su una vista: apri, apri nel runtime, duplica, elimina o ripristina.

## Proprietà della vista

Con nessun componente selezionato, il pannello destro mostra:

| Campo | Significato |
|---|---|
| **Titolo** | nome nel menu e nella barra del faceplate |
| **Larghezza / Altezza** | dimensioni di disegno (la vista si adatta allo schermo mantenendo le proporzioni) |
| **Sfondo** | colore di sfondo |
| **Pagina iniziale** | vista aperta all'avvio del runtime (una sola) |
| **Area** | area di appartenenza: la vista compare nel menu dell'area nella barra delle aree |
| **Faceplate/popup** | vista aperta come finestra flottante, nascosta dai menu |
| **Nel menu** | togli la spunta per le sotto-pagine raggiunte solo via navigazione |
| **Parametri** | parametri ricevuti dalla vista (es. `device`), usabili nei binding come `{device}` |
| **Livelli** | elenco dei componenti; click per selezionarli |

## Componenti

Scheda **Componenti**: trascina sulla vista, o doppio click per aggiungere al centro.

| Componente | Uso |
|---|---|
| Nastro, motore, lampada, valore, testo, pulsante, serbatoio, barra | elementi base |
| Rettangolo, ellisse, linea/tubo | forme, anche animate (flusso) |
| Simbolo SVG | simbolo della libreria o importato |
| Vista incorporata | un'altra vista dentro questa, con parametri |
| Trend | grafico storico + live di uno o più tag |
| Mappa magazzino, KPI magazzino, missioni | componenti del [WCS](wcs.md) |

### Modifica

- **Selezione**: click; Maiusc/Ctrl+click aggiunge alla selezione; trascinando su un'area vuota si seleziona con un rettangolo.
- **Sposta**: trascina, oppure frecce (Maiusc = 10 px).
- **Ridimensiona**: maniglie (Maiusc = proporzionale).
- **Griglia**: aggancio a 10 px (casella *Griglia*). **Zoom**: −/+, ⤢ adatta, Ctrl+rotella.
- **Anteprima**: i click eseguono le azioni come nel runtime.

### Menu del tasto destro

| Su un componente | Su un'area vuota |
|---|---|
| Taglia, Copia, Incolla, **Incolla qui**, Duplica, Elimina | **Incolla qui** |
| Ordine: primo piano, avanti, indietro, in fondo | **Aggiungi componente** nel punto |
| Allinea e distribuisci (con più componenti selezionati) | Seleziona tutto, Annulla, Ripeti |
| Ruota 90°/180°, azzera rotazione | Griglia, Anteprima, Adatta, Zoom 100% |
| Apri faceplate / vista collegata, Rinomina | Salva vista, Apri nel runtime |

Gli **appunti** valgono tra viste diverse e tra schede del browser, quindi puoi copiare da una vista e incollare in un'altra.

### Scorciatoie

| Tasti | Azione |
|---|---|
| Ctrl+C / Ctrl+X / Ctrl+V | copia, taglia, incolla |
| Ctrl+D | duplica |
| Canc | elimina |
| Ctrl+A | seleziona tutto |
| Ctrl+Z / Ctrl+Y | annulla / ripeti |
| Ctrl+S | salva vista |
| Esc | deseleziona |

## Proprietà del componente

| Sezione | Contenuto |
|---|---|
| **Posizione** | X, Y, W, H, rotazione |
| **Proprietà personalizzate** | es. `device = CV1`: usabili nei binding come `{device}.Run` e passabili ai faceplate |
| **Proprietà** | specifiche del tipo (testo, colori, unità…). Il pulsante 🔗 apre il [binding](#binding) |
| **Comuni** | visibile, opacità, tooltip |
| **Elementi del simbolo** | parti animabili di un simbolo SVG: fill, stroke, visible, opacity, text, rotate, tx/ty, flow, blink, spin |
| **Evento: click** | [azioni](#eventi-e-azioni) eseguite al click |

### Binding

Un binding collega una proprietà a un valore live.

| Campo | Significato |
|---|---|
| **Sorgente** | *Tag* (valore di un tag) oppure *Espressione* |
| **Tag** | nome del tag; accetta percorsi indiretti come `{device}.Speed` |
| **Espressione** | JavaScript con `tag('Nome')` e `param('device')`, es. `tag('{device}.Fault') ? '#ef4444' : '#22c55e'` |
| **Trasformazione** | *Mappa*: tabella valore → risultato, con default. *Espressione*: `value * 0.1`. *Formato*: decimali e unità |
| **Anteprima live** | valore calcolato in questo momento |

Puoi anche **trascinare un tag** (scheda *Tag*) sulla vista per creare un indicatore, oppure su un componente per collegarlo.

### Eventi e azioni

Il click di un componente esegue in sequenza un elenco di azioni (con **Conferma** facoltativa):

| Azione | Campi |
|---|---|
| **Scrivi tag** | tag; valore fisso, `toggle` o `prompt` (chiede il valore) |
| **Esegui script** | funzione Python degli [eventi HMI](designer-script.md); payload JSON |
| **Vai a vista** | vista di destinazione e parametri (es. `aisle = 2`) |
| **Apri popup / faceplate** | vista popup e parametri, es. `device = {device}` per passare il tag del simbolo al faceplate |
| **Magazzino** | cerca UdC, prelievo, deposito su un magazzino del WCS |

## Simboli e libreria standard

Scheda **Simboli**: menu **Intralogistica**, **Macchine** e **Processo**, divisi per categoria, con ricerca.

Al trascinamento, un simbolo della libreria riceve:
- la proprietà `device`, da impostare con l'istanza UDT (es. `CV1`);
- i **binding predefiniti** della sua famiglia (colore di stato, flusso, guasto…);
- il click che apre il suo **faceplate** con `device` come parametro.

**Scala**: 30 unità = larghezza di un nastro.
- I trasporti hanno lunghezza variabile: allungandoli aumentano i rulli, che non si deformano.
- Gli altri simboli hanno una dimensione nativa coerente con questa scala.

**⬆ Importa SVG**:
- gli elementi con un `id` diventano animabili;
- script ed elementi pericolosi vengono rimossi.

### Libreria standard

Simboli, faceplate e UDT standard fanno parte dello SCADA (cartella `stdlib/`) e sono disponibili in **ogni progetto**, in sola lettura.
- Per adattare un faceplate o un simbolo, aprilo e salvalo: nasce una **personalizzazione del progetto** (✎) con lo stesso id, che ha la precedenza.
- **↺ Ripristina** elimina la personalizzazione e torna allo standard.
