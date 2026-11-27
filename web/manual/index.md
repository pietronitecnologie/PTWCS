# Manuale utente PTWCS

PTWCS è un **WCS** (Warehouse Control System) che è anche uno **SCADA**:
- dialoga con i PLC (Siemens S7, Modbus TCP, OPC UA, EtherNet/IP);
- mostra impianto, allarmi e trend;
- decide in tempo reale sulle richieste del PLC (smistamento colli, stoccaggio pallet);
- gestisce i magazzini automatici a trasloelevatori.

Si usa da browser e ha due applicazioni:

| Applicazione | Per chi | Indirizzo |
|---|---|---|
| **Runtime** | operatori e capiturno | `http://<server>:8080/` |
| **Designer** | progettisti e manutentori | `http://<server>:8080/designer.html` |

## Runtime

- [Uso del runtime](runtime.md): viste, barra delle aree, navigazione, faceplate, allarmi, decisioni, tag, trend, diagnostica, processo (batch, fasi, ricette, dispositivi), simulazione fisica, audit trail, tema chiaro
- [WCS magazzini automatici](wcs.md): missioni, ordini, giacenze, riorganizzazione, KPI dei trasli, strategie

## Designer

- [Panoramica e viste grafiche](designer.md): editor delle viste, componenti, simboli, binding, eventi, faceplate, libreria standard
- [Tag e UDT](designer-tag.md): tag, tipi e istanze UDT, allarmi, trend
- [Smistamento](designer-smistamento.md): protocollo request/reply, regole, classi, WMS
- [Progetto](designer-progetto.md): aree, scan class, script a timer, magazzini automatici, sicurezza, driver
- [Processo](designer-processo.md): dispositivi, sequenze SFC (fasi) e ricette che le schedulano (ISA-88)
- [Fisica](designer-fisica.md): layer fisico per il collaudo virtuale (nastri, sensori, deviatori, materiali, PLCSIM Advanced)
- [Script Python](designer-script.md): un file per funzione, editor, suggerimenti, snippet, test

## Aiuto contestuale

Nel Designer i pulsanti **?** accanto a sezioni e campi aprono la parte di questo manuale che li descrive, in un pannello laterale. Il manuale completo si apre con **📖 Manuale** in alto, in entrambe le applicazioni.

## Glossario

| Termine | Significato |
|---|---|
| **Tag** | variabile del PLC (o interna) letta/scritta dal WCS |
| **UDT** | tipo di dato composto (es. *Nastro*: Run, Fault, Speed…) con indirizzi parametrici |
| **Istanza UDT** | un dispositivo concreto (es. `CV1` di tipo *Nastro*): genera i tag `CV1.Run`, `CV1.Fault`… |
| **Area** | parte dell'impianto: raccoglie allarmi, errori di qualità e viste |
| **Punto di decisione** | richiesta del PLC a cui il WCS risponde (es. scanner, ingresso magazzino) |
| **Faceplate** | finestra di dettaglio di un dispositivo, aperta cliccando il simbolo |
| **UdC** | unità di carico (pallet, collo, cassa) |
| **P&D** | stazione di presa e deposito in testa alla corsia del magazzino |
| **Traslo** | trasloelevatore |
