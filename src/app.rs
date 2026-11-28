//! Stato applicativo condiviso, avvio dei sottosistemi e applicazione a caldo
//! delle modifiche di progetto fatte dal Designer.

use crate::alarms::{self, AlarmManager, AlarmState};
use crate::config::{Config, DecisionPointDef, Project, TagDef};
use crate::decision;
use crate::drivers::{self, StatusBoard};
use crate::rules::RulesEngine;
use crate::script::ScriptHost;
use crate::sorting::Sorting;
use crate::store::{DecisionRecord, Store};
use crate::tagdb::{TagDb, now_ms};
use crate::value::Value;

tokio::task_local! {
    /// Utente autenticato della richiesta in corso (per l'audit).
    pub static AUDIT_USER: String;
    /// Indirizzo del client della richiesta in corso.
    pub static AUDIT_CLIENT: String;
}

/// Categoria dell'audit trail.
pub fn audit_category(source: &str, action: &str) -> &'static str {
    match (source, action) {
        ("auth", _) | ("admin", _) => "sicurezza",
        (_, a) if a.starts_with("recipe") => "ricette",
        (_, a) if a.starts_with("seq") || a.starts_with("batch") => "sequenze",
        (_, a) if a.starts_with("sim_") => "simulazione",
        (_, a) if a.starts_with("wh_") => "wcs",
        (_, "ack") => "allarmi",
        ("designer", _) => "configurazione",
        (_, "write") | (_, "script_call") => "comandi",
        _ => "altro",
    }
}
use anyhow::{Result, bail};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

/// Eventi diffusi in tempo reale all'HMI.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    Decision(DecisionRecord),
    Alarms(Vec<AlarmState>),
    Log { ts: i64, level: String, source: String, message: String },
    /// Il progetto è cambiato (le pagine ricaricano tag/aree).
    Project,
    /// Nuova riga di audit (inviata solo a chi ha `audit.view`).
    Audit { rec: crate::store::AuditRec },
    /// Fotogramma della simulazione fisica (unità in movimento sulle viste).
    Physics(serde_json::Value),
}

struct Task {
    /// Firma della configurazione: se cambia il task viene riavviato.
    signature: String,
    handle: tokio::task::JoinHandle<()>,
}

pub struct AppState {
    /// Solo file di impianto (base su cui si applica il progetto).
    base: Config,
    /// Configurazione effettiva corrente (impianto + progetto).
    cfg: RwLock<Arc<Config>>,
    pub db: Arc<TagDb>,
    /// Processo Python dedicato ai punti di decisione (tempo reale).
    pub scripts: Arc<ScriptHost>,
    /// Processo Python separato per gli eventi HMI.
    pub hmi_scripts: Arc<ScriptHost>,
    pub security: crate::security::Security,
    pub recipes: crate::recipes::Recipes,
    pub sequencer: crate::sequences::Sequencer,
    pub physics: crate::physics::Physics,
    pub store: Store,
    pub alarms: AlarmManager,
    pub board: StatusBoard,
    pub events: broadcast::Sender<Event>,
    pub views_dir: PathBuf,
    pub symbols_dir: PathBuf,
    /// Libreria standard (sola lettura): `faceplates/`, `symbols/`, `symbols.json`, `udts.json`.
    pub library_dir: PathBuf,
    pub script_path: Option<PathBuf>,
    pub rules: RulesEngine,
    pub sorting: Arc<Sorting>,
    drivers: Mutex<HashMap<String, Task>>,
    decisions: Mutex<HashMap<String, Task>>,
    timers: Mutex<HashMap<String, Task>>,
    warehouses: RwLock<HashMap<String, (Arc<crate::warehouse::Warehouse>, Task)>>,
    /// Edizione del runtime (moduli inclusi).
    pub edition: crate::edition::Edition,
    /// Serializza le modifiche di progetto dal Designer.
    pub edit_lock: tokio::sync::Mutex<()>,
    me: std::sync::OnceLock<std::sync::Weak<AppState>>,
}

impl AppState {
    pub fn cfg(&self) -> Arc<Config> {
        self.cfg.read().unwrap().clone()
    }

    fn arc(&self) -> Arc<AppState> {
        self.me.get().and_then(|w| w.upgrade()).expect("AppState non inizializzato")
    }

    /// Ricarica il modulo Python in entrambi i processi.
    pub async fn reload_scripts(&self) -> Result<crate::script::ScriptStatus> {
        let _ = self.hmi_scripts.reload().await;
        self.scripts.reload().await
    }

    pub fn log(&self, level: &str, source: &str, message: String) {
        let _ = self.events.send(Event::Log { ts: now_ms(), level: level.into(), source: source.into(), message });
    }

    pub fn audit(&self, user: &str, action: &str, detail: String) {
        self.audit_ext(user, action, "", None, None, detail);
    }

    /// Audit con oggetto e valori prima/dopo. `source` = origine (hmi, designer, auth…):
    /// l'utente autenticato e il client della richiesta HTTP in corso vengono aggiunti.
    pub fn audit_ext(&self, source: &str, action: &str, target: &str, old: Option<String>, new: Option<String>, detail: String) {
        let s = self.store.clone();
        // utente della richiesta HTTP in corso (se c'è login): "mario (hmi)"
        let who = AUDIT_USER.try_with(|u| u.clone()).ok().filter(|u| !u.is_empty());
        let user = match who {
            Some(w) if w != source => format!("{w} ({source})"),
            _ => source.to_string(),
        };
        let client = AUDIT_CLIENT.try_with(|c| c.clone()).unwrap_or_default();
        let rec = crate::store::AuditRec {
            ts: now_ms(),
            user,
            action: action.to_string(),
            detail,
            category: audit_category(source, action).into(),
            target: target.to_string(),
            old_value: old,
            new_value: new,
            client,
        };
        let ev = self.events.clone();
        let _ = ev.send(Event::Audit { rec: rec.clone() });
        tokio::spawn(async move {
            let _ = s.audit_rec(rec).await;
        });
    }

    /// Avvia, riavvia o ferma i task driver. Solo i driver la cui configurazione,
    /// lista di tag o scan class sono cambiate vengono riavviati.
    fn sync_drivers(&self, cfg: &Config) {
        self.db.set_sim_drivers(
            cfg.drivers.iter().filter(|d| d.protocol == crate::config::Protocol::Sim).map(|d| d.name.clone()).collect(),
        );
        let mut tasks = self.drivers.lock().unwrap();
        for d in &cfg.drivers {
            let dtags: Vec<TagDef> = cfg.tags.iter().filter(|t| t.driver == d.name).cloned().collect();
            let rates = drivers::scan_rates(d, &dtags, &cfg.scan_classes);
            let signature = serde_json::to_string(&(d, &dtags, &rates)).unwrap_or_default();
            if tasks.get(&d.name).is_some_and(|t| t.signature == signature) {
                continue;
            }
            if let Some(old) = tasks.remove(&d.name) {
                old.handle.abort();
                tracing::info!(driver = %d.name, "riavvio driver per modifica di progetto");
            }
            let (tx, rx) = mpsc::channel(256);
            self.db.set_writer(&d.name, Some(tx));
            let handle =
                tokio::spawn(drivers::run(d.clone(), dtags, rates, self.db.clone(), rx, self.board.clone()));
            tasks.insert(d.name.clone(), Task { signature, handle });
        }
        let names: HashSet<&str> = cfg.drivers.iter().map(|d| d.name.as_str()).collect();
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

    /// Avvia/riavvia i task dei punti di decisione (protocollo request/reply).
    fn sync_decisions(&self, points: &[DecisionPointDef]) {
        let mut tasks = self.decisions.lock().unwrap();
        for dp in points {
            let signature = serde_json::to_string(dp).unwrap_or_default();
            if tasks.get(&dp.name).is_some_and(|t| t.signature == signature) {
                continue;
            }
            if let Some(old) = tasks.remove(&dp.name) {
                old.handle.abort();
                tracing::info!(point = %dp.name, "riavvio punto di decisione per modifica di protocollo");
            }
            let handle = tokio::spawn(decision::run(self.arc(), dp.clone()));
            tasks.insert(dp.name.clone(), Task { signature, handle });
        }
        let names: HashSet<&str> = points.iter().map(|d| d.name.as_str()).collect();
        tasks.retain(|n, t| {
            let keep = names.contains(n.as_str());
            if !keep {
                t.handle.abort();
            }
            keep
        });
    }

    /// Avvia/riavvia gli script a timer (eseguiti nel processo Python HMI,
    /// separato da quello delle decisioni in tempo reale).
    fn sync_timers(&self, timers: &[crate::config::TimerScript]) {
        let mut tasks = self.timers.lock().unwrap();
        for t in timers.iter().filter(|t| t.enabled) {
            let signature = serde_json::to_string(t).unwrap_or_default();
            if tasks.get(&t.name).is_some_and(|x| x.signature == signature) {
                continue;
            }
            if let Some(old) = tasks.remove(&t.name) {
                old.handle.abort();
            }
            let app = self.arc();
            let def = t.clone();
            let handle = tokio::spawn(async move {
                let mut iv = tokio::time::interval(Duration::from_millis(def.interval_ms));
                iv.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut errors = 0u32;
                loop {
                    iv.tick().await;
                    let ctx = serde_json::json!({"tags": decision::tags_ctx(&app), "point": format!("timer:{}", def.name)});
                    let r = app.hmi_scripts.call(&def.function, serde_json::json!({}), ctx, Duration::from_millis(def.interval_ms.max(1000))).await;
                    match r {
                        Ok(rep) => {
                            errors = 0;
                            for e in decision::apply_writes(&app, &rep.writes, &format!("timer:{}", def.name), false).await {
                                tracing::debug!("timer {}: {e}", def.name);
                            }
                            for l in rep.logs {
                                app.log("info", &format!("timer:{}", def.name), l);
                            }
                        }
                        Err(e) => {
                            errors += 1;
                            if errors == 1 || errors.is_multiple_of(100) {
                                app.log("warn", &format!("timer:{}", def.name), format!("{e:#}"));
                            }
                        }
                    }
                }
            });
            tasks.insert(t.name.clone(), Task { signature, handle });
        }
        let names: HashSet<&str> = timers.iter().filter(|t| t.enabled).map(|t| t.name.as_str()).collect();
        tasks.retain(|n, t| {
            let keep = names.contains(n.as_str());
            if !keep {
                t.handle.abort();
            }
            keep
        });
    }

    pub fn warehouse(&self, name: &str) -> Option<Arc<crate::warehouse::Warehouse>> {
        self.warehouses.read().unwrap().get(name).map(|(w, _)| w.clone())
    }

    pub fn warehouse_names(&self) -> Vec<String> {
        self.warehouses.read().unwrap().keys().cloned().collect()
    }

    /// Apre/riapre i magazzini (le locazioni sono persistenti in SQLite).
    async fn sync_warehouses(&self, defs: &[crate::config::WarehouseDef]) {
        for d in defs {
            let signature = serde_json::to_string(d).unwrap_or_default();
            if self.warehouses.read().unwrap().get(&d.name).is_some_and(|(_, t)| t.signature == signature) {
                continue;
            }
            match crate::warehouse::Warehouse::open(d.clone(), self.store.clone()).await {
                Ok(wh) => {
                    let handle = tokio::spawn(crate::warehouse::run(wh.clone(), self.db.clone()));
                    if let Some((_, old)) = self.warehouses.write().unwrap().insert(d.name.clone(), (wh, Task { signature, handle })) {
                        old.handle.abort();
                    }
                }
                Err(e) => tracing::error!("magazzino {}: {e:#}", d.name),
            }
        }
        self.warehouses.write().unwrap().retain(|n, (_, t)| {
            let keep = defs.iter().any(|d| &d.name == n);
            if !keep {
                t.handle.abort();
            }
            keep
        });
    }

    /// Valida un progetto senza applicarlo (verifica e anteprima dal Designer).
    pub fn check_project(&self, project: Project) -> Result<Config> {
        let cand = self.base.with_project(project)?;
        for d in &cand.drivers {
            let dtags: Vec<TagDef> = cand.tags.iter().filter(|t| t.driver == d.name).cloned().collect();
            if let Err(e) = drivers::create(d, &dtags) {
                bail!("{e:#}");
            }
        }
        Ok(cand)
    }

    /// Valida, salva e applica a caldo un nuovo progetto.
    pub fn apply_project(&self, project: Project) -> Result<()> {
        let cand = self.check_project(project)?;
        cand.save_project(&cand.project)?;
        self.db.set_defs(cand.tags.clone());
        self.sync_drivers(&cand);
        let wcs = self.edition.modules.wcs;
        self.sync_decisions(if wcs { &cand.decision_points } else { &[] });
        self.sync_timers(&cand.timer_scripts);
        let me = self.arc();
        let whs = if wcs { cand.warehouses.clone() } else { vec![] };
        tokio::spawn(async move { me.sync_warehouses(&whs).await });
        self.sorting.configure(&cand.sort_classes, cand.wms.clone());
        *self.cfg.write().unwrap() = Arc::new(cand);
        self.physics.invalidate();
        let _ = self.events.send(Event::Project);
        Ok(())
    }
}

pub async fn start(base: Config, cfg: Config) -> Result<Arc<AppState>> {
    let db = TagDb::new(&cfg.tags);

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
    let security = crate::security::Security::open(store.clone()).await?;
    let recipes = crate::recipes::Recipes::open(store.clone()).await?;
    let (events, _) = broadcast::channel(1024);
    let views_dir = cfg.resolve(&cfg.server.views_dir);
    let symbols_dir = cfg.resolve(&cfg.server.symbols_dir);
    std::fs::create_dir_all(&views_dir)?;
    std::fs::create_dir_all(&symbols_dir)?;
    let library_dir = std::env::var("PTWCS_LIBRARY").map(PathBuf::from).unwrap_or_else(|_| cfg.resolve(&cfg.server.library_dir));
    if !library_dir.join("symbols.json").exists() {
        tracing::warn!("libreria standard non trovata in {}", library_dir.display());
    }
    let edition = crate::edition::Edition::resolve(cfg.edition.as_deref(), crate::edition::BUILD_EDITION)?;
    tracing::info!("edizione: {} ({}){}", edition.label, edition.name, edition.locked.as_ref().map(|l| format!(", pacchetto {l}")).unwrap_or_default());
    if !edition.excluded.is_empty() {
        tracing::warn!("moduli richiesti ma non inclusi nel pacchetto: {}", edition.excluded.join(", "));
    }
    let rules = RulesEngine::load(cfg.resolve(&cfg.rules_dir), &cfg.decision_points)?;
    let sorting = Sorting::new();
    sorting.configure(&cfg.sort_classes, cfg.wms.clone());

    let app = Arc::new(AppState {
        base,
        cfg: RwLock::new(Arc::new(cfg.clone())),
        db,
        scripts,
        hmi_scripts,
        security,
        recipes,
        sequencer: Default::default(),
        physics: Default::default(),
        store,
        alarms: AlarmManager::default(),
        board: Default::default(),
        events,
        views_dir,
        symbols_dir,
        library_dir,
        script_path,
        rules,
        sorting: sorting.clone(),
        drivers: Default::default(),
        decisions: Default::default(),
        timers: Default::default(),
        warehouses: Default::default(),
        edition,
        edit_lock: Default::default(),
        me: Default::default(),
    });
    let _ = app.me.set(Arc::downgrade(&app));
    let m = app.edition.modules;
    app.sync_drivers(&cfg);
    app.sync_timers(&cfg.timer_scripts);
    if m.wcs {
        app.sync_decisions(&cfg.decision_points);
        app.sync_warehouses(&cfg.warehouses).await;
        tokio::spawn(crate::sorting::sync_task(sorting));
    }
    tokio::spawn(alarms::run(app.clone()));
    if m.scada {
        tokio::spawn(crate::sequences::run(app.clone()));
    }
    if m.sim {
        tokio::spawn(crate::physics::run(app.clone()));
    }
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
        let before = now_ms() - app.cfg().server.retention_days as i64 * 86_400_000;
        if let Err(e) = app.store.purge(before).await {
            tracing::warn!("purge storico: {e:#}");
        }
    }
}
