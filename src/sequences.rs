//! Motore SFC per **fasi** (sequenze del progetto) e **batch** (ricette in esecuzione),
//! con modello di stato ISA-88 e allocazione esclusiva dei dispositivi.
//!
//! - Una **fase** è un'istanza di una sequenza su un dispositivo (`{unit}`) con i
//!   suoi parametri (`{nome}`). Chiave: `sequenza@dispositivo`.
//! - Un **batch** è un'istanza di una ricetta: alloca i dispositivi richiesti e le
//!   sue azioni `phase` avviano le fasi passando i parametri; i passi con fasi
//!   attendono la loro conclusione (`@phases_done`). Chiave: id del batch (B00001).
//! - **Rami paralleli** (divergenza AND) in entrambe: tutti i rami partono insieme
//!   e il passo prosegue quando tutti sono arrivati in fondo. Più transizioni su un
//!   passo = divergenza alternativa (vince la prima vera).
//! - **Stati**: idle → running → (held ⇄ running) → complete | stopped | aborted;
//!   `error` per timeout o azioni fallite (allarme `SEQ.<chiave>`; restart riprova il
//!   passo fallito). Hold/restart/stop/abort eseguono le azioni di stato della
//!   procedura e si propagano alle fasi del batch.
//! - Un dispositivo allocato a un batch non può essere usato da altri batch né da
//!   fasi manuali finché il batch non termina.

use crate::alarms::NO_AREA;
use crate::app::AppState;
use crate::config::{SeqAction, SeqCond, SequenceDef, Severity, StepDef};
use crate::recipes::{RecipeDef, resolve_params};
use crate::tagdb::now_ms;
use crate::value::Value;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunState {
    Idle,
    Running,
    Held,
    Complete,
    Stopped,
    Aborted,
    Error,
}

impl RunState {
    pub fn finished(self) -> bool {
        matches!(self, RunState::Complete | RunState::Stopped | RunState::Aborted)
    }
    pub fn active(self) -> bool {
        matches!(self, RunState::Running | RunState::Held | RunState::Error)
    }
}

/// Posizione in una lista di passi (con i cursori dei rami paralleli).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cursor {
    pub idx: usize,
    pub step: String,
    pub since: i64,
    pub done: bool,
    /// false = il passo `idx` deve ancora essere eseguito (avvio o nuovo tentativo)
    pub entered: bool,
    pub failed: bool,
    pub branches: Vec<Cursor>,
    /// fasi avviate da questo passo (chiavi)
    pub phases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRec {
    /// percorso del passo: "3", "3.2/1" (passo 1 del ramo 2 del passo 3)
    pub path: String,
    pub step: String,
    pub enter: i64,
    pub exit: Option<i64>,
    pub reason: String,
}

/// Definizione eseguita (copia presa all'avvio).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Procedure {
    pub steps: Vec<StepDef>,
    pub hold: Vec<SeqAction>,
    pub restart: Vec<SeqAction>,
    pub stop: Vec<SeqAction>,
    pub abort: Vec<SeqAction>,
    pub area: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub key: String,
    /// "phase" | "batch"
    pub kind: String,
    /// sequenza o ricetta
    pub name: String,
    /// batch proprietario (fasi avviate da una ricetta)
    pub owner: Option<String>,
    pub device: String,
    /// batch: alias → dispositivo
    pub devices: BTreeMap<String, String>,
    pub params: BTreeMap<String, String>,
    pub state: RunState,
    pub message: String,
    pub started: i64,
    pub finished: Option<i64>,
    pub user: String,
    pub confirm: bool,
    pub held_since: Option<i64>,
    /// batch: lotto / descrizione
    pub label: String,
    /// batch: versione della ricetta
    pub version: u32,
    pub cursor: Cursor,
    pub history: Vec<StepRec>,
    pub proc: Arc<Procedure>,
}

#[derive(Default)]
struct Inner {
    inst: BTreeMap<String, Instance>,
    /// dispositivo → batch che lo ha allocato
    alloc: BTreeMap<String, String>,
    edges: HashMap<String, bool>,
    next_batch: u32,
    loaded: bool,
    /// ultimo stato salvato di ogni istanza (persistenza per il riavvio)
    saved: HashMap<String, String>,
    last_save: i64,
}

#[derive(Default)]
pub struct Sequencer {
    inner: tokio::sync::Mutex<Inner>,
}

pub fn phase_key(seq: &str, device: &str) -> String {
    if device.is_empty() { seq.to_string() } else { format!("{seq}@{device}") }
}

/// Sostituisce `{unit}`, `{alias}` dei dispositivi e `{parametro}`.
fn subst(s: &str, i: &Instance) -> String {
    if !s.contains('{') {
        return s.to_string();
    }
    let mut out = s.replace("{unit}", &i.device);
    for (k, v) in i.devices.iter().chain(i.params.iter()) {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

fn subst_value(v: &Value, i: &Instance) -> Value {
    match v {
        Value::Str(s) if s.contains('{') => {
            let t = subst(s, i);
            match t.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                _ => t.parse::<i64>().map(Value::Int).or_else(|_| t.parse::<f64>().map(Value::Float)).unwrap_or(Value::Str(t)),
            }
        }
        x => x.clone(),
    }
}

fn shift_times(c: &mut Cursor, dt: i64) {
    c.since += dt;
    for b in &mut c.branches {
        shift_times(b, dt);
    }
}

/// Passi attivi (foglie) con il loro percorso.
pub fn active_steps(c: &Cursor, steps: &[StepDef], path: &str, out: &mut Vec<(String, String, i64)>) {
    if c.done || !c.entered {
        return;
    }
    let p = if path.is_empty() { (c.idx + 1).to_string() } else { format!("{path}/{}", c.idx + 1) };
    if let Some(st) = steps.get(c.idx) {
        if st.branches.is_empty() || c.branches.is_empty() {
            out.push((p.clone(), st.name.clone(), c.since));
        }
        for (k, (b, bc)) in st.branches.iter().zip(&c.branches).enumerate() {
            active_steps(bc, b, &format!("{p}.{}", k + 1), out);
        }
    }
}

impl Sequencer {
    async fn lock(&self, app: &AppState) -> tokio::sync::MutexGuard<'_, Inner> {
        let mut g = self.inner.lock().await;
        if !g.loaded {
            g.loaded = true;
            let max = app.store.kv_load("batches").await.unwrap_or_default().iter().filter_map(|(k, _)| k.trim_start_matches('B').parse::<u32>().ok()).max().unwrap_or(0);
            g.next_batch = max + 1;
            // istanze attive prima del riavvio: ripristinate in hold, con le allocazioni
            for (k, d) in app.store.kv_load("active_inst").await.unwrap_or_default() {
                let Ok(mut i) = serde_json::from_str::<Instance>(&d) else { continue };
                if i.state == RunState::Running {
                    i.state = RunState::Held;
                    i.held_since = Some(now_ms());
                    i.message = "ripristinato dopo il riavvio del server: verificare l'impianto e dare restart".into();
                }
                if i.kind == "batch" && i.state.active() {
                    for d in i.devices.values() {
                        g.alloc.insert(d.clone(), i.key.clone());
                    }
                }
                g.saved.insert(k.clone(), d);
                g.inst.insert(k, i);
            }
            if !g.inst.is_empty() {
                app.log("warn", "processo", format!("{} istanze ripristinate in hold dopo il riavvio", g.inst.len()));
            }
        }
        g
    }

    /// Salva le istanze cambiate (e cancella quelle rimosse) per il ripristino dopo un riavvio.
    fn persist(app: &AppState, g: &mut Inner) {
        let now = now_ms();
        if now - g.last_save < 1000 {
            return;
        }
        g.last_save = now;
        let mut cur = HashMap::new();
        for (k, i) in &g.inst {
            if let Ok(d) = serde_json::to_string(i) {
                cur.insert(k.clone(), d);
            }
        }
        let mut ops = vec![];
        for (k, d) in &cur {
            if g.saved.get(k) != Some(d) {
                ops.push((k.clone(), Some(d.clone())));
            }
        }
        for k in g.saved.keys() {
            if !cur.contains_key(k) {
                ops.push((k.clone(), None));
            }
        }
        g.saved = cur;
        if ops.is_empty() {
            return;
        }
        let store = app.store.clone();
        tokio::spawn(async move {
            for (k, d) in ops {
                let _ = store.kv_put("active_inst", k, d).await;
            }
        });
    }

    /// Istanze attive e allocazioni dei dispositivi.
    pub async fn snapshot(&self, app: &AppState) -> (Vec<Instance>, BTreeMap<String, String>) {
        let g = self.lock(app).await;
        (g.inst.values().cloned().collect(), g.alloc.clone())
    }

    /// Stato di un'istanza senza attendere il motore (per test e diagnostica).
    pub fn clone_state(&self, key: &str) -> Option<Instance> {
        self.inner.try_lock().ok().and_then(|g| g.inst.get(key).cloned()).or_else(|| {
            // il motore sta lavorando: riprova a breve
            std::thread::sleep(Duration::from_millis(5));
            self.inner.try_lock().ok().and_then(|g| g.inst.get(key).cloned())
        })
    }

    pub async fn get(&self, app: &AppState, key: &str) -> Option<Instance> {
        self.lock(app).await.inst.get(key).cloned()
    }

    fn seq_def(app: &AppState, name: &str) -> Result<SequenceDef> {
        app.cfg().project.sequences.iter().find(|d| d.name == name).cloned().ok_or_else(|| anyhow!("sequenza {name} non trovata"))
    }

    fn check_device(app: &AppState, def: &SequenceDef, device: &str) -> Result<()> {
        if def.device_class.is_empty() {
            return Ok(());
        }
        let cfg = app.cfg();
        let d = cfg.project.devices.iter().find(|d| d.name == device).ok_or_else(|| anyhow!("dispositivo '{device}' inesistente"))?;
        if d.class != def.device_class {
            bail!("la sequenza {} lavora su dispositivi di classe {}, {device} è {}", def.name, def.device_class, d.class);
        }
        Ok(())
    }

    fn new_phase(def: &SequenceDef, device: &str, params: BTreeMap<String, String>, owner: Option<String>, user: &str) -> Instance {
        Instance {
            key: phase_key(&def.name, device),
            kind: "phase".into(),
            name: def.name.clone(),
            owner,
            device: device.into(),
            devices: BTreeMap::new(),
            params,
            state: RunState::Running,
            message: String::new(),
            started: now_ms(),
            finished: None,
            user: user.into(),
            confirm: false,
            held_since: None,
            label: String::new(),
            version: 0,
            cursor: Cursor::default(),
            history: vec![],
            proc: Arc::new(Procedure {
                steps: def.steps.clone(),
                hold: def.hold_actions.clone(),
                restart: def.restart_actions.clone(),
                stop: def.stop_actions.clone(),
                abort: def.abort_actions.clone(),
                area: def.area.clone(),
            }),
        }
    }

    /// Avvio manuale (o da PLC) di una fase.
    pub async fn start_phase(&self, app: &AppState, seq: &str, device: &str, params: BTreeMap<String, String>, user: &str) -> Result<Instance> {
        let def = Self::seq_def(app, seq)?;
        let device = if device.is_empty() { def.unit.clone() } else { device.to_string() };
        if !def.device_class.is_empty() && device.is_empty() {
            bail!("indica il dispositivo (classe {})", def.device_class);
        }
        Self::check_device(app, &def, &device)?;
        let params = resolve_params(&format!("sequenza {seq}"), &def.parameters, &params)?;
        let mut g = self.lock(app).await;
        if let Some(b) = g.alloc.get(&device) {
            let r = g.inst.get(b).map(|x| x.name.clone()).unwrap_or_default();
            bail!("il dispositivo {device} è allocato al batch {b} (ricetta {r})");
        }
        let key = phase_key(seq, &device);
        if g.inst.get(&key).is_some_and(|i| i.state.active()) {
            bail!("{key} è già in esecuzione");
        }
        let i = Self::new_phase(&def, &device, params, None, user);
        Self::alarm(app, &i, false, "");
        g.inst.insert(key, i.clone());
        app.log("info", &format!("seq:{}", i.key), format!("avvio da {user}"));
        Ok(i)
    }

    /// Avvio di un batch: verifica e alloca i dispositivi, risolve i parametri.
    pub async fn start_batch(&self, app: &AppState, r: &RecipeDef, devices: BTreeMap<String, String>, params: BTreeMap<String, String>, label: &str, user: &str) -> Result<Instance> {
        r.validate(&app.cfg())?;
        let params = resolve_params(&format!("ricetta {}", r.name), &r.parameters, &params)?;
        let cfg = app.cfg();
        let mut g = self.lock(app).await;
        let mut chosen = BTreeMap::new();
        for d in &r.devices {
            let dev = devices.get(&d.alias).cloned().filter(|x| !x.is_empty()).unwrap_or_else(|| d.default.clone());
            let def = cfg.project.devices.iter().find(|x| x.name == dev).ok_or_else(|| anyhow!("dispositivo per '{}' non indicato o inesistente", d.alias))?;
            if def.class != d.class {
                bail!("{}: {dev} è di classe {}, serve {}", d.alias, def.class, d.class);
            }
            if chosen.values().any(|x| x == &dev) {
                bail!("il dispositivo {dev} è indicato due volte");
            }
            if let Some(b) = g.alloc.get(&dev) {
                bail!("il dispositivo {dev} è già allocato al batch {b}");
            }
            if let Some(p) = g.inst.values().find(|i| i.kind == "phase" && i.device == dev && i.state.active()) {
                bail!("sul dispositivo {dev} è in esecuzione la fase {} (manuale)", p.name);
            }
            chosen.insert(d.alias.clone(), dev);
        }
        let id = format!("B{:05}", g.next_batch);
        g.next_batch += 1;
        for dev in chosen.values() {
            g.alloc.insert(dev.clone(), id.clone());
        }
        let area = chosen.values().next().and_then(|d| cfg.project.devices.iter().find(|x| &x.name == d)).and_then(|d| d.area.clone());
        let i = Instance {
            key: id.clone(),
            kind: "batch".into(),
            name: r.name.clone(),
            owner: None,
            device: String::new(),
            devices: chosen,
            params,
            state: RunState::Running,
            message: String::new(),
            started: now_ms(),
            finished: None,
            user: user.into(),
            confirm: false,
            held_since: None,
            label: label.into(),
            version: r.version,
            cursor: Cursor::default(),
            history: vec![],
            proc: Arc::new(Procedure {
                steps: r.steps.clone(),
                hold: r.hold_actions.clone(),
                restart: r.restart_actions.clone(),
                stop: r.stop_actions.clone(),
                abort: r.abort_actions.clone(),
                area,
            }),
        };
        g.inst.insert(id.clone(), i.clone());
        app.log("info", &format!("batch:{id}"), format!("avvio ricetta {} v{} da {user}, dispositivi {:?}", r.name, r.version, i.devices));
        Ok(i)
    }

    fn alarm(app: &AppState, i: &Instance, active: bool, msg: &str) {
        let area = i.proc.area.clone().unwrap_or_else(|| NO_AREA.into());
        app.alarms.raise(app, &format!("SEQ.{}", i.key), &area, msg, Severity::Alarm, active);
    }

    // ------------------------------------------------------------ comandi

    /// Comandi ISA-88: hold, restart, stop, abort, reset, confirm, next, goto (`step`).
    pub async fn command(&self, app: &AppState, key: &str, cmd: &str, step: Option<&str>, user: &str) -> Result<Instance> {
        let mut g = self.lock(app).await;
        let st = g.inst.get(key).map(|i| i.state).ok_or_else(|| anyhow!("{key} non è in esecuzione"))?;
        // le fasi di un batch si comandano dal batch (salvo restart dopo un errore)
        if let Some(o) = g.inst.get(key).and_then(|i| i.owner.clone())
            && g.inst.get(&o).is_some_and(|b| b.state.active())
            && !matches!(cmd, "confirm" | "hold")
            && !(cmd == "restart" && st == RunState::Error)
        {
            bail!("{key} appartiene al batch {o}: usa i comandi del batch");
        }
        let cmd = match cmd {
            "pause" => "hold",
            "resume" => "restart",
            c => c,
        };
        match (cmd, st) {
            ("hold", RunState::Running) => self.hold(app, &mut g, key, &format!("hold da {user}")).await,
            ("restart", RunState::Held) | ("restart", RunState::Error) => self.restart(app, &mut g, key, user).await,
            ("stop", s) if s.active() => self.end(app, &mut g, key, RunState::Stopped, &format!("stop da {user}")).await,
            ("abort", s) if s.active() => self.end(app, &mut g, key, RunState::Aborted, &format!("abort da {user}")).await,
            ("reset", s) if s.finished() || s == RunState::Idle => {
                let i = g.inst.remove(key).unwrap();
                Self::alarm(app, &i, false, "");
                return Ok(Instance { state: RunState::Idle, ..i });
            }
            ("confirm", s) if s.active() => {
                g.inst.get_mut(key).unwrap().confirm = true;
            }
            ("next", RunState::Running) | ("next", RunState::Held) | ("goto", RunState::Running) | ("goto", RunState::Held) => {
                let mut i = g.inst.remove(key).unwrap();
                let res = self.jump(app, &mut g, &mut i, cmd, step, user).await;
                g.inst.insert(key.to_string(), i);
                res?;
            }
            (c, s) => bail!("comando '{c}' non ammesso nello stato {s:?}"),
        }
        g.inst.get(key).cloned().ok_or_else(|| anyhow!("{key} terminato"))
    }

    /// Avanzamento manuale (solo passi del livello principale).
    async fn jump(&self, app: &AppState, g: &mut Inner, i: &mut Instance, cmd: &str, step: Option<&str>, user: &str) -> Result<()> {
        let proc = i.proc.clone();
        let cur = i.cursor.idx;
        let target = if cmd == "goto" {
            match step {
                Some("END") => None,
                Some(n) => Some(proc.steps.iter().position(|s| s.name == n).ok_or_else(|| anyhow!("passo {n} non trovato (solo passi principali)"))?),
                None => bail!("passo mancante"),
            }
        } else {
            let next = proc.steps.get(cur).and_then(|s| s.transitions.first()).map(|t| t.next.as_str()).unwrap_or("");
            match next {
                "END" => None,
                "" if cur + 1 >= proc.steps.len() => None,
                "" => Some(cur + 1),
                n => proc.steps.iter().position(|s| s.name == n),
            }
        };
        close_rec(i, &(cur + 1).to_string(), &format!("manuale ({user})"));
        match target {
            None => {
                i.cursor.done = true;
                Ok(())
            }
            Some(t) => {
                let mut c = std::mem::take(&mut i.cursor);
                let r = self.enter(app, g, i, &proc.steps, &mut c, t, "").await;
                i.cursor = c;
                r
            }
        }
    }

    fn children(g: &Inner, key: &str) -> Vec<String> {
        g.inst.values().filter(|c| c.owner.as_deref() == Some(key)).map(|c| c.key.clone()).collect()
    }

    fn hold<'a>(&'a self, app: &'a AppState, g: &'a mut Inner, key: &'a str, why: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            let Some(mut i) = g.inst.remove(key) else { return };
            if i.state == RunState::Running {
                i.state = RunState::Held;
                i.held_since = Some(now_ms());
                let acts = i.proc.hold.clone();
                if let Err(e) = self.run_actions(app, g, &mut i, &acts, None).await {
                    app.log("warn", &format!("seq:{key}"), format!("azioni di hold: {e:#}"));
                }
                app.log("info", &format!("seq:{key}"), why.to_string());
            }
            g.inst.insert(key.to_string(), i);
            for c in Self::children(g, key) {
                self.hold(app, g, &c, &format!("hold dal batch {key}")).await;
            }
        })
    }

    fn restart<'a>(&'a self, app: &'a AppState, g: &'a mut Inner, key: &'a str, user: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            let Some(mut i) = g.inst.remove(key) else { return };
            let now = now_ms();
            if let Some(h) = i.held_since.take() {
                shift_times(&mut i.cursor, now - h);
            }
            if i.state == RunState::Error {
                // riprova i passi falliti
                fn retry(c: &mut Cursor) {
                    if c.failed {
                        c.failed = false;
                        c.entered = false;
                    }
                    c.branches.iter_mut().for_each(retry);
                }
                retry(&mut i.cursor);
                Self::alarm(app, &i, false, "");
                i.message.clear();
            }
            let acts = i.proc.restart.clone();
            if let Err(e) = self.run_actions(app, g, &mut i, &acts, None).await {
                app.log("warn", &format!("seq:{key}"), format!("azioni di restart: {e:#}"));
            }
            i.state = RunState::Running;
            app.log("info", &format!("seq:{key}"), format!("restart da {user}"));
            g.inst.insert(key.to_string(), i);
            for c in Self::children(g, key) {
                if g.inst.get(&c).is_some_and(|x| x.state == RunState::Held) {
                    self.restart(app, g, &c, user).await;
                }
            }
        })
    }

    /// Stop / abort / completamento: azioni di stato, propagazione alle fasi, rilascio dei dispositivi.
    fn end<'a>(&'a self, app: &'a AppState, g: &'a mut Inner, key: &'a str, state: RunState, why: &'a str) -> Fut<'a, ()> {
        Box::pin(async move {
            for c in Self::children(g, key) {
                if g.inst.get(&c).is_some_and(|x| x.state.active()) {
                    self.end(app, g, &c, if state == RunState::Complete { RunState::Stopped } else { state }, &format!("{why} (batch {key})")).await;
                }
            }
            let Some(mut i) = g.inst.remove(key) else { return };
            let acts = match state {
                RunState::Stopped => i.proc.stop.clone(),
                RunState::Aborted if i.proc.abort.is_empty() => i.proc.stop.clone(),
                RunState::Aborted => i.proc.abort.clone(),
                _ => vec![],
            };
            if let Err(e) = self.run_actions(app, g, &mut i, &acts, None).await {
                app.log("warn", &format!("seq:{key}"), format!("azioni di {state:?}: {e:#}"));
            }
            let now = now_ms();
            for h in i.history.iter_mut().filter(|h| h.exit.is_none()) {
                h.exit = Some(now);
                h.reason = why.into();
            }
            i.state = state;
            i.finished = Some(now);
            i.held_since = None;
            i.message = why.into();
            Self::alarm(app, &i, false, "");
            if i.kind == "batch" {
                g.alloc.retain(|_, b| b != key);
            }
            app.log("info", &format!("seq:{key}"), format!("fine: {state:?} – {why}"));
            // registro
            let store = app.store.clone();
            let (table, k) = if i.kind == "batch" { ("batches", i.key.clone()) } else { ("seq_runs", format!("{}/{:015}", i.key, i.started)) };
            let mut rec = serde_json::to_value(&i).unwrap_or_default();
            if i.kind == "batch" {
                // storico delle fasi del batch
                rec["phases"] = serde_json::json!(g.inst.values().filter(|c| c.owner.as_deref() == Some(key)).map(|c| serde_json::json!({"key": c.key, "state": c.state, "params": c.params, "started": c.started, "finished": c.finished, "history": c.history})).collect::<Vec<_>>());
            }
            let d = serde_json::to_string(&rec).ok();
            tokio::spawn(async move {
                let _ = store.kv_put(table, k, d).await;
            });
            g.inst.insert(key.to_string(), i);
        })
    }

    async fn fail(&self, app: &AppState, g: &mut Inner, key: &str, msg: &str) {
        if let Some(i) = g.inst.get_mut(key) {
            i.state = RunState::Error;
            i.held_since = Some(now_ms());
            i.message = msg.into();
            Self::alarm(app, i, true, &format!("{} {}: {msg}", if i.kind == "batch" { "Batch" } else { "Fase" }, i.key));
            app.log("error", &format!("seq:{key}"), msg.to_string());
        }
        // le altre fasi del batch vanno in hold (stato sicuro)
        for c in Self::children(g, key) {
            if g.inst.get(&c).is_some_and(|x| x.state == RunState::Running) {
                self.hold(app, g, &c, &format!("hold per errore del batch {key}")).await;
            }
        }
    }

    // ------------------------------------------------------------ esecuzione

    fn run_actions<'a>(&'a self, app: &'a AppState, g: &'a mut Inner, i: &'a mut Instance, acts: &'a [SeqAction], mut cur: Option<&'a mut Cursor>) -> Fut<'a, Result<()>> {
        Box::pin(async move {
            for a in acts {
                match a.kind.as_str() {
                    "write" => {
                        let tag = subst(&a.tag, i);
                        let v = subst_value(a.value.as_ref().unwrap_or(&Value::Bool(true)), i);
                        app.db.write(&tag, v).await.map_err(|e| anyhow!("scrittura {tag}: {e:#}"))?;
                    }
                    "log" => app.log("info", &format!("seq:{}", i.key), subst(&a.text, i)),
                    "script" => {
                        let ctx = serde_json::json!({"tags": crate::decision::tags_ctx(app), "point": format!("seq:{}", i.key), "user": "sequenza"});
                        let payload = serde_json::json!({"instance": i.key, "name": i.name, "unit": i.device, "devices": i.devices, "params": i.params});
                        let rep = app.hmi_scripts.call(&a.func, payload, ctx, Duration::from_secs(10)).await?;
                        let errs = crate::decision::apply_script_writes(app, &rep.writes, &format!("seq:{}", i.key)).await;
                        for l in rep.logs {
                            app.log("info", &format!("seq:{}", i.key), l);
                        }
                        if !errs.is_empty() {
                            bail!("script {}: {}", a.func, errs.join("; "));
                        }
                    }
                    "phase" => {
                        let def = Self::seq_def(app, &a.sequence)?;
                        let dev = subst(&a.device, i);
                        Self::check_device(app, &def, &dev)?;
                        if !def.device_class.is_empty() && g.alloc.get(&dev) != Some(&i.key) {
                            bail!("fase {}: il dispositivo {dev} non è allocato a questo batch", def.name);
                        }
                        let given: BTreeMap<String, String> = a.params.iter().map(|(k, v)| (k.clone(), subst(v, i))).collect();
                        let params = resolve_params(&format!("fase {}", def.name), &def.parameters, &given)?;
                        let key = phase_key(&def.name, &dev);
                        match g.inst.get(&key) {
                            // nuovo tentativo dopo un errore: le fasi già concluse non si ripetono
                            Some(x) if x.owner.as_deref() == Some(&i.key) && x.state == RunState::Complete && cur.as_ref().is_some_and(|c| c.phases.contains(&key)) => {}
                            Some(x) if x.owner.as_deref() == Some(&i.key) && x.state == RunState::Error => {
                                let ph = Self::new_phase(&def, &dev, params, Some(i.key.clone()), &format!("batch {}", i.key));
                                app.log("info", &format!("batch:{}", i.key), format!("nuovo avvio della fase {key}"));
                                g.inst.insert(key.clone(), ph);
                            }
                            Some(x) if x.state.active() => bail!("fase {key} già in esecuzione"),
                            _ => {
                                let ph = Self::new_phase(&def, &dev, params, Some(i.key.clone()), &format!("batch {}", i.key));
                                app.log("info", &format!("batch:{}", i.key), format!("avvio fase {key} {:?}", ph.params));
                                g.inst.insert(key.clone(), ph);
                            }
                        }
                        if let Some(c) = cur.as_deref_mut()
                            && !c.phases.contains(&key)
                        {
                            c.phases.push(key);
                        }
                    }
                    k => bail!("azione sconosciuta {k}"),
                }
            }
            Ok(())
        })
    }

    /// Ingresso nel passo `idx` di una lista (con avvio dei rami paralleli).
    #[allow(clippy::too_many_arguments)]
    fn enter<'a>(&'a self, app: &'a AppState, g: &'a mut Inner, i: &'a mut Instance, steps: &'a [StepDef], c: &'a mut Cursor, idx: usize, path: &'a str) -> Fut<'a, Result<()>> {
        Box::pin(async move {
            let st = &steps[idx];
            let retry = c.idx == idx && !c.entered && !c.phases.is_empty();
            if !retry {
                c.phases.clear();
            }
            c.idx = idx;
            c.step = st.name.clone();
            c.since = now_ms();
            c.done = false;
            c.entered = true;
            c.failed = false;
            c.branches.clear();
            i.confirm = false;
            let p = if path.is_empty() { (idx + 1).to_string() } else { format!("{path}/{}", idx + 1) };
            i.history.push(StepRec { path: p.clone(), step: st.name.clone(), enter: c.since, exit: None, reason: String::new() });
            if i.history.len() > 1000 {
                i.history.remove(0);
            }
            if let Err(e) = self.run_actions(app, g, i, &st.actions, Some(&mut *c)).await {
                c.failed = true;
                c.entered = false;
                return Err(anyhow!("passo {}: {e:#}", st.name));
            }
            for (k, b) in st.branches.iter().enumerate() {
                let mut bc = Cursor::default();
                let bp = format!("{p}.{}", k + 1);
                let r = self.enter(app, g, i, b, &mut bc, 0, &bp).await;
                c.branches.push(bc);
                r?;
            }
            Ok(())
        })
    }

    fn cond(app: &AppState, i: &Instance, c: &SeqCond, el: f64, phases_done: bool) -> bool {
        let v = match c.tag.as_str() {
            "@elapsed" => Value::Float(el),
            "@confirm" => Value::Bool(i.confirm),
            "@phases_done" => Value::Bool(phases_done),
            t => app.db.value(&subst(t, i)),
        };
        let lim = match &c.value {
            Value::Str(x) if x.starts_with("tag:") => app.db.value(&subst(&x[4..], i)),
            x => subst_value(x, i),
        };
        let norm = |x: Value| match x {
            Value::Bool(b) => Value::Int(b as i64),
            Value::Str(ref t) if t == "true" || t == "false" => Value::Int((t == "true") as i64),
            o => o,
        };
        crate::alarms::evaluate(&c.op, &norm(lim), &norm(v))
    }

    /// Un ciclo di una lista di passi; true = lista conclusa.
    fn tick_list<'a>(&'a self, app: &'a AppState, g: &'a mut Inner, i: &'a mut Instance, steps: &'a [StepDef], c: &'a mut Cursor, path: &'a str) -> Fut<'a, Result<bool>> {
        Box::pin(async move {
            if c.done {
                return Ok(true);
            }
            if !c.entered {
                let idx = c.idx.min(steps.len().saturating_sub(1));
                self.enter(app, g, i, steps, c, idx, path).await?;
                return Ok(false);
            }
            let st = &steps[c.idx];
            let p = if path.is_empty() { (c.idx + 1).to_string() } else { format!("{path}/{}", c.idx + 1) };
            let now = now_ms();
            let el = (now - c.since).max(0) as f64 / 1000.0;
            // rami paralleli
            let mut branches_done = true;
            for (k, b) in st.branches.iter().enumerate() {
                if let Some(bc) = c.branches.get_mut(k) {
                    let bp = format!("{p}.{}", k + 1);
                    if !self.tick_list(app, g, i, b, bc, &bp).await? {
                        branches_done = false;
                    }
                }
            }
            // fasi avviate dal passo
            let mut phases_done = true;
            for k in &c.phases {
                match g.inst.get(k).map(|x| x.state) {
                    Some(RunState::Complete) | None => {}
                    Some(RunState::Error) => {
                        c.failed = true;
                        bail!("fase {k} in errore");
                    }
                    Some(s @ (RunState::Stopped | RunState::Aborted)) => {
                        c.failed = true;
                        bail!("fase {k} terminata ({s:?})");
                    }
                    _ => phases_done = false,
                }
            }
            if el >= st.min_time_s {
                let hit = if st.transitions.is_empty() {
                    (branches_done && phases_done).then_some(None)
                } else if !branches_done {
                    None
                } else {
                    st.transitions
                        .iter()
                        .find(|t| {
                            let mut it = t.when.iter().map(|x| Self::cond(app, i, x, el, phases_done));
                            if t.when.is_empty() { phases_done } else if t.mode == "any" { it.any(|x| x) } else { it.all(|x| x) }
                        })
                        .map(Some)
                };
                if let Some(t) = hit {
                    let next = t.map(|t| t.next.as_str()).unwrap_or("");
                    let why = t.map(|t| if t.label.is_empty() { "transizione".to_string() } else { t.label.clone() }).unwrap_or_else(|| "completato".into());
                    close_rec(i, &p, &why);
                    return self.go(app, g, i, steps, c, next, path).await;
                }
            }
            if st.timeout_s > 0.0 && el > st.timeout_s {
                if st.on_timeout.is_empty() {
                    c.failed = true;
                    c.entered = true;
                    bail!("timeout nel passo {} ({} s)", st.name, st.timeout_s);
                }
                close_rec(i, &p, "timeout");
                let to = st.on_timeout.clone();
                return self.go(app, g, i, steps, c, &to, path).await;
            }
            Ok(false)
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn go(&self, app: &AppState, g: &mut Inner, i: &mut Instance, steps: &[StepDef], c: &mut Cursor, next: &str, path: &str) -> Result<bool> {
        let target = match next {
            "END" => None,
            "" if c.idx + 1 >= steps.len() => None,
            "" => Some(c.idx + 1),
            n => steps.iter().position(|s| s.name == n),
        };
        match target {
            None => {
                c.done = true;
                Ok(true)
            }
            Some(t) => {
                self.enter(app, g, i, steps, c, t, path).await?;
                Ok(false)
            }
        }
    }

    /// Un ciclo del motore: avvii da PLC, avanzamento di tutte le istanze in esecuzione.
    pub async fn tick(&self, app: &AppState) {
        let cfg = app.cfg();
        // avvio dal PLC (fronte di salita) sul dispositivo predefinito
        for def in cfg.project.sequences.iter().filter(|d| !d.start_tag.is_empty()) {
            let v = app.db.value(&def.start_tag).as_bool().unwrap_or(false);
            let prev = self.inner.lock().await.edges.insert(def.name.clone(), v).unwrap_or(v);
            if v && !prev
                && let Err(e) = self.start_phase(app, &def.name, &def.unit, BTreeMap::new(), "PLC").await
            {
                app.log("warn", &format!("seq:{}", def.name), format!("avvio da PLC rifiutato: {e:#}"));
            }
        }
        let mut g = self.lock(app).await;
        let keys: Vec<String> = g.inst.iter().filter(|(_, i)| i.state == RunState::Running).map(|(k, _)| k.clone()).collect();
        for key in keys {
            let Some(mut i) = g.inst.remove(&key) else { continue };
            if i.state != RunState::Running {
                g.inst.insert(key, i);
                continue;
            }
            let proc = i.proc.clone();
            let mut c = std::mem::take(&mut i.cursor);
            let r = self.tick_list(app, &mut g, &mut i, &proc.steps, &mut c, "").await;
            i.cursor = c;
            let done = matches!(r, Ok(true));
            g.inst.insert(key.clone(), i);
            match r {
                Err(e) => self.fail(app, &mut g, &key, &format!("{e:#}")).await,
                Ok(_) if done => self.end(app, &mut g, &key, RunState::Complete, "completata").await,
                Ok(_) => {}
            }
        }
        // un batch in esecuzione si accorge delle fasi in errore al ciclo successivo;
        // le fasi concluse di batch terminati vengono rimosse
        let stale: Vec<String> = g
            .inst
            .values()
            .filter(|i| i.kind == "phase" && i.state.finished() && i.owner.as_ref().is_some_and(|o| !g.inst.get(o).is_some_and(|b| b.state.active())))
            .filter(|i| i.finished.is_some_and(|f| now_ms() - f > 60_000))
            .map(|i| i.key.clone())
            .collect();
        for k in stale {
            g.inst.remove(&k);
        }
        Self::persist(app, &mut g);
    }
}

fn close_rec(i: &mut Instance, path: &str, why: &str) {
    if let Some(h) = i.history.iter_mut().rev().find(|h| h.path == path && h.exit.is_none()) {
        h.exit = Some(now_ms());
        h.reason = why.into();
    }
}

pub async fn run(app: Arc<AppState>) {
    let mut iv = tokio::time::interval(Duration::from_millis(200));
    loop {
        iv.tick().await;
        app.sequencer.tick(&app).await;
    }
}
