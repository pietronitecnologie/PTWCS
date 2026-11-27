//! Sequenze di processo a passi (stile SFC).
//!
//! Ogni passo esegue le sue **azioni** all'ingresso (scrittura tag, caricamento
//! ricetta, script Python, messaggio) e poi attende una **transizione**: la prima
//! le cui condizioni sui tag sono vere porta al passo successivo. Pseudo-tag nelle
//! condizioni: `@elapsed` (secondi nel passo) e `@confirm` (conferma operatore).
//! Ogni passo può avere tempo minimo, timeout e passo di timeout.
//!
//! Comandi: start (con parametri, es. ricetta), stop (azioni di arresto), pausa,
//! ripresa, reset, avanti (passo successivo), vai a passo, conferma. Avvio anche
//! dal PLC sul fronte di salita di `start_tag`. Ogni esecuzione è registrata
//! con lo storico dei passi; i comandi finiscono nell'audit trail.

use crate::alarms::NO_AREA;
use crate::app::AppState;
use crate::config::{SeqAction, SeqCond, SequenceDef, Severity};
use crate::tagdb::now_ms;
use crate::value::Value;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeqStatus {
    Idle,
    Running,
    Paused,
    Done,
    Stopped,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepRec {
    pub step: String,
    pub enter: i64,
    pub exit: Option<i64>,
    /// motivo dell'uscita: transizione, timeout, manuale, arresto…
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeqState {
    pub name: String,
    pub status: SeqStatus,
    pub step: Option<usize>,
    pub step_name: String,
    pub step_since: i64,
    /// tempo nel passo corrente (s, pause escluse)
    pub elapsed_s: f64,
    #[serde(skip)]
    paused_ms: i64,
    #[serde(skip)]
    paused_since: Option<i64>,
    pub started: Option<i64>,
    pub finished: Option<i64>,
    pub params: BTreeMap<String, String>,
    pub history: Vec<StepRec>,
    pub message: String,
    pub confirm: bool,
    pub run_id: i64,
    pub user: String,
}

impl SeqState {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            status: SeqStatus::Idle,
            step: None,
            step_name: String::new(),
            step_since: 0,
            elapsed_s: 0.0,
            paused_ms: 0,
            paused_since: None,
            started: None,
            finished: None,
            params: BTreeMap::new(),
            history: vec![],
            message: String::new(),
            confirm: false,
            run_id: 0,
            user: String::new(),
        }
    }
    fn elapsed(&self, now: i64) -> f64 {
        let paused = self.paused_ms + self.paused_since.map(|p| now - p).unwrap_or(0);
        ((now - self.step_since - paused).max(0)) as f64 / 1000.0
    }
    fn active(&self) -> bool {
        matches!(self.status, SeqStatus::Running | SeqStatus::Paused)
    }
}

/// Dove andare dopo un passo.
enum Target {
    Step(usize),
    End,
}

#[derive(Default)]
pub struct Sequencer {
    states: Mutex<HashMap<String, SeqState>>,
    start_edges: Mutex<HashMap<String, bool>>,
}

/// Sostituisce `{unit}` e `{parametro}` nelle stringhe della sequenza.
fn subst(s: &str, def: &SequenceDef, params: &BTreeMap<String, String>) -> String {
    let mut out = s.replace("{unit}", &def.unit);
    for (k, v) in params {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

fn subst_value(v: &Value, def: &SequenceDef, params: &BTreeMap<String, String>) -> Value {
    match v {
        Value::Str(s) if s.contains('{') => {
            let t = subst(s, def, params);
            t.parse::<i64>().map(Value::Int).or_else(|_| t.parse::<f64>().map(Value::Float)).unwrap_or(Value::Str(t))
        }
        x => x.clone(),
    }
}

impl Sequencer {
    pub fn states(&self, app: &AppState) -> Vec<SeqState> {
        let cfg = app.cfg();
        let now = now_ms();
        let mut st = self.states.lock().unwrap();
        cfg.project
            .sequences
            .iter()
            .map(|d| {
                let s = st.entry(d.name.clone()).or_insert_with(|| SeqState::new(&d.name));
                s.elapsed_s = if s.step.is_some() && s.active() { s.elapsed(now) } else { 0.0 };
                s.clone()
            })
            .collect()
    }

    pub fn state(&self, app: &AppState, name: &str) -> Option<SeqState> {
        self.states(app).into_iter().find(|s| s.name == name)
    }

    fn def(app: &AppState, name: &str) -> Result<SequenceDef> {
        app.cfg().project.sequences.iter().find(|d| d.name == name).cloned().ok_or_else(|| anyhow!("sequenza {name} non trovata"))
    }

    fn alarm(app: &AppState, def: &SequenceDef, active: bool, msg: &str) {
        let area = def.area.clone().unwrap_or_else(|| NO_AREA.into());
        app.alarms.raise(app, &format!("SEQ.{}", def.name), &area, msg, Severity::Alarm, active);
    }

    /// Comando operatore (o PLC). `step` per start/goto, `params` per start.
    pub async fn command(&self, app: &AppState, name: &str, cmd: &str, step: Option<&str>, params: BTreeMap<String, String>, user: &str) -> Result<SeqState> {
        let def = Self::def(app, name)?;
        let now = now_ms();
        let find_step = |n: &str| def.steps.iter().position(|s| s.name == n).ok_or_else(|| anyhow!("passo {n} non trovato"));
        match cmd {
            "start" => {
                if def.steps.is_empty() {
                    bail!("la sequenza non ha passi");
                }
                let first = match step.filter(|s| !s.is_empty()) {
                    Some(s) => find_step(s)?,
                    None => 0,
                };
                {
                    let mut st = self.states.lock().unwrap();
                    let s = st.entry(name.into()).or_insert_with(|| SeqState::new(name));
                    if s.active() {
                        bail!("sequenza già in esecuzione");
                    }
                    let mut p = def.params.clone();
                    p.extend(params);
                    *s = SeqState { params: p, status: SeqStatus::Running, started: Some(now), run_id: now, user: user.into(), ..SeqState::new(name) };
                }
                Self::alarm(app, &def, false, "");
                app.log("info", &format!("seq:{name}"), format!("avvio da {user}"));
                self.enter(app, &def, first, "avvio").await;
            }
            "stop" => {
                {
                    let mut st = self.states.lock().unwrap();
                    let s = st.get_mut(name).ok_or_else(|| anyhow!("sequenza non avviata"))?;
                    if !s.active() {
                        bail!("sequenza non in esecuzione");
                    }
                }
                let params = self.states.lock().unwrap()[name].params.clone();
                for a in &def.stop_actions {
                    if let Err(e) = self.action(app, &def, &params, a).await {
                        app.log("warn", &format!("seq:{name}"), format!("azione di arresto: {e:#}"));
                    }
                }
                self.finish(app, &def, SeqStatus::Stopped, &format!("arresto da {user}"));
            }
            "pause" | "resume" => {
                let mut st = self.states.lock().unwrap();
                let s = st.get_mut(name).ok_or_else(|| anyhow!("sequenza non avviata"))?;
                match (cmd, s.status) {
                    ("pause", SeqStatus::Running) => {
                        s.status = SeqStatus::Paused;
                        s.paused_since = Some(now);
                    }
                    ("resume", SeqStatus::Paused) => {
                        s.status = SeqStatus::Running;
                        s.paused_ms += now - s.paused_since.take().unwrap_or(now);
                    }
                    _ => bail!("comando {cmd} non ammesso nello stato {:?}", s.status),
                }
            }
            "reset" => {
                {
                    let mut st = self.states.lock().unwrap();
                    let s = st.entry(name.into()).or_insert_with(|| SeqState::new(name));
                    if s.active() {
                        bail!("arresta la sequenza prima del reset");
                    }
                    *s = SeqState::new(name);
                }
                Self::alarm(app, &def, false, "");
            }
            "confirm" => {
                let mut st = self.states.lock().unwrap();
                let s = st.get_mut(name).filter(|s| s.active()).ok_or_else(|| anyhow!("sequenza non in esecuzione"))?;
                s.confirm = true;
            }
            "next" | "goto" => {
                let cur = {
                    let st = self.states.lock().unwrap();
                    let s = st.get(name).filter(|s| s.active()).ok_or_else(|| anyhow!("sequenza non in esecuzione"))?;
                    s.step.unwrap_or(0)
                };
                let target = if cmd == "goto" {
                    match step {
                        Some("END") => Target::End,
                        Some(n) => Target::Step(find_step(n)?),
                        None => bail!("passo mancante"),
                    }
                } else {
                    self.resolve_next(&def, cur, def.steps[cur].transitions.first().map(|t| t.next.as_str()).unwrap_or(""))
                };
                self.go(app, &def, target, &format!("manuale ({user})")).await;
            }
            _ => bail!("comando sconosciuto: {cmd} (start, stop, pause, resume, reset, next, goto, confirm)"),
        }
        self.state(app, name).ok_or_else(|| anyhow!("stato non disponibile"))
    }

    fn resolve_next(&self, def: &SequenceDef, cur: usize, next: &str) -> Target {
        match next {
            "END" => Target::End,
            "" if cur + 1 >= def.steps.len() => Target::End,
            "" => Target::Step(cur + 1),
            n => def.steps.iter().position(|s| s.name == n).map(Target::Step).unwrap_or(Target::End),
        }
    }

    async fn go(&self, app: &AppState, def: &SequenceDef, target: Target, reason: &str) {
        {
            let mut st = self.states.lock().unwrap();
            if let Some(s) = st.get_mut(&def.name)
                && let Some(h) = s.history.last_mut()
                && h.exit.is_none()
            {
                h.exit = Some(now_ms());
                h.reason = reason.into();
            }
        }
        match target {
            Target::End => self.finish(app, def, SeqStatus::Done, "completata"),
            Target::Step(i) => self.enter(app, def, i, reason).await,
        }
    }

    /// Ingresso in un passo: aggiorna lo stato ed esegue le azioni.
    async fn enter(&self, app: &AppState, def: &SequenceDef, idx: usize, why: &str) {
        let step = &def.steps[idx];
        let params = {
            let mut st = self.states.lock().unwrap();
            let s = st.entry(def.name.clone()).or_insert_with(|| SeqState::new(&def.name));
            let now = now_ms();
            s.step = Some(idx);
            s.step_name = step.name.clone();
            s.step_since = now;
            s.paused_ms = 0;
            if s.status == SeqStatus::Paused {
                s.paused_since = Some(now);
            }
            s.confirm = false;
            s.message.clear();
            s.history.push(StepRec { step: step.name.clone(), enter: now, exit: None, reason: String::new() });
            if s.history.len() > 500 {
                s.history.remove(0);
            }
            s.params.clone()
        };
        app.log("info", &format!("seq:{}", def.name), format!("passo {} ({why})", step.name));
        for a in &step.actions {
            if let Err(e) = self.action(app, def, &params, a).await {
                let msg = format!("passo {}: {e:#}", step.name);
                self.fail(app, def, &msg);
                return;
            }
        }
    }

    async fn action(&self, app: &AppState, def: &SequenceDef, params: &BTreeMap<String, String>, a: &SeqAction) -> Result<()> {
        match a.kind.as_str() {
            "write" => {
                let tag = subst(&a.tag, def, params);
                let v = subst_value(a.value.as_ref().unwrap_or(&Value::Bool(true)), def, params);
                app.db.write(&tag, v).await.map_err(|e| anyhow!("scrittura {tag}: {e:#}"))
            }
            "recipe" => {
                let name = subst(&a.recipe, def, params);
                if name.is_empty() {
                    bail!("ricetta non indicata (parametro di avvio mancante?)");
                }
                let res = app.recipes.load(&app.cfg(), &app.db, &a.recipe_type, &name, &def.unit, &format!("seq:{}", def.name)).await?;
                if let Some(e) = res.iter().find(|r| !r.ok) {
                    bail!("ricetta {name}: {} {}", e.tag, e.error.clone().unwrap_or_default());
                }
                app.audit_ext("sequenza", "recipe_load", &format!("{}/{}", a.recipe_type, name), None, Some(serde_json::to_string(&res.iter().map(|r| (r.param.clone(), r.value.clone())).collect::<BTreeMap<_, _>>()).unwrap_or_default()), format!("sequenza {} unità {}", def.name, def.unit));
                Ok(())
            }
            "script" => {
                let ctx = serde_json::json!({"tags": crate::decision::tags_ctx(app), "point": format!("seq:{}", def.name), "user": "sequenza"});
                let payload = serde_json::json!({"sequence": def.name, "unit": def.unit, "params": params});
                let rep = app.hmi_scripts.call(&a.func, payload, ctx, Duration::from_secs(10)).await?;
                let errs = crate::decision::apply_script_writes(app, &rep.writes, &format!("seq:{}", def.name)).await;
                for l in rep.logs {
                    app.log("info", &format!("seq:{}", def.name), l);
                }
                if !errs.is_empty() {
                    bail!("script {}: {}", a.func, errs.join("; "));
                }
                Ok(())
            }
            "log" => {
                app.log("info", &format!("seq:{}", def.name), subst(&a.text, def, params));
                Ok(())
            }
            k => bail!("azione sconosciuta {k}"),
        }
    }

    fn fail(&self, app: &AppState, def: &SequenceDef, msg: &str) {
        app.log("error", &format!("seq:{}", def.name), msg.to_string());
        Self::alarm(app, def, true, &format!("Sequenza {}: {msg}", def.name));
        self.finish(app, def, SeqStatus::Error, msg);
    }

    fn finish(&self, app: &AppState, def: &SequenceDef, status: SeqStatus, msg: &str) {
        let rec = {
            let mut st = self.states.lock().unwrap();
            let Some(s) = st.get_mut(&def.name) else { return };
            let now = now_ms();
            if let Some(h) = s.history.last_mut()
                && h.exit.is_none()
            {
                h.exit = Some(now);
                h.reason = msg.into();
            }
            s.status = status;
            s.finished = Some(now);
            s.paused_since = None;
            s.message = msg.into();
            s.clone()
        };
        app.log(if status == SeqStatus::Error { "error" } else { "info" }, &format!("seq:{}", def.name), format!("fine: {status:?} – {msg}"));
        // registro delle esecuzioni
        let store = app.store.clone();
        let k = format!("{}/{:015}", def.name, rec.run_id);
        let d = serde_json::to_string(&rec).ok();
        tokio::spawn(async move {
            let _ = store.kv_put("seq_runs", k, d).await;
        });
    }

    fn cond_true(app: &AppState, def: &SequenceDef, s: &SeqState, c: &SeqCond, now: i64) -> bool {
        let v = match c.tag.as_str() {
            "@elapsed" => Value::Float(s.elapsed(now)),
            "@confirm" => Value::Bool(s.confirm),
            t => app.db.value(&subst(t, def, &s.params)),
        };
        let lim = match &c.value {
            Value::Str(x) if x.starts_with("tag:") => app.db.value(&subst(&x[4..], def, &s.params)),
            x => subst_value(x, def, &s.params),
        };
        // i booleani confrontati con numeri (1/0)
        let norm = |x: Value| match x {
            Value::Bool(b) => Value::Int(b as i64),
            Value::Str(ref t) if t == "true" || t == "false" => Value::Int((t == "true") as i64),
            o => o,
        };
        crate::alarms::evaluate(&c.op, &norm(lim), &norm(v))
    }

    /// Un ciclo del motore: avvii da PLC, transizioni e timeout.
    pub async fn tick(&self, app: &AppState) {
        let cfg = app.cfg();
        let now = now_ms();
        for def in &cfg.project.sequences {
            // avvio dal PLC (fronte di salita)
            if !def.start_tag.is_empty() {
                let v = app.db.value(&def.start_tag).as_bool().unwrap_or(false);
                let prev = self.start_edges.lock().unwrap().insert(def.name.clone(), v).unwrap_or(v);
                if v && !prev
                    && let Err(e) = self.command(app, &def.name, "start", None, BTreeMap::new(), "PLC").await
                {
                    app.log("warn", &format!("seq:{}", def.name), format!("avvio da PLC rifiutato: {e:#}"));
                }
            }
            let Some(s) = self.states.lock().unwrap().get(&def.name).cloned() else { continue };
            if s.status != SeqStatus::Running {
                continue;
            }
            let Some(idx) = s.step.filter(|&i| i < def.steps.len()) else { continue };
            let step = &def.steps[idx];
            let el = s.elapsed(now);
            if el >= step.min_time_s {
                let hit = step.transitions.iter().find(|t| {
                    let mut it = t.when.iter().map(|c| Self::cond_true(app, def, &s, c, now));
                    if t.when.is_empty() { true } else if t.mode == "any" { it.any(|x| x) } else { it.all(|x| x) }
                });
                if let Some(t) = hit {
                    let target = self.resolve_next(def, idx, &t.next);
                    let why = if t.label.is_empty() { "transizione".to_string() } else { t.label.clone() };
                    self.go(app, def, target, &why).await;
                    continue;
                }
            }
            if step.timeout_s > 0.0 && el > step.timeout_s {
                if step.on_timeout.is_empty() {
                    self.fail(app, def, &format!("timeout nel passo {} ({} s)", step.name, step.timeout_s));
                } else {
                    let target = self.resolve_next(def, idx, &step.on_timeout);
                    app.log("warn", &format!("seq:{}", def.name), format!("timeout nel passo {}", step.name));
                    self.go(app, def, target, "timeout").await;
                }
            }
        }
    }
}

pub async fn run(app: Arc<AppState>) {
    let mut iv = tokio::time::interval(Duration::from_millis(200));
    loop {
        iv.tick().await;
        app.sequencer.tick(&app).await;
    }
}
