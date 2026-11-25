//! Stato applicativo condiviso e avvio di tutti i sottosistemi.

use crate::alarms::{self, AlarmManager, AlarmState};
use crate::config::Config;
use crate::decision;
use crate::drivers::{self, StatusBoard};
use crate::script::ScriptHost;
use crate::store::{DecisionRecord, Store};
use crate::tagdb::{TagDb, now_ms};
use crate::value::Value;
use anyhow::Result;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
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

    pub fn audit(&self, user: &str, action: &str, detail: String) {
        let s = self.store.clone();
        let (u, a) = (user.to_string(), action.to_string());
        tokio::spawn(async move {
            let _ = s.audit(now_ms(), u, a, detail).await;
        });
    }
}

pub async fn start(cfg: Config) -> Result<Arc<AppState>> {
    // Canali di scrittura per driver
    let mut writers = HashMap::new();
    let mut receivers = HashMap::new();
    for d in &cfg.drivers {
        let (tx, rx) = mpsc::channel(256);
        writers.insert(d.name.clone(), tx);
        receivers.insert(d.name.clone(), rx);
    }
    let db = TagDb::new(&cfg.tags, writers);
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
    });

    for d in &cfg.drivers {
        let tags: Vec<_> = cfg.tags.iter().filter(|t| t.driver == d.name).cloned().collect();
        let rx = receivers.remove(&d.name).unwrap();
        tokio::spawn(drivers::run(d.clone(), tags, db.clone(), rx, board.clone()));
    }
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
    let hist: std::collections::HashSet<String> =
        app.cfg.tags.iter().filter(|t| t.historize).map(|t| t.name.clone()).collect();
    if hist.is_empty() {
        return;
    }
    let mut last: HashMap<String, i64> = HashMap::new();
    let mut rx = app.db.subscribe();
    loop {
        match rx.recv().await {
            Ok(u) if hist.contains(&u.name) => {
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
