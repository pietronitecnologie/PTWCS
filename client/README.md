# PTWCS Client

Applicazione desktop per usare **runtime** e **Designer** di PTWCS in finestre proprie, senza browser: una sola applicazione per tutte le postazioni (operatore, manutenzione, progettazione).

| Funzione | Dettaglio |
|---|---|
| Launcher | server salvati con stato (online, nome impianto), aggiunta e prova di un server, apertura di runtime e Designer |
| Finestre | più finestre insieme; i link "nuova finestra" (Designer ↗, manuale) si aprono nel client |
| Chiosco | runtime a schermo intero senza bordi per le postazioni operatore; si esce con `Ctrl`+`Maiusc`+`Q` |
| Avvio diretto | opzione "all'avvio apri il runtime" (dell'ultimo server) oppure da riga di comando |
| Riconnessione | se il server non risponde compare una pagina di attesa che riprova ogni 3 s e ricollega da sola |
| Sessione | il login vale per tutte le finestre e resta valido tra un avvio e l'altro |
| Progetti | nuovo progetto, apri e salva su file `.ptwcs`, backup e ripristino sul server, progetti recenti |
| Download | le esportazioni (es. CSV dell'audit) finiscono nella cartella Download |
| Scorciatoie | `F5` aggiorna · `F11` schermo intero · `Ctrl`+`+`/`−`/`0` zoom · `Ctrl`+`L` launcher · `Ctrl`+`N` nuova finestra |

Il motore di visualizzazione è quello del sistema operativo:
- WebView2 su Windows 10/11, già installato;
- WebKitGTK su Linux;
- WebKit su macOS.

## Progetti

Il pulsante **📁 Progetto** di ogni server apre la gestione del progetto. Se il server ha la sicurezza attiva chiede prima un utente con il permesso `designer.project`.

| Azione | Cosa fa |
|---|---|
| **🆕 Nuovo progetto…** | sostituisce il progetto del server con uno vuoto (una pagina iniziale). Ruoli e utenti restano |
| **📂 Apri da file…** | sceglie un file `.ptwcs` con il dialogo del sistema, ne mostra il riepilogo (tag, viste, regole, script, ricette, autore, data) e, dopo la conferma, lo carica sul server |
| **💾 Salva su file…** | scarica l'intero progetto in un file `.ptwcs` |
| **🗄 Crea backup** | salva una copia del progetto sul server |
| **↺ Ripristina** | ricarica uno dei backup del server |

Prima di ogni sostituzione (nuovo, apri, ripristina) il server salva un **backup automatico** del progetto attuale, quindi si può sempre tornare indietro. Il server conserva gli ultimi 30 backup.

Dopo la sostituzione le finestre aperte su quel server si ricaricano da sole. I file aperti o salvati di recente compaiono nell'elenco **📂 Progetti recenti** del launcher, con la scelta del server su cui aprirli.

Il file `.ptwcs` è JSON e contiene:
- il progetto: tag, UDT, aree, punti di decisione, magazzini, fasi, fisica, ruoli;
- viste, regole di smistamento, script Python, simboli e ricette.

Non contiene utenti, password e file di impianto (`plant.yaml`: driver, porte, percorsi), che restano propri di ogni server. Per questo lo stesso progetto si può portare da un server di sviluppo a quello di produzione.

All'apertura ruoli e sicurezza del server restano quelli attuali. Si importano anche quelli del file solo spuntando l'opzione, e solo con il permesso `users.manage`.

## Riga di comando

```
ptwcs-client                                  launcher (o avvio diretto, se impostato)
ptwcs-client --server http://10.0.0.5:8080 --runtime [--kiosk]
ptwcs-client --server http://10.0.0.5:8080 --designer
ptwcs-client --launcher                       forza il launcher
```

Per una postazione operatore che parte già nel runtime, metti nell'esecuzione automatica del sistema operativo:

```
ptwcs-client --server http://<server>:8080 --runtime --kiosk
```

## Compilazione

Il client è un crate a parte del workspace. `cargo build` alla radice compila solo il server, quindi il server non richiede le librerie grafiche.

**Linux** (Debian/Ubuntu):

```
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev
cargo build --release -p ptwcs-client
./target/release/ptwcs-client
```

Per averlo nel menu delle applicazioni:

```
sudo install target/release/ptwcs-client /usr/local/bin/
install -Dm644 client/ptwcs-client.desktop ~/.local/share/applications/ptwcs-client.desktop
```

**Windows**: con Rust installato basta compilare. In release il programma parte senza finestra di console.

```
cargo build --release -p ptwcs-client
```

Il risultato è `target\release\ptwcs-client.exe`, da copiare sulle postazioni. Su Windows 10/11 il WebView2 Runtime è già presente.

**macOS**: `cargo build --release -p ptwcs-client`.

## Dove salva i dati

| Contenuto | Linux | Windows |
|---|---|---|
| Configurazione (server, opzioni) | `~/.config/ptwcs-client/config.json` | `%APPDATA%\ptwcs-client\config.json` |
| Sessione e dati delle pagine | `~/.local/share/ptwcs-client/webdata` | `%LOCALAPPDATA%\ptwcs-client\webdata` |

Cancellando la cartella `webdata` si esce da tutte le sessioni.
