# WCS-SCADA

Warehouse Control System con funzioni SCADA. Il core è in **Rust**, le regole di business sono in **Python** e l'HMI web ha un **Designer grafico** in stile Ignition Perspective.

- **Comunicazione PLC**: Siemens S7 (stack nativo, senza librerie C), Modbus TCP, OPC UA ed EtherNet/IP (Allen-Bradley Logix).
- **Decision engine**: risponde alle "domande" del PLC (es. "collo con barcode X allo scanner: dove lo mando?") con un handshake robusto ReqID/RespID.
- **SCADA**: sinottici live, allarmi (ISA-18.2 semplificato), trend storici, log delle decisioni, diagnostica dei driver e audit dei comandi.
- **Designer**: disegni le viste nel browser, importi simboli SVG e colleghi ogni proprietà a tag, espressioni e script Python.

## Avvio rapido (demo)

Servono Rust ≥ 1.85 e Python ≥ 3.9 (nessun pacchetto Python richiesto per il runtime).

```bash
cargo build --release
```

```bash
./target/release/plc_sim 127.0.0.1:5020
```

In un secondo terminale:

```bash
./target/release/wcs_scada config/plant.yaml
```

- Runtime HMI: http://localhost:8080/
- Designer: http://localhost:8080/designer.html

Il simulatore è un vero server Modbus TCP. Simula una linea di smistamento: 3 nastri, uno scanner barcode e un deviatore con 3 uscite più lo scarto. Genera colli e chiede al WCS la destinazione con l'handshake descritto sotto. Ci sono anche guasti casuali, emergenza e analogici.

## Architettura

```
 Browser ── Runtime HMI (index.html) ──┐        ┌── Designer (designer.html)
                                       │ REST + WebSocket
┌──────────────────────────────────────┴────────┴─────────────────────────┐
│ web.rs        API REST, WebSocket live (batch 50 ms), viste, simboli    │
│ decision.rs   handshake ReqID/RespID → script Python → uscite al PLC    │
│ alarms.rs     valutazione allarmi + allarmi di comunicazione automatici │
│ store.rs      SQLite: decisioni, eventi allarme, storico tag, audit     │
│ script.rs     2 processi Python isolati (decisioni / eventi HMI)        │
│ tagdb.rs      immagine live dei tag con qualità e timestamp             │
│ drivers/      un task per driver: polling a blocchi, coda scritture     │
│               prioritaria, riconnessione con backoff                    │
│   s7.rs · modbus.rs · opcua.rs · eip.rs · sim.rs                        │
└────────────────────────────────────┬────────────────────────────────────┘
                                     PLC
```

Scelte di progetto:
- **Gli script girano in processi separati.** Un errore, un crash o un loop infinito in Python non abbatte il WCS. Dopo il timeout il processo viene riavviato.
- **Ci sono due processi Python.** Uno serve i punti di decisione e uno gli eventi HMI, così un comando operatore lento non ritarda le risposte ai PLC.
- **Le scritture hanno priorità sul polling** e ognuna attende la conferma del driver.
- **Gli errori di configurazione emergono all'avvio**, non in produzione: tag, driver, writable e riferimenti vengono validati.

## Il protocollo di decisione (lato PLC)

Per ogni punto di decisione, in un'area dati contigua (es. un DB S7 o un blocco di holding register):

| Campo | Scritto da | Note |
|---|---|---|
| dati (barcode, peso…) | PLC | scritti **prima** di ReqID |
| `ReqID` | PLC | contatore ≠ 0, incrementato a ogni richiesta |
| uscite (destinazione, esito…) | WCS | |
| `RespID` | WCS | scritto **per ultimo** = ReqID |

Il PLC usa le uscite solo quando `RespID == ReqID`. Se la risposta non arriva entro il suo timeout, applica un default sicuro (es. scarto o ricircolo). Il WCS controlla anche ogni 500 ms se ci sono richieste rimaste in sospeso, ad esempio dopo una riconnessione. Se lo script fallisce o va in timeout, usa le uscite di `fallback`. Se il fallback non c'è, non risponde e lascia decidere il PLC.

> Lettura consistente: tieni dati e ReqID nella **stessa area contigua**, così il driver li legge nello stesso blocco.

## Configurazione (`config/plant.yaml`)

```yaml
drivers:
  - { name: PLC1, protocol: s7,     host: 192.168.0.10, rack: 0, slot: 1, poll_ms: 100 }
  - { name: PLC2, protocol: modbus, host: 192.168.0.11, port: 502, unit_id: 1 }
  - { name: PLC3, protocol: opcua,  endpoint: "opc.tcp://192.168.0.12:4840" }
  - { name: PLC4, protocol: eip,    host: 192.168.0.13, slot: 0 }
  - { name: MEM,  protocol: sim }                     # variabili interne
tags:
  - { name: SC1.ReqID, driver: PLC1, address: DB100.DBW0, type: u16 }
  - { name: SC1.Barcode, driver: PLC1, address: DB100.DBB2, type: string, len: 20 }
  - { name: SC1.RespID, driver: PLC1, address: DB100.DBW30, type: u16, writable: true }
decision_points:
  - name: SC1
    request_tag: SC1.ReqID
    response_tag: SC1.RespID
    inputs:  { barcode: SC1.Barcode }
    outputs: { destination: SC1.Dest }
    function: decide_sc1          # funzione Python
    timeout_ms: 300
    fallback: { destination: 99 }
alarms:
  - { id: T.HIGH, tag: CV1.Temp, op: ">", value: 450, message: "Temperatura alta", severity: warning }
```

Tipi: `bool u8 i16 u16 i32 u32 f32 string`. Opzioni dei tag: `writable`, `historize` (trend), `unit`, `description` e `initial` (solo per `sim`).

| Protocollo | Formato indirizzo | Note |
|---|---|---|
| S7 | `DB10.DBX0.1` `DB10.DBW2` `DB10.DBD4` `DB10.DBB8` `M10.0` `MW20` `I0.0`/`E0.0` `Q0.1`/`A0.1` | S7-1200/1500: abilitare PUT/GET e disattivare l'"accesso ottimizzato" sui DB. Stringhe = STRING S7 |
| Modbus | `coil:N` `di:N` `hr:N` `ir:N` `hr:N.bit` | offset 0-based, 32 bit = 2 registri big-endian |
| OPC UA | NodeId: `ns=3;s="DB_WCS"."ReqID"`, `ns=2;i=1001` | build con `--features opcua`. Per ora solo SecurityPolicy None |
| EtherNet/IP | nome tag Logix: `Prog:Main.ReqID`, `Arr[3]` | build con `--features eip`. Le STRING Logix sono in sola lettura |

```bash
cargo build --release --features opcua,eip
```

## Script Python (`scripts/rules.py`)

```python
def decide_sc1(req, ctx):            # punto di decisione
    if req["barcode"] == "NOREAD":
        ctx.log("lettura fallita")
        return {"destination": 99, "result": 1}
    return {"destination": 2, "result": 0}

def start_line(payload, ctx):        # evento HMI (pulsante)
    ctx.write("CV1.Run", True)
    return {"message": "Linea avviata"}   # mostrato all'operatore ({"navigate": "vista"} per cambiare pagina)
```

`ctx.tag(nome)`, `ctx.tags`, `ctx.write(tag, valore)`, `ctx.log(msg)`, `ctx.user`, `ctx.point`. Lo stato globale del modulo resta in memoria tra una chiamata e l'altra. Dal Designer (pulsante **Script Python**) puoi modificare e ricaricare a caldo: la sintassi viene verificata prima di sostituire il file e la versione precedente resta in `.py.bak`.

## Designer

- **Viste**: crea, duplica, elimina, imposta la pagina iniziale. Sono salvate in `config/views/*.json`.
- **Componenti**: nastro, motore, lampada, valore, testo, pulsante, serbatoio, barra, rettangolo, ellisse e linea/tubo animato.
- **Simboli SVG**: importa file `.svg`, che vengono sanificati (niente script, eventi o riferimenti esterni) e salvati in `config/symbols/`. Ogni elemento con un `id` diventa animabile: `fill`, `stroke`, `visible`, `opacity`, `text` e `rotate` sono collegabili a tag.
- **Binding** (🔗 accanto a ogni proprietà):
  - sorgente: un **tag** oppure un'**espressione** (`tag('A') && !tag('B')`);
  - trasformazione: **mappa** (`true`, `1`, `10..20`, `>5` → valore), **espressione** (`value / 10`) o **formato** (`{value} kg`);
  - anteprima live nel dialog.
- **Eventi click**: scrivi tag (valore fisso, `toggle` o `prompt`), chiama una funzione Python con payload JSON, oppure naviga a un'altra vista. Si può aggiungere una conferma.
- **Produttività**:
  - trascina un tag sulla vista per creare un indicatore, oppure su un componente per collegarne la proprietà principale;
  - griglia/snap, zoom (Ctrl+rotella), selezione multipla e rettangolare, allineamento e distribuzione;
  - undo/redo, copia/incolla, frecce per spostare;
  - **Anteprima**, in cui i click eseguono le azioni come nel runtime.

## Test

```bash
cargo test
```

I test di integrazione dei driver usano server di protocollo reali (Python). Prima installa le dipendenze:

```bash
pip install python-snap7 asyncua cpppo
```

**S7** — avvia il server in un terminale:

```bash
python tools/s7_test_server.py 1102
```

e in un altro lancia il test:

```bash
S7_TEST_PORT=1102 cargo test --test s7_integration
```

**OPC UA** — server:

```bash
python tools/opcua_test_server.py 4841
```

test:

```bash
OPCUA_TEST_PORT=4841 cargo test --features opcua --test opcua_integration
```

**EtherNet/IP** — server:

```bash
python -m cpppo.server.enip --address 127.0.0.1:44818 ReqID=DINT Run=BOOL Temp=REAL Cnt=INT
```

test:

```bash
EIP_TEST_HOST=127.0.0.1 cargo test --features eip --test eip_integration
```

## Limiti noti e prossimi passi

- **Autenticazione e ruoli**: assenti. Chiunque raggiunga la porta può scrivere tag e modificare script e viste. Prima della produzione:
  - imposta `server.designer: false`;
  - metti il servizio su una rete isolata o dietro un reverse proxy con login.

  Il prossimo passo è aggiungere utenti e ruoli (operatore / manutentore / ingegnere).
- **OPC UA**: manca la cifratura (Sign/SignAndEncrypt) e manca l'utente/password. Il polling avviene tramite Read e non con le subscription.
- **EtherNet/IP**: viene fatta una richiesta per ogni tag. Con molti tag conviene leggere una UDT come blocco (Multiple Service Packet).
- **Ridondanza**: il WCS è una singola istanza. Per l'alta disponibilità serve una coppia attivo/standby.
- **Missioni di magazzino** (traslo/shuttle, gestione celle) e interfaccia WMS/ERP: il decision engine generico è la base su cui costruirle.
