//! Host degli script Python. Le regole di business girano in un processo
//! Python separato (isolamento: un errore o un crash dello script non può
//! abbattere il WCS). Protocollo JSON-lines su stdin/stdout.

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{Mutex, oneshot};

const RUNTIME: &str = include_str!("python_runtime.py");

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScriptReply {
    pub result: Json,
    /// Scritture tag richieste dallo script tramite ctx.write().
    #[serde(default)]
    pub writes: Vec<(String, Json)>,
    #[serde(default)]
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScriptStatus {
    pub module: String,
    pub running: bool,
    pub load_error: Option<String>,
    pub functions: Vec<String>,
    /// File caricati (cartella script): nome, tipo (function|module), doc, errore.
    #[serde(default)]
    pub files: Vec<Json>,
    pub restarts: u64,
}

type Pending = Arc<std::sync::Mutex<HashMap<u64, oneshot::Sender<Result<Json>>>>>;

struct Worker {
    child: Child,
    stdin: ChildStdin,
    pending: Pending,
}

pub struct ScriptHost {
    python: String,
    module: Option<PathBuf>,
    worker: Mutex<Option<Worker>>,
    next_id: AtomicU64,
    status: std::sync::RwLock<ScriptStatus>,
}

impl ScriptHost {
    pub fn new(python: String, module: Option<PathBuf>) -> Arc<Self> {
        let status = ScriptStatus {
            module: module.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
            ..Default::default()
        };
        Arc::new(Self { python, module, worker: Mutex::new(None), next_id: AtomicU64::new(1), status: status.into() })
    }

    pub fn status(&self) -> ScriptStatus {
        self.status.read().unwrap().clone()
    }

    async fn spawn(&self) -> Result<Worker> {
        let module = self.module.as_ref().ok_or_else(|| anyhow!("nessun modulo script configurato"))?;
        let mut child = Command::new(&self.python)
            .arg("-u")
            .arg("-c")
            .arg(RUNTIME)
            .arg(module)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .with_context(|| format!("avvio interprete {}", self.python))?;
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
        let stderr = child.stderr.take().unwrap();

        // stderr degli script -> log
        tokio::spawn(async move {
            let mut l = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = l.next_line().await {
                tracing::info!(target: "script", "{line}");
            }
        });

        // handshake iniziale
        let hello = tokio::time::timeout(Duration::from_secs(10), stdout.next_line())
            .await
            .context("timeout avvio script")??
            .ok_or_else(|| anyhow!("il processo Python è terminato all'avvio"))?;
        let hello: Json = serde_json::from_str(&hello)?;
        {
            let mut st = self.status.write().unwrap();
            st.running = true;
            st.load_error = hello["error"].as_str().map(str::to_string);
            st.functions = serde_json::from_value(hello["functions"].clone()).unwrap_or_default();
            st.files = serde_json::from_value(hello["files"].clone()).unwrap_or_default();
            if let Some(e) = &st.load_error {
                tracing::error!("errore caricamento script:\n{e}");
            } else {
                tracing::info!("script caricato, funzioni: {:?}", st.functions);
            }
        }

        let pending: Pending = Default::default();
        let p2 = pending.clone();
        tokio::spawn(async move {
            while let Ok(Some(line)) = stdout.next_line().await {
                let Ok(msg) = serde_json::from_str::<Json>(&line) else { continue };
                let Some(id) = msg["id"].as_u64() else { continue };
                let Some(tx) = p2.lock().unwrap().remove(&id) else { continue };
                let _ = tx.send(Ok(msg));
            }
            // processo terminato: fallisci le chiamate pendenti
            for (_, tx) in p2.lock().unwrap().drain() {
                let _ = tx.send(Err(anyhow!("processo script terminato")));
            }
        });
        Ok(Worker { child, stdin, pending })
    }

    /// Riavvia il processo Python ricaricando il modulo (hot reload).
    pub async fn reload(&self) -> Result<ScriptStatus> {
        let mut w = self.worker.lock().await;
        if let Some(mut old) = w.take() {
            let _ = old.child.kill().await;
        }
        let nw = self.spawn().await?;
        *w = Some(nw);
        self.status.write().unwrap().restarts += 1;
        Ok(self.status())
    }

    /// Chiama `fn(payload, ctx)` nel modulo utente.
    pub async fn call(&self, func: &str, payload: Json, ctx: Json, timeout: Duration) -> Result<ScriptReply> {
        let msg = self.call_raw(func, payload, ctx, timeout, false).await?;
        if msg["ok"].as_bool() == Some(true) {
            Ok(ScriptReply {
                result: msg["result"].clone(),
                writes: serde_json::from_value(msg["writes"].clone()).unwrap_or_default(),
                logs: serde_json::from_value(msg["logs"].clone()).unwrap_or_default(),
            })
        } else {
            if let Some(t) = msg["trace"].as_str() {
                tracing::warn!(target: "script", "{t}");
            }
            Err(anyhow!("{}", msg["error"].as_str().unwrap_or("errore script")))
        }
    }

    /// Chiamata di test: ritorna la risposta completa (risultato o errore con
    /// traccia e righe, scritture non applicate, print catturate, durata).
    pub async fn test(&self, func: &str, payload: Json, ctx: Json, timeout: Duration) -> Result<Json> {
        self.call_raw(func, payload, ctx, timeout, true).await
    }

    /// Processo già avviato e caricato (per i test isolati).
    pub async fn start(&self) -> Result<ScriptStatus> {
        self.reload().await
    }

    async fn call_raw(&self, func: &str, payload: Json, ctx: Json, timeout: Duration, test: bool) -> Result<Json> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        {
            let mut w = self.worker.lock().await;
            let alive = match w.as_mut() {
                Some(x) => x.child.try_wait().ok().flatten().is_none(),
                None => false,
            };
            if !alive {
                if w.is_some() {
                    tracing::warn!("processo script terminato: riavvio");
                    self.status.write().unwrap().restarts += 1;
                }
                *w = None;
                self.status.write().unwrap().running = false;
                *w = Some(self.spawn().await?);
            }
            let wk = w.as_mut().unwrap();
            wk.pending.lock().unwrap().insert(id, tx);
            let line = serde_json::to_string(&serde_json::json!({"id": id, "fn": func, "payload": payload, "ctx": ctx, "test": test}))?;
            if let Err(e) = async {
                wk.stdin.write_all(line.as_bytes()).await?;
                wk.stdin.write_all(b"\n").await?;
                wk.stdin.flush().await
            }
            .await
            {
                wk.pending.lock().unwrap().remove(&id);
                *w = None;
                bail!("invio allo script fallito: {e}");
            }
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(r) => r.map_err(|_| anyhow!("risposta script persa"))?,
            Err(_) => {
                // Script bloccato (es. loop infinito): termina il processo così
                // le chiamate successive ripartono da un interprete sano.
                let mut w = self.worker.lock().await;
                if let Some(wk) = w.as_mut() {
                    wk.pending.lock().unwrap().remove(&id);
                }
                if let Some(mut old) = w.take() {
                    let _ = old.child.kill().await;
                }
                self.status.write().unwrap().running = false;
                bail!("timeout script '{func}' ({} ms): processo riavviato", timeout.as_millis())
            }
        }
    }
}
