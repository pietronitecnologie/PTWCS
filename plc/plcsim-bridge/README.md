# Ponte PTWCS ↔ PLCSIM Advanced

Server Modbus TCP che espone l'**immagine di processo** di un'istanza PLCSIM Advanced. Il layer fisico di PTWCS scrive gli ingressi `%I` e legge le uscite `%Q` direttamente. Il programma PLC resta **identico a quello di produzione**: non servono il DB `SimIO` né `FB_SimIO`.

| Modbus (driver `modbus` di PTWCS) | PLCSIM Advanced | Uso tipico |
|---|---|---|
| `coil:N` (lettura/scrittura) | `%I (N/8).(N%8)` | sensori bool: fotocellule, finecorsa |
| `di:N` (lettura) | `%Q (N/8).(N%8)` | attuatori bool: marcia, valvole |
| `hr:N` (lettura/scrittura) | `%IW(2·N)` | sensori analogici: livello, temperatura, velocità |
| `ir:N` (lettura) | `%QW(2·N)` | uscite analogiche: setpoint, aperture |
| `hr:60000` | ScaleFactor × 100 | fattore tempo (100 = tempo reale) |
| `hr:60001` | stato | 1 spento, 2 STOP, 3 avvio, 4 RUN |
| `hr:60002` | RUN/STOP | scrivi 1 = RUN, 0 = STOP |

Esempi:
- `%I3.5` → `coil:29` (3·8 + 5);
- `%Q0.0` → `di:0`;
- `%IW64` → `hr:32`;
- `%QW10` → `ir:5`.

I valori a 16 bit sono big-endian, come in S7. Un `Int` del PLC si legge e si scrive con il tag `i16` o `u16`.

## Compilazione

Su Windows x64 con PLCSIM Advanced V4 o successiva e .NET SDK:

```
dotnet build -c Release -p:PlcsimApiDir="C:\Program Files (x86)\Common Files\Siemens\PLCSIMADV\API\6.0"
```

Adatta `PlcsimApiDir` alla cartella della versione installata (contiene `Siemens.Simatic.Simulation.Runtime.Api.x64.dll`).

> Il progetto è stato scritto sull'API pubblica di PLCSIM Advanced ma **non è stato compilato nell'ambiente di sviluppo di PTWCS**, perché lì non ci sono Windows né PLCSIM. Alla prima compilazione verifica i nomi delle chiamate con la versione installata. I punti da controllare sono:
> - `SimulationRuntimeManager.CreateInterface`;
> - `IInstance.InputArea` e `OutputArea`, con `ReadBit`, `WriteBit`, `ReadByte`, `WriteByte`;
> - `ScaleFactor`, `OperatingState`, `PowerOn`, `Run`, `Stop`.

## Uso

1. Crea l'istanza in **PLCSIM Advanced Control Panel**, per esempio `PLC_1` con comunicazione *PLCSIM Virtual Ethernet Adapter*, e caricaci il programma da TIA Portal.
2. In `bridge.json` imposta il nome dell'istanza e la porta (default 5030), poi avvia il ponte:
   ```
   PlcsimBridge.exe bridge.json
   ```
   Se l'istanza è spenta il ponte la accende e la mette in RUN (`powerOnAndRun`). Se cade, si ricollega.
3. In PTWCS (`plant.yaml`) aggiungi un driver `modbus` verso il PC con il ponte, per esempio `PLCSIM_IO`, porta 5030. Crea i tag dei sensori e degli attuatori con gli indirizzi della tabella sopra, segnando `writable: true` sui sensori.
4. Nel Designer, workspace **⚛ Fisica**:
   - spunta `PLCSIM_IO` tra i **driver simulabili**;
   - imposta **Tag fattore tempo** a un tag `hr:60000` (es. `PLCSIM.Scale`, `u16`, scrivibile). Così, cambiando il fattore tempo della simulazione, anche il PLC virtuale accelera o rallenta.
5. Per lo SCADA continua a usare il driver `s7` verso l'indirizzo dell'istanza, cioè DB, merker e allarmi come con il PLC reale. Il ponte serve solo all'I/O di campo simulato.

Heartbeat e "simulazione attiva" non servono più: il PLC non deve scegliere tra ingressi reali e simulati. Gli ingressi sono quelli della CPU virtuale, che non ha periferia.
