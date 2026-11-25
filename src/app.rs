//! Stato applicativo condiviso e avvio di tutti i sottosistemi.

use crate::alarms::{self, AlarmManager, AlarmState};
use crate::config::{Config, DriverConfig, TagDef};
use crate::decision;
use crate::drivers::{self, StatusBoard};
use crate::rules::RulesEngine;
use crate::script::ScriptHost;
use crate::store::{DecisionRecord, Store};
use crate::tagdb::{TagDb, now_ms};
use crate::value::Value;
use anyhow::{Result, bail};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// Eventi diffusi in tempo reale all'HMI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    Decision(DecisionRecord),
    Alarms(Vec<AlarmState>),
    Log { ts: i64, level: String, source: String, message: String },
}

pub struct AppState {
    pub cfg: Config,
    pub db: Arc<TagDb>,
    /// Processo Python dedicato ai punti di decisione (tempo reale).
    pub scripts: Arc<ScriptHost>,
    /// Processo Python separato per gli eventi HMI: uno script lento lanciato
    /// da un operatore non può ritardare le risposte ai PLC.
    pub hmi_scripts: Arc<ScriptHost>,
    pub store: Store,
    pub alarms: AlarmManager,
    pub board: StatusBoard,
    pub events: broadcast::Sender<Event>,
    pub views_dir: PathBuf,
    pub symbols_dir: PathBuf,
    pub script_path: Option<PathBuf>,
    /// Regole di smistamento grafiche.
    pub rules: RulesEngine,
    /// Task dei driver attivi (riavviati a caldo quando cambiano i loro tag).
    drivers: std::sync::Mutex<HashMap<String, DriverTask>>,
    /// Tag modificabili dal Designer (salvati in `tags_file`).
    pub editable_tags: RwLock<HashSet<String>>,
    /// Serializza le modifiche di configurazione dal Designer.
    pub edit_lock: tokio::sync::Mutex<()>,
}

struct DriverTask {
    /// Firma (config + tag) per capire se va riavviato.
    signature: String,
    handle: tokio::task::JoinHandle<()>,
}

impl AppState {
    /// Ricarica il modulo Python in entrambi i processi.
    pub async fn reload_scripts(&self) -> Result<crate::script::ScriptStatus> {
        let _ = self.hmi_scripts.reload().await;
        self.scripts.reload().await
    }

    pub fn log(&self, level: &str, source: &str, message: String) {
        let _ = self.events.send(Event::Log { ts: now_ms(), level: level.into(), source: source.into(), message });
    }

    /// Avvia, riavvia o ferma i task driver in base a configurazione e tag.
    /// Solo i driver la cui configurazione o lista di tag è cambiata vengono
    /// riavviati: gli altri continuano a comunicare senza interruzioni.
    pub fn sync_drivers(&self, drivers: &[DriverConfig], tags: &[TagDef]) {
        let mut tasks = self.drivers.lock().unwrap();
        for d in drivers {
            let dtags: Vec<TagDef> = tags.iter().filter(|t| t.driver == d.name).cloned().collect();
            let signature = serde_json::to_string(&(d, &dtags)).unwrap_or_default();
            if tasks.get(&d.name).is_some_and(|t| t.signature == signature) {
                continue;
            }
            if let Some(old) = tasks.remove(&d.name) {
                old.handle.abort();
                tracing::info!(driver = %d.name, "riavvio driver per modifica tag");
            }
            let (tx, rx) = mpsc::channel(256);
            self.db.set_writer(&d.name, Some(tx));
            let handle = tokio::spawn(drivers::run(d.clone(), dtags, self.db.clone(), rx, self.board.clone()));
            tasks.insert(d.name.clone(), DriverTask { signature, handle });
        }
        let names: HashSet<&str> = drivers.iter().map(|d| d.name.as_str()).collect();
        tasks.retain(|n, t| {
            let keep = names.contains(n.as_str());
            if !keep {
                t.handle.abort();
                self.db.set_writer(n, None);
                self.board.write().unwrap().remove(n);
            }
            keep
        });
    }

    /// Applica una nuova lista completa di tag (dal Designer): valida, prova a
    /// creare i driver coinvolti (verifica indirizzi), salva su file e applica a caldo.
    pub fn apply_tags(&self, tags: Vec<TagDef>, editable: HashSet<String>) -> Result<()> {
        let mut cand = self.cfg.clone();
        cand.tags = tags.clone();
        cand.validate()?;
        for d in &cand.drivers {
            let dtags: Vec<TagDef> = tags.iter().filter(|t| t.driver == d.name).cloned().collect();
            if let Err(e) = drivers::create(d, &dtags) {
                bail!("{e:#}");
            }
        }
        for t in &tags {
            if t.name.trim().is_empty() || t.name.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"') {
                bail!("nome tag non valido: '{}' (niente spazi o apici)", t.name);
            }
        }
        cand.editable_tags = editable.clone();
        cand.save_editable_tags(&tags)?;
        *self.editable_tags.write().unwrap() = editable;
        self.db.set_defs(tags.clone());
        self.sync_drivers(&cand.drivers, &tags);
        Ok(())
    }

    pub fn audit(&self, user: &str, action: &str, detail: String) {
        let s = self.store.clone();
        let (u, a) = (user.to_string(), action.to_string());
        tokio::spawn(async move {
            let _ = s.audit(now_ms(), u, a, detail).await;
        });
    }
}

pub async fn start(cfg: Config) -> Result<Arc<AppState>> {
    let db = TagDb::new(&cfg.tags);
    let board: StatusBoard = Default::default();

    let script_path = cfg.scripting.module.as_ref().map(|m| cfg.resolve(m));
    let scripts = ScriptHost::new(cfg.scripting.python.clone(), script_path.clone());
    let hmi_scripts = ScriptHost::new(cfg.scripting.python.clone(), script_path.clone());
    if script_path.is_some() {
        for h in [&scripts, &hmi_scripts] {
            if let Err(e) = h.reload().await {
                tracing::error!("avvio script: {e:#}");
            }
        }
    }

    let store = Store::open(&cfg.resolve(&cfg.server.database))?;
    let (events, _) = broadcast::channel(1024);
    let views_dir = cfg.resolve(&cfg.server.views_dir);
    let symbols_dir = cfg.resolve(&cfg.server.symbols_dir);
    std::fs::create_dir_all(&views_dir)?;
    std::fs::create_dir_all(&symbols_dir)?;
    let rules = RulesEngine::load(cfg.resolve(&cfg.rules_dir), &cfg.decision_points)?;

    let app = Arc::new(AppState {
        cfg: cfg.clone(),
        db: db.clone(),
        scripts,
        hmi_scripts,
        store,
        alarms: AlarmManager::default(),
        board: board.clone(),
        events,
        views_dir,
        symbols_dir,
        script_path,
        rules,
        drivers: Default::default(),
        editable_tags: RwLock::new(cfg.editable_tags.clone()),
        edit_lock: Default::default(),
    });
    app.sync_drivers(&cfg.drivers, &cfg.tags);
    for dp in &cfg.decision_points {
        tokio::spawn(decision::run(app.clone(), dp.clone()));
    }
    tokio::spawn(alarms::run(app.clone()));
    tokio::spawn(historian(app.clone()));
    tokio::spawn(housekeeping(app.clone()));
    Ok(app)
}

/// Registra nello storico i cambi dei tag con `historize: true` (max 4/s per tag).
async fn historian(app: Arc<AppState>) {
    let mut last: HashMap<String, i64> = HashMap::new();
    let mut rx = app.db.subscribe();
    loop {
        match rx.recv().await {
            Ok(u) if app.db.def(&u.name).is_some_and(|d| d.historize) => {
                let ts = u.state.ts;
                if last.get(&u.name).is_some_and(|&l| ts - l < 250) {
                    continue;
                }
                last.insert(u.name.clone(), ts);
                let (num, txt) = match &u.state.value {
                    Value::Str(s) => (None, Some(s.clone())),
                    v => (v.as_f64(), None),
                };
                let _ = app.store.log_history(ts, u.name, num, txt).await;
            }
            Ok(_) | Err(broadcast::error::RecvError::Lagged(_)) => {}
            Err(_) => return,
        }
    }
}

async fn housekeeping(app: Arc<AppState>) {
    let mut t = tokio::time::interval(Duration::from_secs(3600));
    loop {
        t.tick().await;
        let before = now_ms() - app.cfg.server.retention_days as i64 * 86_400_000;
        if let Err(e) = app.store.purge(before).await {
            tracing::warn!("purge storico: {e:#}");
        }
    }
}
