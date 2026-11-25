# Designer – Tag e UDT

Il workspace **🏷 Tag e UDT** ha quattro sezioni:
- [Tag di progetto](#tag-di-progetto)
- [Tipi UDT](#tipi-udt)
- [Istanze UDT](#istanze-udt)
- [Trend](#trend)

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

La libreria standard offre già i tipi delle famiglie di simboli (nastro, deviatore, lift, traslo…), importabili nel progetto.

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

## Trend

Scegli uno o più tag e l'intervallo per vedere storico e valori live. Solo i tag con **Storico** hanno dati passati.
