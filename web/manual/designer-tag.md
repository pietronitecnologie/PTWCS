# Designer – Tag e UDT

Il workspace **🏷 Tag e UDT** ha cinque sezioni:
- [Tag di progetto](#tag-di-progetto)
- [Tipi UDT](#tipi-udt)
- [Istanze UDT](#istanze-udt)
- [📚 Libreria standard](#libreria-standard)
- [Trend](#trend)

Tipi UDT, istanze e tag si possono [esportare in un file di testo e reimportare](#esporta-e-importa-in-formato-testo).

Le modifiche si attivano con **Salva progetto** e vengono applicate a caldo.

## Tag di progetto

| Colonna | Significato | Esempio |
|---|---|---|
| **Nome** | identificativo univoco; il punto separa i livelli | `Line.EStop` |
| **Driver** | collegamento PLC (definito in `plant.yaml`) | `PLC_LINEA` |
| **Indirizzo** | indirizzo nel protocollo del driver ([formati](#indirizzi)) | `DB10.DBX0.1` |
| **Tipo** | `bool`, `u8`, `i16`, `u16`, `i32`, `u32`, `f32`, `string` | `bool` |
| **Len** | lunghezza (solo `string`) | `20` |
| **R/W** | scrivibile dal WCS | ☑ |
| **Unità** | unità di misura mostrata | `m/s` |
| **Area** | area di appartenenza: i suoi allarmi ed errori di qualità finiscono qui | `Linea` |
| **Scan class** | frequenza di lettura; vuoto = `poll_ms` del driver | `veloce` |
| **Storico** | 📈 registra lo storico (trend) | ☑ |
| **Allarmi** | 🔔 apre l'editor degli [allarmi](#allarmi-sul-tag) | — |
| **Descrizione** | testo libero | Fungo di emergenza |

I tag generati dalle istanze UDT sono in sola lettura: si modificano dal tipo o dall'istanza.

### Allarmi sul tag

| Campo | Significato |
|---|---|
| **Nome** | id breve; l'allarme sarà `<tag>.<nome>` |
| **Op** | confronto: `==`, `!=`, `>`, `>=`, `<`, `<=` (per i bool: `== true`) |
| **Soglia** | valore di confronto |
| **Gravità** | info, avviso, allarme, critico |
| **Messaggio** | testo mostrato all'operatore |

L'allarme va nell'**area** del tag, che si colora nella barra delle aree.

### Indirizzi

| Protocollo | Formato | Esempi |
|---|---|---|
| Siemens S7 | `DB<n>.DBX<byte>.<bit>`, `DBB`, `DBW`, `DBD`; merker `M10.0`, `MW20`; ingressi `I0.0`/`IW64`; uscite `Q0.0`/`QW64` | `DB10.DBW4`, `DB10.DBX0.1` |
| Modbus TCP | `hr:<n>` holding register (`hr:<n>.<bit>` = bit), `ir:<n>` input register, `coil:<n>`, `di:<n>` | `hr:100`, `coil:5` |
| OPC UA | NodeId | `ns=2;s=Linea.CV1.Run` |
| EtherNet/IP | nome del tag Logix | `CV1_Run` |
| Interno / simulato | nome libero | `sim_speed` |

## Tipi UDT

Un tipo UDT descrive un dispositivo, ad esempio un *Nastro* con `Run`, `Fault` e `Speed`.

**Parametri** (es. `DB`, `Base`): valori che ogni istanza imposta.

**Membri**:

| Colonna | Significato |
|---|---|
| **Nome** | nome del membro: il tag sarà `<istanza>.<nome>` |
| **Tipo**, **Len**, **R/W**, **Unità** | come per i tag |
| **Indirizzo (template)** | indirizzo con segnaposto: `{Param}` è il valore di un parametro, `{Base+2}` un'espressione aritmetica, `{Instance}` il nome dell'istanza. Es. `DB{DB}.DBW{Base+2}` |
| **Ruolo protocollo** | per i telegrammi request/reply: `request (PLC)`, `response (WCS)`, `input (dato richiesta)`, `output (risposta)`. Vedi [Smistamento](designer-smistamento.md#ruoli-dei-membri-del-telegramma) |
| **Scan class**, **Storico**, **Allarmi** | come per i tag, ereditati da tutte le istanze |

La libreria standard offre già i tipi delle famiglie di simboli (nastro, deviatore, lift, traslo…), importabili nel progetto con **⬇ Dalla libreria**.

Pulsanti sul tipo selezionato:
- **⬆ Esporta**: solo questo tipo in un file di testo;
- **📚 In libreria**: copia il tipo nella [libreria standard](#libreria-standard) (poi **Salva libreria**);
- **Duplica**, **Elimina** (non se il tipo è usato da istanze).

## Istanze UDT

| Colonna | Significato |
|---|---|
| **Nome** | nome del dispositivo, prefisso dei tag (`CV1` → `CV1.Run`) |
| **Tipo UDT** | tipo da istanziare |
| **Driver** | PLC del dispositivo |
| **Parametri** | valori dei parametri del tipo, es. `DB = 101` |
| **Area** | area di tutti i tag dell'istanza |
| **Scan class** | scan class predefinita dei tag dell'istanza |
| **Descrizione** | testo libero |

L'**anteprima** mostra i tag generati con gli indirizzi risolti: controlla qui che `{Base+2}` dia l'indirizzo atteso.

## Libreria standard

La scheda **📚 Libreria standard** modifica i tipi UDT condivisi da tutti i progetti del server (file `udts.json` della libreria, quelli usati da simboli e faceplate standard). L'editor è lo stesso dei tipi di progetto:
- **+ Nuovo tipo**, **Duplica**, **Elimina**, parametri e membri;
- **→ Nel progetto**: copia il tipo nel progetto corrente;
- **⬇ Importa…** e **⬆ Esporta libreria**: file di testo (vedi sotto);
- **💾 Salva libreria** applica le modifiche (permesso `designer.project`); la versione precedente resta in `udts.json.bak`. **↺ Annulla modifiche** ricarica la libreria salvata.

Finché non salvi, la scheda mostra il pallino ●.

## Esporta e importa in formato testo

Il formato è YAML, leggibile e modificabile con un qualsiasi editor di testo (estensione `.ptlib.yaml`):

```yaml
format: ptwcs-tag-library
version: 1
name: Motori linea 2
udt_types:
- name: Motore
  params:
  - { name: DB, default: 100 }
  - { name: Base, default: 0 }
  members:
  - { name: Run, type: bool, address: "DB{DB}.DBX{Base+0}.0" }
  - { name: Speed, type: f32, address: "DB{DB}.DBD{Base+2}", unit: m/s }
udt_instances:          # facoltativo
- { name: M1, type: Motore, driver: PLC1, params: { DB: 101 } }
tags:                   # facoltativo
- { name: Linea2.Start, driver: PLC1, address: DB1.DBX0.0, type: bool, writable: true }
```

**Esportare**
- *Tipi UDT* → **⬆ Esporta…**: scegli i tipi; facoltativamente le istanze dei tipi scelti e i tag di progetto.
- *Tag di progetto* → **⬆ Esporta libreria…**: l'intera libreria del progetto (tutti i tipi, istanze e tag già selezionati).
- *Libreria standard* → **⬆ Esporta libreria**: tutti i tipi della libreria standard.
- **📋 Mostra testo** visualizza il testo da copiare; **⬆ Salva file** lo scarica (nel client desktop finisce in *Download*).

**Importare** (**⬇ Importa…** in *Tag di progetto*, *Tipi UDT* o *Libreria standard*)
1. Scegli un file oppure incolla il testo; sono accettati anche JSON, un elenco di tipi o un singolo tipo.
2. **🔍 Analizza** valida il testo (nomi ripetuti, indirizzi template non espandibili) e mostra cosa è *nuovo* e cosa *esiste* già.
3. Per i nomi già presenti scegli **sostituisci**, **salta** o **rinomina** (`Motore_2`; le istanze importate seguono il nuovo nome del tipo).
4. **⬇ Importa**, poi **Salva progetto** (o **Salva libreria**).

Nella libreria standard si importano solo i tipi UDT: istanze e tag del file vengono ignorati.

## Trend

Scegli uno o più tag e l'intervallo per vedere storico e valori live. Solo i tag con **Storico** hanno dati passati.
