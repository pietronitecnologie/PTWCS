//! Persistenza su SQLite: log decisioni, eventi allarme, storico tag (trend),
//! audit dei comandi operatore.

/// Riga dell'audit trail: chi, quando, cosa, su quale oggetto, valore prima/dopo, da dove.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AuditRec {
    pub ts: i64,
    pub user: String,
    pub action: String,
    pub detail: String,
    pub category: String,
    pub target: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub client: String,
}

#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    pub from: i64,
    pub to: i64,
    pub user: String,
    pub category: String,
    pub action: String,
    pub q: String,
    pub limit: u32,
    pub offset: u32,
}

use anyhow::Result;
use crate::tagdb::now_ms;
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
        // audit esteso (migrazione: colonne aggiunte se mancano)
        for col in ["category TEXT", "target TEXT", "old_value TEXT", "new_value TEXT", "client TEXT"] {
            let _ = conn.execute(&format!("ALTER TABLE audit ADD COLUMN {col}"), []);
        }
        conn.execute_batch("CREATE INDEX IF NOT EXISTS ix_audit_ts ON audit(ts);")?;
        // migrazione: colonna 'rule' aggiunta in seguito (errore ignorato se esiste già)
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS wh_locations(wh TEXT, idx INTEGER, state TEXT, lu TEXT, ts INTEGER, PRIMARY KEY(wh, idx));
             CREATE TABLE IF NOT EXISTS wh_missions(wh TEXT, id INTEGER, state TEXT, data TEXT, ts INTEGER, PRIMARY KEY(wh, id));
             CREATE TABLE IF NOT EXISTS wh_lus(wh TEXT, lu TEXT, data TEXT, PRIMARY KEY(wh, lu));
             CREATE TABLE IF NOT EXISTS wh_orders(wh TEXT, id INTEGER, state TEXT, data TEXT, ts INTEGER, PRIMARY KEY(wh, id));
             CREATE TABLE IF NOT EXISTS wh_picks(wh TEXT, sku TEXT, ts INTEGER);
             CREATE INDEX IF NOT EXISTS ix_wh_picks ON wh_picks(wh, ts);
             CREATE TABLE IF NOT EXISTS sec_users(k TEXT PRIMARY KEY, data TEXT);
             CREATE TABLE IF NOT EXISTS sec_sessions(k TEXT PRIMARY KEY, data TEXT);
             CREATE TABLE IF NOT EXISTS recipe_procs(k TEXT PRIMARY KEY, data TEXT);
             CREATE TABLE IF NOT EXISTS batches(k TEXT PRIMARY KEY, data TEXT);
             CREATE TABLE IF NOT EXISTS active_inst(k TEXT PRIMARY KEY, data TEXT);
             CREATE TABLE IF NOT EXISTS seq_runs(k TEXT PRIMARY KEY, data TEXT);",
        )?;
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
        self.audit_rec(AuditRec { ts, user, action, detail, ..Default::default() }).await
    }

    pub async fn audit_rec(&self, r: AuditRec) -> Result<()> {
        self.run(move |c| {
            c.execute(
                "INSERT INTO audit(ts,user,action,detail,category,target,old_value,new_value,client) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![r.ts, r.user, r.action, r.detail, r.category, r.target, r.old_value, r.new_value, r.client],
            )
        })
        .await?;
        Ok(())
    }

    /// Ricerca nell'audit (più recenti prima). Ritorna (righe, totale).
    pub async fn audit_query(&self, f: AuditFilter) -> Result<(Vec<AuditRec>, u64)> {
        self.run(move |c| {
            let mut w = vec!["ts >= ?1 AND ts <= ?2".to_string()];
            let mut args: Vec<rusqlite::types::Value> = vec![f.from.into(), f.to.into()];
            let mut add = |cond: &str, v: String| {
                args.push(v.into());
                w.push(cond.replace('?', &format!("?{}", args.len())));
            };
            if !f.user.is_empty() {
                add("user LIKE ?", format!("%{}%", f.user));
            }
            if !f.category.is_empty() {
                add("category = ?", f.category.clone());
            }
            if !f.action.is_empty() {
                add("action = ?", f.action.clone());
            }
            if !f.q.is_empty() {
                let q = format!("%{}%", f.q);
                args.push(q.into());
                let n = args.len();
                w.push(format!("(detail LIKE ?{n} OR target LIKE ?{n} OR action LIKE ?{n} OR user LIKE ?{n} OR new_value LIKE ?{n} OR old_value LIKE ?{n})"));
            }
            let wh = w.join(" AND ");
            let total: i64 = c.query_row(&format!("SELECT COUNT(*) FROM audit WHERE {wh}"), rusqlite::params_from_iter(args.iter()), |r| r.get(0))?;
            let total = total as u64;
            let sql = format!(
                "SELECT ts,user,action,detail,COALESCE(category,''),COALESCE(target,''),old_value,new_value,COALESCE(client,'') FROM audit WHERE {wh} ORDER BY ts DESC LIMIT {} OFFSET {}",
                f.limit.min(100_000),
                f.offset
            );
            let mut st = c.prepare(&sql)?;
            let rows = st
                .query_map(rusqlite::params_from_iter(args.iter()), |r| {
                    Ok(AuditRec {
                        ts: r.get(0)?,
                        user: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        action: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        detail: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                        category: r.get(4)?,
                        target: r.get(5)?,
                        old_value: r.get(6)?,
                        new_value: r.get(7)?,
                        client: r.get(8)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok((rows, total))
        })
        .await
    }

    /// Utenti e azioni presenti (filtri della pagina audit).
    pub async fn audit_facets(&self) -> Result<(Vec<String>, Vec<String>)> {
        self.run(|c| {
            let col = |sql: &str| -> rusqlite::Result<Vec<String>> {
                let mut st = c.prepare(sql)?;
                st.query_map([], |r| r.get::<_, Option<String>>(0))?.map(|x| x.map(|v| v.unwrap_or_default())).collect()
            };
            Ok((col("SELECT DISTINCT action FROM audit ORDER BY action")?, col("SELECT DISTINCT category FROM audit WHERE category IS NOT NULL ORDER BY category")?))
        })
        .await
    }

    // ---------------------------------------------------------------- magazzino

    pub async fn wh_load_locs(&self, wh: String) -> Result<Vec<(u32, String, Option<String>, i64)>> {
        self.run(move |c| {
            let mut st = c.prepare("SELECT idx,state,lu,ts FROM wh_locations WHERE wh=?1")?;
            st.query_map(params![wh], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect()
        })
        .await
    }

    pub async fn wh_save_locs(&self, wh: String, rows: Vec<(u32, String, Option<String>, i64)>) -> Result<()> {
        self.run(move |c| {
            let tx = c.unchecked_transaction()?;
            {
                let mut st = tx.prepare("INSERT OR REPLACE INTO wh_locations VALUES(?1,?2,?3,?4,?5)")?;
                for (i, s, lu, ts) in rows {
                    st.execute(params![wh, i, s, lu, ts])?;
                }
            }
            tx.commit()
        })
        .await
    }

    pub async fn wh_save_mission(&self, wh: String, m: crate::warehouse::Mission) -> Result<()> {
        let state = serde_json::to_value(m.state)?.as_str().unwrap_or("").to_string();
        let data = serde_json::to_string(&m)?;
        self.run(move |c| {
            c.execute("INSERT OR REPLACE INTO wh_missions VALUES(?1,?2,?3,?4,?5)", params![wh, m.id, state, data, now_ms()])
        })
        .await?;
        Ok(())
    }

    /// Missioni non concluse (da riprendere all'avvio).
    pub async fn wh_load_missions(&self, wh: String) -> Result<Vec<crate::warehouse::Mission>> {
        let rows: Vec<String> = self
            .run(move |c| {
                let mut st = c.prepare("SELECT data FROM wh_missions WHERE wh=?1 AND state IN ('queued','sent','running') ORDER BY id")?;
                st.query_map(params![wh], |r| r.get(0))?.collect()
            })
            .await?;
        Ok(rows.into_iter().filter_map(|d| serde_json::from_str(&d).ok()).collect())
    }

    pub async fn wh_recent_missions(&self, wh: String, limit: u32) -> Result<Vec<crate::warehouse::Mission>> {
        let rows: Vec<String> = self
            .run(move |c| {
                let mut st = c.prepare("SELECT data FROM wh_missions WHERE wh=?1 ORDER BY id DESC LIMIT ?2")?;
                st.query_map(params![wh, limit], |r| r.get(0))?.collect()
            })
            .await?;
        Ok(rows.into_iter().filter_map(|d| serde_json::from_str(&d).ok()).collect())
    }

    /// Salva (Some) o cancella (None) i dati di una UdC.
    pub async fn wh_save_lu(&self, wh: String, lu: String, data: Option<String>) -> Result<()> {
        self.run(move |c| match data {
            Some(d) => c.execute("INSERT OR REPLACE INTO wh_lus VALUES(?1,?2,?3)", params![wh, lu, d]),
            None => c.execute("DELETE FROM wh_lus WHERE wh=?1 AND lu=?2", params![wh, lu]),
        })
        .await?;
        Ok(())
    }

    pub async fn wh_save_lus(&self, wh: String, rows: Vec<(String, String)>) -> Result<()> {
        self.run(move |c| {
            let tx = c.unchecked_transaction()?;
            {
                let mut st = tx.prepare("INSERT OR REPLACE INTO wh_lus VALUES(?1,?2,?3)")?;
                for (lu, d) in rows {
                    st.execute(params![wh, lu, d])?;
                }
            }
            tx.commit()
        })
        .await
    }

    pub async fn wh_load_lus(&self, wh: String) -> Result<Vec<(String, String)>> {
        self.run(move |c| {
            let mut st = c.prepare("SELECT lu, data FROM wh_lus WHERE wh=?1")?;
            st.query_map(params![wh], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
        })
        .await
    }

    pub async fn wh_save_order(&self, wh: String, id: u32, state: String, data: String) -> Result<()> {
        self.run(move |c| c.execute("INSERT OR REPLACE INTO wh_orders VALUES(?1,?2,?3,?4,?5)", params![wh, id, state, data, now_ms()])).await?;
        Ok(())
    }

    /// Ordini aperti + ultimi chiusi.
    pub async fn wh_load_orders(&self, wh: String, closed_limit: u32) -> Result<Vec<String>> {
        self.run(move |c| {
            let mut st = c.prepare(
                "SELECT data FROM wh_orders WHERE wh=?1 AND state NOT IN ('done','cancelled','partial')
                 UNION ALL SELECT data FROM (SELECT data FROM wh_orders WHERE wh=?1 AND state IN ('done','cancelled','partial') ORDER BY id DESC LIMIT ?2)",
            )?;
            st.query_map(params![wh, closed_limit], |r| r.get(0))?.collect()
        })
        .await
    }

    pub async fn wh_max_order(&self, wh: String) -> Result<u32> {
        self.run(move |c| c.query_row("SELECT COALESCE(MAX(id),0) FROM wh_orders WHERE wh=?1", params![wh], |r| r.get(0))).await
    }

    pub async fn wh_add_picks(&self, wh: String, rows: Vec<(String, i64)>) -> Result<()> {
        self.run(move |c| {
            let tx = c.unchecked_transaction()?;
            {
                let mut st = tx.prepare("INSERT INTO wh_picks VALUES(?1,?2,?3)")?;
                for (sku, ts) in rows {
                    st.execute(params![wh, sku, ts])?;
                }
            }
            tx.commit()
        })
        .await
    }

    pub async fn wh_load_picks(&self, wh: String, since: i64) -> Result<Vec<(String, i64)>> {
        self.run(move |c| {
            let mut st = c.prepare("SELECT sku, ts FROM wh_picks WHERE wh=?1 AND ts>=?2")?;
            st.query_map(params![wh, since], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
        })
        .await
    }

    /// Tabelle chiave/valore (sicurezza, ricette, esecuzioni delle sequenze).
    pub async fn kv_load(&self, table: &'static str) -> Result<Vec<(String, String)>> {
        self.run(move |c| {
            let mut st = c.prepare(&format!("SELECT k, data FROM {table}"))?;
            st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
        })
        .await
    }

    pub async fn kv_put(&self, table: &'static str, key: String, data: Option<String>) -> Result<()> {
        self.run(move |c| match data {
            Some(d) => c.execute(&format!("INSERT OR REPLACE INTO {table} VALUES(?1,?2)"), params![key, d]),
            None => c.execute(&format!("DELETE FROM {table} WHERE k=?1"), params![key]),
        })
        .await?;
        Ok(())
    }

    /// Ultime N righe di una tabella chiave/valore con chiave ordinabile (es. `seq/timestamp`).
    pub async fn kv_recent(&self, table: &'static str, prefix: String, limit: u32) -> Result<Vec<(String, String)>> {
        self.run(move |c| {
            let mut st = c.prepare(&format!("SELECT k, data FROM {table} WHERE k LIKE ?1 ORDER BY k DESC LIMIT ?2"))?;
            st.query_map(params![format!("{prefix}%"), limit], |r| Ok((r.get(0)?, r.get(1)?)))?.collect()
        })
        .await
    }

    pub async fn wh_max_mission(&self, wh: String) -> Result<u32> {
        self.run(move |c| c.query_row("SELECT COALESCE(MAX(id),0) FROM wh_missions WHERE wh=?1", params![wh], |r| r.get(0))).await
    }

    /// Pulizia dati oltre la retention (giorni).
    pub async fn purge(&self, before_ts: i64) -> Result<()> {
        self.run(move |c| {
            c.execute("DELETE FROM history WHERE ts < ?1", params![before_ts])?;
            c.execute("DELETE FROM wh_missions WHERE ts < ?1 AND state IN ('done','cancelled')", params![before_ts])?;
            c.execute("DELETE FROM wh_orders WHERE ts < ?1 AND state IN ('done','cancelled','partial')", params![before_ts])?;
            c.execute("DELETE FROM wh_picks WHERE ts < ?1", params![before_ts])?;
            c.execute("DELETE FROM decisions WHERE ts < ?1", params![before_ts])
        })
        .await?;
        Ok(())
    }
}
