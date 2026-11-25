//! Decision engine: gestisce le "domande" del PLC con handshake.
//!
//! Protocollo (per ogni punto di decisione):
//! 1. Il PLC scrive i dati (barcode, peso, ...) e poi incrementa `request_tag`
//!    (ReqID, ≠ 0). I dati e il ReqID devono stare in un'area contigua così
//!    vengono letti nello stesso blocco (lettura consistente).
//! 2. Il WCS rileva ReqID ≠ RespID, valuta le regole grafiche (poi l'eventuale
//!    funzione Python, poi il fallback), scrive le uscite e per ultimo
//!    `response_tag` = ReqID.
//! 3. Il PLC usa le uscite solo quando RespID == ReqID; se non arriva entro il
//!    suo timeout applica una destinazione di default (fail-safe lato PLC).
//!
//! Variante "flag": il PLC alza Req, il WCS risponde e alza Ack; il PLC
//! abbassa Req e il WCS abbassa Ack (fine ciclo).

use crate::app::{AppState, Event};
use crate::config::{DecisionPointDef, Handshake};
use crate::store::DecisionRecord;
use crate::tagdb::{Quality, now_ms};
use crate::value::Value;
use anyhow::{Result, anyhow};
use serde_json::{Map, Value as Json, json};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::error::RecvError;

pub fn value_to_json(v: &Value) -> Json {
    serde_json::to_value(v).unwrap_or(Json::Null)
}

pub fn json_to_value(j: &Json) -> Value {
    serde_json::from_value(j.clone()).unwrap_or(Value::Null)
}

/// Snapshot dei tag per il contesto degli script.
pub fn tags_ctx(app: &AppState) -> Json {
    let m: Map<String, Json> = app.db.snapshot().into_iter().map(|(k, s)| (k, value_to_json(&s.value))).collect();
    Json::Object(m)
}

/// Applica le scritture richieste da uno script con ctx.write().
pub async fn apply_script_writes(app: &AppState, writes: &[(String, Json)], source: &str) -> Vec<String> {
    apply_writes(app, writes, source, true).await
}

/// Scritture richieste da uno script; `audit` = registra ogni scrittura (no per i timer).
pub async fn apply_writes(app: &AppState, writes: &[(String, Json)], source: &str, audit: bool) -> Vec<String> {
    let mut errors = vec![];
    for (tag, v) in writes {
        if let Err(e) = app.db.write_ext(tag, json_to_value(v), !audit).await {
            errors.push(format!("{tag}: {e:#}"));
        } else if audit {
            app.audit("script", "write", format!("{source}: {tag}={v}"));
        }
    }
    errors
}

pub async fn run(app: Arc<AppState>, dp: DecisionPointDef) {
    let mut rx = app.db.subscribe();
    let mut last_handled: Option<i64> = None;
    let mut recheck = tokio::time::interval(Duration::from_millis(500));
    loop {
        tokio::select! {
            r = rx.recv() => match r {
                Ok(u) if u.name == dp.request_tag => {}
                Ok(_) => continue,
                Err(RecvError::Lagged(_)) => {}
                Err(RecvError::Closed) => return,
            },
            _ = recheck.tick() => {}
        }
        if dp.handshake == Handshake::Flag {
            // Il PLC ha ritirato la richiesta: azzera l'ack (fine ciclo handshake).
            let req = app.db.get(&dp.request_tag);
            let ack = app.db.get(&dp.response_tag);
            if let (Some(r), Some(a)) = (req, ack)
                && r.quality == Quality::Good && a.quality == Quality::Good
                    && r.value.as_bool() == Some(false) && a.value.as_bool() == Some(true)
                {
                    if let Err(e) = app.db.write(&dp.response_tag, Value::Bool(false)).await {
                        tracing::warn!(point = %dp.name, "reset ack fallito: {e:#}");
                    }
                    last_handled = None;
                }
        }
        let Some(req) = pending_request(&app, &dp) else { continue };
        if dp.handshake == Handshake::Counter && last_handled == Some(req) {
            continue;
        }
        match handle(&app, &dp, req).await {
            Ok(()) => last_handled = Some(req),
            Err(e) => {
                tracing::warn!(point = %dp.name, req, "risposta non inviata: {e:#}");
                app.log("warn", &dp.name, format!("richiesta {req}: risposta non inviata: {e:#}"));
            }
        }
    }
}

/// Ritorna il ReqID se c'è una richiesta in attesa di risposta.
fn pending_request(app: &AppState, dp: &DecisionPointDef) -> Option<i64> {
    let req = app.db.get(&dp.request_tag)?;
    let resp = app.db.get(&dp.response_tag)?;
    if req.quality != Quality::Good || resp.quality != Quality::Good {
        return None;
    }
    match dp.handshake {
        Handshake::Counter => {
            let r = req.value.as_i64()?;
            (r != 0 && Some(r) != resp.value.as_i64()).then_some(r)
        }
        // flag: richiesta = Req alto e Ack basso
        Handshake::Flag => (req.value.as_bool()? && !resp.value.as_bool()?).then_some(1),
    }
}

/// Uscite decise + scritture extra richieste dallo script.
type Decided = (BTreeMap<String, Value>, Vec<(String, Json)>);

/// Collega le regole a tag, classi di smistamento e WMS.
struct AppResolver<'a>(&'a AppState);

impl crate::rules::Resolver for AppResolver<'_> {
    fn tag(&self, name: &str) -> Value {
        self.0.db.value(name)
    }
    fn in_class(&self, value: &str, class: &str) -> bool {
        self.0.sorting.in_class(value, class)
    }
    fn class_dest(&self, value: &str) -> Option<String> {
        self.0.sorting.classify(value).map(|c| c.1).filter(|d| !d.is_empty())
    }
    fn wms_dest(&self, value: &str) -> Option<String> {
        self.0.sorting.wms_cached(value).and_then(|a| a.destination)
    }
}

/// Valuta le regole di un punto, interrogando prima il WMS se le regole lo usano.
pub async fn evaluate_rules(
    app: &AppState,
    point: &str,
    inputs: &std::collections::HashMap<String, Value>,
    dry_run: bool,
) -> (Option<crate::rules::Outcome>, Vec<crate::rules::RuleTrace>) {
    evaluate_rules_on(app, &app.rules, point, inputs, dry_run).await
}

/// Come `evaluate_rules` ma su un motore regole qualsiasi (bozze dal Designer).
pub async fn evaluate_rules_on(
    app: &AppState,
    engine: &crate::rules::RulesEngine,
    point: &str,
    inputs: &std::collections::HashMap<String, Value>,
    dry_run: bool,
) -> (Option<crate::rules::Outcome>, Vec<crate::rules::RuleTrace>) {
    if app.sorting.wms_enabled() {
        for f in engine.wms_fields(point) {
            if let Some(v) = inputs.get(&f) {
                let key = v.to_string();
                if !key.trim().is_empty()
                    && let Err(e) = app.sorting.wms_lookup(&key).await {
                        app.log("warn", point, format!("WMS non raggiungibile per {key}: {e:#}"));
                    }
            }
        }
    }
    engine.evaluate(point, inputs, &AppResolver(app), dry_run)
}

async fn handle(app: &AppState, dp: &DecisionPointDef, req: i64) -> Result<()> {
    let t0 = Instant::now();
    let inputs: Map<String, Json> =
        dp.inputs.iter().map(|(alias, tag)| (alias.clone(), value_to_json(&app.db.value(tag)))).collect();
    let mut payload = inputs.clone();
    payload.insert("request_id".into(), json!(req));
    // 1) regole grafiche
    let rule_inputs: std::collections::HashMap<String, Value> =
        payload.iter().map(|(k, v)| (k.clone(), json_to_value(v))).collect();
    let (outcome, _) = evaluate_rules(app, &dp.name, &rule_inputs, false).await;
    let mut rule_name: Option<String> = None;
    let mut rule_outputs: Option<BTreeMap<String, Value>> = None;
    let mut script_fn = dp.function.clone();
    if let Some(o) = outcome {
        if let Some(l) = &o.log {
            app.log("info", &dp.name, l.clone());
        }
        rule_name = Some(o.rule_name.clone());
        if o.script.is_some() {
            script_fn = o.script.clone();
        }
        if !o.outputs.is_empty() && o.script.is_none() {
            rule_outputs = Some(o.outputs);
        }
    }

    let result: Result<Decided> = if let Some(o) = rule_outputs {
        // uscite non specificate dalla regola -> fallback del punto
        complete(dp, o).map(|o| (o, vec![]))
    } else if let Some(func) = &script_fn {
        // 2) funzione Python (delegata da regola, o nessuna regola scattata)
        let ctx = json!({"tags": tags_ctx(app), "point": dp.name});
        match app.scripts.call(func, Json::Object(payload), ctx, Duration::from_millis(dp.timeout_ms)).await {
            Ok(r) => {
                for l in &r.logs {
                    app.log("info", &dp.name, l.clone());
                }
                validate(dp, &r.result).map(|o| (o, r.writes))
            }
            Err(e) => Err(e),
        }
    } else {
        Err(anyhow!("nessuna regola applicabile e nessuno script configurato"))
    };

    let (outputs, status, error, extra_writes) = match result {
        Ok((o, w)) => (o, "ok", None, w),
        Err(e) => {
            let msg = format!("{e:#}");
            tracing::warn!(point = %dp.name, req, "decisione fallita: {msg}");
            if dp.fallback.is_empty() {
                // Nessun fallback: non rispondiamo, il PLC andrà in timeout
                // e applicherà la propria logica di default.
                record(app, dp, req, inputs, BTreeMap::new(), t0, "error", Some(msg.clone()), rule_name).await;
                return Ok(());
            }
            (dp.fallback.clone(), "fallback", Some(msg), vec![])
        }
    };

    // Scrittura uscite, poi il RespID (ordine garantito: stesso driver = stessa coda).
    for (alias, v) in &outputs {
        let tag = &dp.outputs[alias];
        app.db.write(tag, v.clone()).await.map_err(|e| anyhow!("scrittura {tag}: {e:#}"))?;
    }
    let ack = match dp.handshake {
        Handshake::Counter => Value::Int(req),
        Handshake::Flag => Value::Bool(true),
    };
    app.db.write(&dp.response_tag, ack).await.map_err(|e| anyhow!("scrittura risposta: {e:#}"))?;
    for e in apply_script_writes(app, &extra_writes, &dp.name).await {
        app.log("warn", &dp.name, format!("scrittura script fallita {e}"));
    }
    record(app, dp, req, inputs, outputs, t0, status, error, rule_name).await;
    Ok(())
}

/// Controlla che lo script abbia restituito un dict con uscite note.
/// Le uscite mancanti prendono il valore di fallback, se definito.
fn validate(dp: &DecisionPointDef, res: &Json) -> Result<BTreeMap<String, Value>> {
    let obj = res.as_object().ok_or_else(|| anyhow!("lo script deve restituire un dict, ricevuto {res}"))?;
    let mut out = BTreeMap::new();
    for (k, v) in obj {
        if !dp.outputs.contains_key(k) {
            return Err(anyhow!("uscita sconosciuta '{k}' (attese: {:?})", dp.outputs.keys().collect::<Vec<_>>()));
        }
        out.insert(k.clone(), json_to_value(v));
    }
    complete(dp, out)
}

/// Completa le uscite mancanti con il fallback del punto.
fn complete(dp: &DecisionPointDef, mut out: BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>> {
    for k in dp.outputs.keys() {
        if !out.contains_key(k) {
            let fb = dp.fallback.get(k).ok_or_else(|| anyhow!("uscita '{k}' mancante e senza fallback"))?;
            out.insert(k.clone(), fb.clone());
        }
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
async fn record(
    app: &AppState,
    dp: &DecisionPointDef,
    req: i64,
    inputs: Map<String, Json>,
    outputs: BTreeMap<String, Value>,
    t0: Instant,
    status: &str,
    error: Option<String>,
    rule: Option<String>,
) {
    let rec = DecisionRecord {
        ts: now_ms(),
        point: dp.name.clone(),
        request_id: req,
        inputs: Json::Object(inputs),
        outputs: serde_json::to_value(&outputs).unwrap_or(Json::Null),
        duration_ms: t0.elapsed().as_secs_f64() * 1000.0,
        status: status.into(),
        error,
        rule,
    };
    tracing::info!(point = %rec.point, req, status, ms = format!("{:.1}", rec.duration_ms), "decisione {}", rec.outputs);
    let _ = app.events.send(Event::Decision(rec.clone()));
    if let Err(e) = app.store.log_decision(rec).await {
        tracing::error!("salvataggio decisione: {e:#}");
    }
}
