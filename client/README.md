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
| Download | le esportazioni (es. CSV dell'audit) finiscono nella cartella Download |
| Scorciatoie | `F5` aggiorna · `F11` schermo intero · `Ctrl`+`+`/`−`/`0` zoom · `Ctrl`+`L` launcher · `Ctrl`+`N` nuova finestra |

Il motore di visualizzazione è quello del sistema operativo:
- WebView2 su Windows 10/11, già installato;
- WebKitGTK su Linux;
- WebKit su macOS.

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
