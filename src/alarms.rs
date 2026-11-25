//! Gestione allarmi (modello ISA-18.2 semplificato): un allarme resta in lista
//! finché non è sia rientrato sia riconosciuto (ack). Gli allarmi di
//! comunicazione dei driver sono generati automaticamente.

use crate::app::{AppState, Event};
use crate::config::{AlarmDef, Severity};
use crate::store::AlarmEvent;
use crate::tagdb::{Quality, now_ms};
use crate::value::Value;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

#[derive(Debug, Clone, Serialize)]
pub struct AlarmState {
    pub id: String,
    pub message: String,
    pub severity: Severity,
    pub active: bool,
    pub acked: bool,
    pub raised_ts: i64,
    pub cleared_ts: Option<i64>,
    pub value: String,
}

#[derive(Default)]
pub struct AlarmManager {
    states: RwLock<BTreeMap<String, AlarmState>>,
}

pub fn evaluate(def: &AlarmDef, v: &Value) -> bool {
    match def.op.as_str() {
        "==" | "!=" => {
            let eq = match (v.as_f64(), def.value.as_f64()) {
                (Some(a), Some(b)) => a == b,
                _ => v.to_string() == def.value.to_string(),
            };
            eq == (def.op == "==")
        }
        op => {
            let (Some(a), Some(b)) = (v.as_f64(), def.value.as_f64()) else { return false };
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

impl AlarmManager {
    /// Lista corrente (attivi o non ancora riconosciuti).
    pub fn list(&self) -> Vec<AlarmState> {
        let mut v: Vec<_> = self.states.read().unwrap().values().cloned().collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.raised_ts));
        v
    }

    fn set(&self, app: &AppState, id: &str, msg: &str, sev: Severity, active: bool, value: String) {
        let ev = {
            let mut st = self.states.write().unwrap();
            let cur = st.get(id).map(|s| s.active).unwrap_or(false);
            if cur == active {
                return;
            }
            if active {
                st.insert(
                    id.into(),
                    AlarmState {
                        id: id.into(),
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
        self.publish(app, id, ev, msg, sev, value, None);
    }

    /// Riconoscimento operatore. `id` = None riconosce tutti.
    pub fn ack(&self, app: &AppState, id: Option<&str>, user: &str) -> usize {
        let acked: Vec<AlarmState> = {
            let mut st = self.states.write().unwrap();
            let ids: Vec<String> =
                st.values().filter(|s| !s.acked && id.is_none_or(|i| i == s.id)).map(|s| s.id.clone()).collect();
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
            self.publish(app, &s.id, "acked", &s.message, s.severity, s.value.clone(), Some(user.into()));
        }
        acked.len()
    }

    #[allow(clippy::too_many_arguments)]
    fn publish(&self, app: &AppState, id: &str, ev: &str, msg: &str, sev: Severity, value: String, user: Option<String>) {
        let e = AlarmEvent {
            ts: now_ms(),
            alarm: id.into(),
            event: ev.into(),
            message: msg.into(),
            severity: format!("{sev:?}").to_lowercase(),
            value,
            user,
        };
        tracing::info!(alarm = id, event = ev, "{msg}");
        let _ = app.events.send(Event::Alarms(self.list()));
        let store = app.store.clone();
        tokio::spawn(async move {
            let _ = store.log_alarm(e).await;
        });
    }
}

/// Task: valuta gli allarmi sui cambi tag e controlla lo stato dei driver.
pub async fn run(app: Arc<AppState>) {
    let mut by_tag: BTreeMap<String, Vec<AlarmDef>> = BTreeMap::new();
    for a in &app.cfg.alarms {
        by_tag.entry(a.tag.clone()).or_default().push(a.clone());
    }
    let eval_tag = |name: &str| {
        if let (Some(defs), Some(st)) = (by_tag.get(name), app.db.get(name)) {
            if st.quality != Quality::Good {
                return; // con qualità Bad vale l'allarme di comunicazione
            }
            for d in defs {
                app.alarms.set(&app, &d.id, &d.message, d.severity, evaluate(d, &st.value), st.value.to_string());
            }
        }
    };
    let mut rx = app.db.subscribe();
    let mut comm = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            r = rx.recv() => match r {
                Ok(u) => eval_tag(&u.name),
                Err(RecvError::Lagged(_)) => { for t in by_tag.keys() { eval_tag(t); } }
                Err(RecvError::Closed) => return,
            },
            _ = comm.tick() => {
                let board = app.board.read().unwrap().clone();
                for (name, s) in board {
                    let msg = format!("Comunicazione persa con {name}: {}", s.last_error.clone().unwrap_or_default());
                    app.alarms.set(&app, &format!("COMM.{name}"), &msg, Severity::Critical, !s.connected, String::new());
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
        let mut d: AlarmDef = serde_yaml::from_str("{id: a, tag: t, message: m}").unwrap();
        assert!(evaluate(&d, &Value::Bool(true)));
        assert!(!evaluate(&d, &Value::Int(0)));
        d.op = ">".into();
        d.value = Value::Float(80.0);
        assert!(evaluate(&d, &Value::Int(81)));
        assert!(!evaluate(&d, &Value::Null));
    }
}
