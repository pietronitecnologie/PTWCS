//! Persistenza su SQLite: log decisioni, eventi allarme, storico tag (trend),
//! audit dei comandi operatore.

use anyhow::Result;
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value as Json;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Store {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionRecord {
    pub ts: i64,
    pub point: String,
    pub request_id: i64,
    pub inputs: Json,
    pub outputs: Json,
    pub duration_ms: f64,
    /// ok | fallback | error
    pub status: String,
    pub error: Option<String>,
    /// Regola grafica che ha deciso (None = script o fallback).
    pub rule: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlarmEvent {
    pub ts: i64,
    pub alarm: String,
    pub area: String,
    /// raised | cleared | acked
    pub event: String,
    pub message: String,
    pub severity: String,
    pub value: String,
    pub user: Option<String>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             CREATE TABLE IF NOT EXISTS decisions(
                ts INTEGER, point TEXT, request_id INTEGER, inputs TEXT, outputs TEXT,
                duration_ms REAL, status TEXT, error TEXT);
             CREATE INDEX IF NOT EXISTS ix_dec_ts ON decisions(ts);
             CREATE TABLE IF NOT EXISTS alarm_events(
                ts INTEGER, alarm TEXT, event TEXT, message TEXT, severity TEXT, value TEXT, user TEXT);
             CREATE INDEX IF NOT EXISTS ix_al_ts ON alarm_events(ts);
             CREATE TABLE IF NOT EXISTS history(ts INTEGER, tag TEXT, value REAL, text TEXT);
             CREATE INDEX IF NOT EXISTS ix_hist ON history(tag, ts);
             CREATE TABLE IF NOT EXISTS audit(ts INTEGER, user TEXT, action TEXT, detail TEXT);",
        )?;
        // migrazione: colonna 'rule' aggiunta in seguito (errore ignorato se esiste già)
        let _ = conn.execute("ALTER TABLE decisions ADD COLUMN rule TEXT", []);
        let _ = conn.execute("ALTER TABLE alarm_events ADD COLUMN area TEXT", []);
        Ok(Self { conn: Arc::new(Mutex::new(conn)) })
    }

    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> rusqlite::Result<T> + Send + 'static,
    ) -> Result<T> {
        let c = self.conn.clone();
        Ok(tokio::task::spawn_blocking(move || f(&c.lock().unwrap())).await??)
    }

    pub async fn log_decision(&self, r: DecisionRecord) -> Result<()> {
        self.run(move |c| {
            c.execute(
                "INSERT INTO decisions(ts,point,request_id,inputs,outputs,duration_ms,status,error,rule) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![r.ts, r.point, r.request_id, r.inputs.to_string(), r.outputs.to_string(), r.duration_ms, r.status, r.error, r.rule],
            )
        })
        .await?;
        Ok(())
    }

    pub async fn decisions(&self, point: Option<String>, limit: u32) -> Result<Vec<DecisionRecord>> {
        self.run(move |c| {
            let mut st = c.prepare(
                "SELECT ts,point,request_id,inputs,outputs,duration_ms,status,error,rule FROM decisions
                 WHERE (?1 IS NULL OR point=?1) ORDER BY ts DESC, rowid DESC LIMIT ?2",
            )?;
            st.query_map(params![point, limit], |r| {
                Ok(DecisionRecord {
                    ts: r.get(0)?,
                    point: r.get(1)?,
                    request_id: r.get(2)?,
                    inputs: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or(Json::Null),
                    outputs: serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or(Json::Null),
                    duration_ms: r.get(5)?,
                    status: r.get(6)?,
                    error: r.get(7)?,
                    rule: r.get(8)?,
                })
            })?
            .collect()
        })
        .await
    }

    pub async fn log_alarm(&self, e: AlarmEvent) -> Result<()> {
        self.run(move |c| {
            c.execute(
                "INSERT INTO alarm_events(ts,alarm,event,message,severity,value,user,area) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
                params![e.ts, e.alarm, e.event, e.message, e.severity, e.value, e.user, e.area],
            )
        })
        .await?;
        Ok(())
    }

    pub async fn alarm_history(&self, limit: u32) -> Result<Vec<AlarmEvent>> {
        self.run(move |c| {
            let mut st = c.prepare(
                "SELECT ts,alarm,event,message,severity,value,user,COALESCE(area,'') FROM alarm_events ORDER BY ts DESC, rowid DESC LIMIT ?1",
            )?;
            st.query_map(params![limit], |r| {
                Ok(AlarmEvent {
                    ts: r.get(0)?,
                    alarm: r.get(1)?,
                    event: r.get(2)?,
                    message: r.get(3)?,
                    severity: r.get(4)?,
                    value: r.get(5)?,
                    user: r.get(6)?,
                    area: r.get(7)?,
                })
            })?
            .collect()
        })
        .await
    }

    pub async fn log_history(&self, ts: i64, tag: String, value: Option<f64>, text: Option<String>) -> Result<()> {
        self.run(move |c| c.execute("INSERT INTO history VALUES(?1,?2,?3,?4)", params![ts, tag, value, text])).await?;
        Ok(())
    }

    /// Serie storica di un tag: [(ts, valore)].
    pub async fn trend(&self, tag: String, from: i64, to: i64) -> Result<Vec<(i64, Option<f64>)>> {
        self.run(move |c| {
            let mut st = c.prepare("SELECT ts,value FROM history WHERE tag=?1 AND ts BETWEEN ?2 AND ?3 ORDER BY ts LIMIT 20000")?;
            st.query_map(params![tag, from, to], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
        })
        .await
    }

    pub async fn audit(&self, ts: i64, user: String, action: String, detail: String) -> Result<()> {
        self.run(move |c| c.execute("INSERT INTO audit VALUES(?1,?2,?3,?4)", params![ts, user, action, detail])).await?;
        Ok(())
    }

    /// Pulizia dati oltre la retention (giorni).
    pub async fn purge(&self, before_ts: i64) -> Result<()> {
        self.run(move |c| {
            c.execute("DELETE FROM history WHERE ts < ?1", params![before_ts])?;
            c.execute("DELETE FROM decisions WHERE ts < ?1", params![before_ts])
        })
        .await?;
        Ok(())
    }
}
