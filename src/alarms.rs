//! Gestione allarmi (modello ISA-18.2 semplificato): un allarme resta in lista
//! finché non è sia rientrato sia riconosciuto (ack).
//!
//! Sorgenti:
//! - allarmi definiti sul tag (o sul membro UDT) → area del tag;
//! - allarmi globali (`alarms:`) → area esplicita o area del tag;
//! - qualità: tag con qualità BAD → un allarme aggregato per area (`<area>.QUALITY`);
//! - comunicazione: driver disconnesso → `COMM.<driver>` nell'area del driver.

use crate::app::{AppState, Event};
use crate::config::{Config, Severity};
use crate::store::AlarmEvent;
use crate::tagdb::{Quality, now_ms};
use crate::value::Value;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

/// Area usata per ciò che non ha un'area assegnata.
pub const NO_AREA: &str = "Sistema";

#[derive(Debug, Clone, Serialize)]
pub struct AlarmState {
    pub id: String,
    pub area: String,
    pub message: String,
    pub severity: Severity,
    pub active: bool,
    pub acked: bool,
    pub raised_ts: i64,
    pub cleared_ts: Option<i64>,
    pub value: String,
}

/// Riepilogo di un'area per la panoramica SCADA.
#[derive(Debug, Clone, Serialize)]
pub struct AreaSummary {
    pub name: String,
    pub description: String,
    pub parent: Option<String>,
    pub active: usize,
    pub unacked: usize,
    /// Gravità massima tra gli allarmi attivi.
    pub worst: Option<Severity>,
    pub tags: usize,
    pub bad_tags: usize,
}

#[derive(Default)]
pub struct AlarmManager {
    states: RwLock<BTreeMap<String, AlarmState>>,
}

pub fn evaluate(op: &str, limit: &Value, v: &Value) -> bool {
    match op {
        "==" | "!=" => {
            let eq = match (v.as_f64(), limit.as_f64()) {
                (Some(a), Some(b)) => a == b,
                _ => v.to_string() == limit.to_string(),
            };
            eq == (op == "==")
        }
        op => {
            let (Some(a), Some(b)) = (v.as_f64(), limit.as_f64()) else { return false };
            match op {
                ">" => a > b,
                ">=" => a >= b,
                "<" => a < b,
                "<=" => a <= b,
                _ => false,
            }
        }
    }
}

/// Definizione allarme normalizzata (da tag o globale).
struct Def {
    id: String,
    area: String,
    op: String,
    value: Value,
    message: String,
    severity: Severity,
}

fn defs_for_tag(cfg: &Config, app: &AppState, tag: &str) -> Vec<Def> {
    let Some(t) = app.db.def(tag) else { return vec![] };
    let area = t.area.clone().unwrap_or_else(|| NO_AREA.into());
    let mut out: Vec<Def> = t
        .alarms
        .iter()
        .map(|a| Def {
            id: format!("{}.{}", t.name, a.name),
            area: area.clone(),
            op: a.op.clone(),
            value: a.value.clone(),
            message: a.message.clone(),
            severity: a.severity,
        })
        .collect();
    out.extend(cfg.alarms.iter().filter(|a| a.tag == tag).map(|a| Def {
        id: a.id.clone(),
        area: a.area.clone().unwrap_or_else(|| area.clone()),
        op: a.op.clone(),
        value: a.value.clone(),
        message: a.message.clone(),
        severity: a.severity,
    }));
    out
}

impl AlarmManager {
    /// Lista corrente (attivi o non ancora riconosciuti), filtrabile per area
    /// (l'area include le sotto-aree).
    pub fn list(&self) -> Vec<AlarmState> {
        let mut v: Vec<_> = self.states.read().unwrap().values().cloned().collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.raised_ts));
        v
    }

    #[allow(clippy::too_many_arguments)]
    fn set(&self, app: &AppState, id: &str, area: &str, msg: &str, sev: Severity, active: bool, value: String) {
        let ev = {
            let mut st = self.states.write().unwrap();
            let cur = st.get(id);
            let was = cur.map(|s| s.active).unwrap_or(false);
            if was == active {
                // aggiorna il testo se cambia (es. conteggio tag in errore)
                if active {
                    if let Some(s) = st.get_mut(id) {
                        if s.message != msg {
                            s.message = msg.into();
                            drop(st);
                            let _ = app.events.send(Event::Alarms(self.list()));
                        }
                    }
                }
                return;
            }
            if active {
                st.insert(
                    id.into(),
                    AlarmState {
                        id: id.into(),
                        area: area.into(),
                        message: msg.into(),
                        severity: sev,
                        active: true,
                        acked: false,
                        raised_ts: now_ms(),
                        cleared_ts: None,
                        value: value.clone(),
                    },
                );
            } else if let Some(s) = st.get_mut(id) {
                s.active = false;
                s.cleared_ts = Some(now_ms());
                if s.acked {
                    st.remove(id);
                }
            }
            if active { "raised" } else { "cleared" }
        };
        self.publish(app, id, area, ev, msg, sev, value, None);
    }

    /// Riconoscimento operatore: un allarme, un'area (con sotto-aree) o tutti.
    pub fn ack(&self, app: &AppState, id: Option<&str>, area: Option<&str>, user: &str) -> usize {
        let cfg = app.cfg();
        let acked: Vec<AlarmState> = {
            let mut st = self.states.write().unwrap();
            let ids: Vec<String> = st
                .values()
                .filter(|s| !s.acked && id.is_none_or(|i| i == s.id) && area.is_none_or(|a| area_in(&cfg, &s.area, a)))
                .map(|s| s.id.clone())
                .collect();
            let mut out = vec![];
            for i in ids {
                let s = st.get_mut(&i).unwrap();
                s.acked = true;
                out.push(s.clone());
                if !s.active {
                    st.remove(&i);
                }
            }
            out
        };
        for s in &acked {
            self.publish(app, &s.id, &s.area, "acked", &s.message, s.severity, s.value.clone(), Some(user.into()));
        }
        acked.len()
    }

    #[allow(clippy::too_many_arguments)]
    fn publish(
        &self,
        app: &AppState,
        id: &str,
        area: &str,
        ev: &str,
        msg: &str,
        sev: Severity,
        value: String,
        user: Option<String>,
    ) {
        let e = AlarmEvent {
            ts: now_ms(),
            alarm: id.into(),
            area: area.into(),
            event: ev.into(),
            message: msg.into(),
            severity: format!("{sev:?}").to_lowercase(),
            value,
            user,
        };
        tracing::info!(alarm = id, area, event = ev, "{msg}");
        let _ = app.events.send(Event::Alarms(self.list()));
        let store = app.store.clone();
        tokio::spawn(async move {
            let _ = store.log_alarm(e).await;
        });
    }

    /// Riepilogo per area (comprende le sotto-aree).
    pub fn summary(&self, app: &AppState) -> Vec<AreaSummary> {
        let cfg = app.cfg();
        let alarms = self.list();
        let snap = app.db.snapshot();
        let defs = app.db.defs();
        let mut names: Vec<(String, String, Option<String>)> =
            cfg.areas.iter().map(|a| (a.name.clone(), a.description.clone(), a.parent.clone())).collect();
        if alarms.iter().any(|a| a.area == NO_AREA) || defs.iter().any(|t| t.area.is_none()) {
            names.push((NO_AREA.into(), "Elementi senza area".into(), None));
        }
        names
            .into_iter()
            .map(|(name, description, parent)| {
                let mine: Vec<&AlarmState> = alarms.iter().filter(|a| area_in(&cfg, &a.area, &name)).collect();
                let tags: Vec<_> = defs
                    .iter()
                    .filter(|t| area_in(&cfg, t.area.as_deref().unwrap_or(NO_AREA), &name))
                    .collect();
                AreaSummary {
                    active: mine.iter().filter(|a| a.active).count(),
                    unacked: mine.iter().filter(|a| !a.acked).count(),
                    worst: mine.iter().filter(|a| a.active).map(|a| a.severity).max(),
                    bad_tags: tags.iter().filter(|t| snap.get(&t.name).is_some_and(|s| s.quality == Quality::Bad)).count(),
                    tags: tags.len(),
                    name,
                    description,
                    parent,
                }
            })
            .collect()
    }
}

/// `area` è uguale a `target` o ne è una sotto-area?
pub fn area_in(cfg: &Config, area: &str, target: &str) -> bool {
    let mut cur = Some(area.to_string());
    let mut guard = 0;
    while let Some(a) = cur {
        if a == target {
            return true;
        }
        guard += 1;
        if guard > 32 {
            return false;
        }
        cur = cfg.areas.iter().find(|x| x.name == a).and_then(|x| x.parent.clone());
    }
    false
}

/// Task: valuta gli allarmi sui cambi tag e controlla qualità e comunicazione.
pub async fn run(app: Arc<AppState>) {
    let eval_tag = |name: &str| {
        let cfg = app.cfg();
        let Some(st) = app.db.get(name) else { return };
        if st.quality != Quality::Good {
            return; // con qualità non buona vale l'allarme di qualità dell'area
        }
        for d in defs_for_tag(&cfg, &app, name) {
            let on = evaluate(&d.op, &d.value, &st.value);
            app.alarms.set(&app, &d.id, &d.area, &d.message, d.severity, on, st.value.to_string());
        }
    };
    let mut rx = app.db.subscribe();
    let mut periodic = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            r = rx.recv() => match r {
                Ok(u) => eval_tag(&u.name),
                Err(RecvError::Lagged(_)) => { for t in app.db.defs() { eval_tag(&t.name); } }
                Err(RecvError::Closed) => return,
            },
            _ = periodic.tick() => {
                let cfg = app.cfg();
                // comunicazione driver
                let board = app.board.read().unwrap().clone();
                for (name, s) in board {
                    let area = cfg.drivers.iter().find(|d| d.name == name).and_then(|d| d.area.clone()).unwrap_or_else(|| NO_AREA.into());
                    let msg = format!("Comunicazione persa con {name}: {}", s.last_error.clone().unwrap_or_default());
                    app.alarms.set(&app, &format!("COMM.{name}"), &area, &msg, Severity::Critical, !s.connected, String::new());
                }
                // qualità per area (aggregato)
                let snap = app.db.snapshot();
                let mut bad: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for t in app.db.defs() {
                    if snap.get(&t.name).is_some_and(|s| s.quality == Quality::Bad) {
                        bad.entry(t.area.clone().unwrap_or_else(|| NO_AREA.into())).or_default().push(t.name.clone());
                    }
                }
                let mut areas: Vec<String> = cfg.areas.iter().map(|a| a.name.clone()).collect();
                areas.push(NO_AREA.into());
                for a in areas {
                    let list = bad.get(&a);
                    let n = list.map(|l| l.len()).unwrap_or(0);
                    let sample = list.map(|l| l.iter().take(5).cloned().collect::<Vec<_>>().join(", ")).unwrap_or_default();
                    let msg = format!("{a}: {n} tag con qualità BAD ({sample}{})", if n > 5 { ", …" } else { "" });
                    app.alarms.set(&app, &format!("{a}.QUALITY"), &a, &msg, Severity::Alarm, n > 0, n.to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eval() {
        assert!(evaluate("==", &Value::Bool(true), &Value::Bool(true)));
        assert!(!evaluate("==", &Value::Bool(true), &Value::Int(0)));
        assert!(evaluate(">", &Value::Float(80.0), &Value::Int(81)));
        assert!(!evaluate(">", &Value::Float(80.0), &Value::Null));
    }
}
