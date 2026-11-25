//! Server HTTP: API REST, WebSocket live, file statici HMI/Designer.

use crate::app::{AppState, Event};
use crate::decision::{apply_script_writes, json_to_value, tags_ctx, value_to_json};
use crate::config::TagDef;
use crate::tagdb::now_ms;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value as J, json};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tower_http::services::ServeDir;

type S = State<Arc<AppState>>;

struct ApiError(StatusCode, String);
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error": self.1}))).into_response()
    }
}
impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError(StatusCode::BAD_REQUEST, format!("{e:#}"))
    }
}
impl From<std::io::Error> for ApiError {
    fn from(e: std::io::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    }
}
type R<T> = Result<T, ApiError>;

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

pub fn router(app: Arc<AppState>) -> Router {
    let web_root = app.cfg().resolve(&app.cfg().server.web_root);
    Router::new()
        .route("/api/info", get(info))
        .route("/api/tags", get(tags).post(create_tag))
        .route("/api/tags/write", post(write_tag))
        .route("/api/tags/def/{name}", axum::routing::put(update_tag).delete(delete_tag))
        .route("/api/project", get(project_get).put(project_put))
        .route("/api/project/check", post(project_check))
        .route("/api/project/udt_preview", post(udt_preview))
        .route("/api/areas", get(areas_summary))
        .route("/api/sorting/status", get(sorting_status))
        .route("/api/sorting/lookup", get(sorting_lookup))
        .route("/api/sorting/sync", post(sorting_sync))
        .route("/api/drivers", get(drivers_list))
        .route("/api/rules", get(rules_list))
        .route("/api/rules/{point}", get(rules_get).put(rules_put))
        .route("/api/rules/{point}/test", post(rules_test))
        .route("/api/rules/{point}/reset", post(rules_reset))
        .route("/api/alarms", get(alarms))
        .route("/api/alarms/ack", post(ack))
        .route("/api/alarms/history", get(alarm_history))
        .route("/api/decisions", get(decisions))
        .route("/api/trend", get(trend))
        .route("/api/views", get(list_views))
        .route("/api/views/{id}", get(get_view).put(put_view).delete(del_view))
        .route("/api/symbols", get(list_symbols))
        .route("/api/symbols/{id}", get(get_symbol).put(put_symbol).delete(del_symbol))
        .route("/api/scripts", get(script_status))
        .route("/api/scripts/reload", post(script_reload))
        .route("/api/scripts/source", get(script_source).put(put_script_source))
        .route("/api/scripts/call", post(script_call))
        .route("/ws", get(ws))
        .fallback_service(ServeDir::new(web_root).append_index_html_on_directories(true))
        .layer(DefaultBodyLimit::max(4 * 1024 * 1024))
        .with_state(app)
}

fn require_designer(app: &AppState) -> R<()> {
    if app.cfg().server.designer { Ok(()) } else { Err(ApiError(StatusCode::FORBIDDEN, "designer disabilitato".into())) }
}

/// Id sicuro per file (viste, simboli): niente path traversal.
fn safe_id(id: &str) -> R<()> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok { Ok(()) } else { Err(bad(format!("id non valido: '{id}' (ammessi a-z 0-9 _ -)"))) }
}

// ---------------------------------------------------------------- info / tag

async fn info(State(app): S) -> Json<J> {
    let drivers: Vec<_> = app.board.read().unwrap().values().cloned().collect();
    Json(json!({
        "plant": app.cfg().plant,
        "drivers": drivers,
        "scripts": app.scripts.status(),
        "designer": app.cfg().server.designer,
        "decision_points": app.cfg().decision_points.iter().map(|d| json!({
            "name": d.name, "description": d.description, "function": d.function,
            "request_tag": d.request_tag, "response_tag": d.response_tag,
            "inputs": d.inputs, "outputs": d.outputs,
        })).collect::<Vec<_>>(),
    }))
}

async fn tags(State(app): S) -> Json<J> {
    let snap = app.db.snapshot();
    let cfg = app.cfg();
    let editable: std::collections::HashSet<&str> = cfg.project.tags.iter().map(|t| t.name.as_str()).collect();
    let list: Vec<J> = app
        .db
        .defs()
        .iter()
        .map(|t| {
            let st = snap.get(&t.name);
            let mut j = serde_json::to_value(t).unwrap_or(J::Null);
            j["len"] = json!(t.len);
            j["value"] = st.map(|s| value_to_json(&s.value)).unwrap_or(J::Null);
            j["quality"] = json!(st.map(|s| s.quality));
            j["ts"] = json!(st.map(|s| s.ts));
            j["editable"] = json!(editable.contains(t.name.as_str()));
            j
        })
        .collect();
    Json(J::Array(list))
}

// ---------------------------------------------------------------- gestione tag (Designer)

async fn drivers_list(State(app): S) -> Json<J> {
    Json(json!(app.cfg().drivers.iter().map(|d| json!({"name": d.name, "protocol": d.protocol})).collect::<Vec<_>>()))
}

async fn create_tag(State(app): S, Json(t): Json<TagDef>) -> R<Json<J>> {
    require_designer(&app)?;
    let _g = app.edit_lock.lock().await;
    let mut p = app.cfg().project.clone();
    let name = t.name.clone();
    p.tags.push(t);
    app.apply_project(p)?;
    app.audit("designer", "create_tag", name);
    Ok(Json(json!({"ok": true})))
}

async fn update_tag(State(app): S, Path(name): Path<String>, Json(t): Json<TagDef>) -> R<Json<J>> {
    require_designer(&app)?;
    let _g = app.edit_lock.lock().await;
    let mut p = app.cfg().project.clone();
    let Some(i) = p.tags.iter().position(|x| x.name == name) else {
        return Err(bad(format!("il tag '{name}' non è un tag di progetto modificabile (UDT o file di impianto)")));
    };
    p.tags[i] = t;
    app.apply_project(p)?;
    app.audit("designer", "update_tag", name);
    Ok(Json(json!({"ok": true})))
}

async fn delete_tag(State(app): S, Path(name): Path<String>) -> R<Json<J>> {
    require_designer(&app)?;
    let _g = app.edit_lock.lock().await;
    let mut p = app.cfg().project.clone();
    let n = p.tags.len();
    p.tags.retain(|x| x.name != name);
    if p.tags.len() == n {
        return Err(bad(format!("il tag '{name}' non è un tag di progetto modificabile")));
    }
    app.apply_project(p)?;
    app.audit("designer", "delete_tag", name);
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------- progetto

async fn project_get(State(app): S) -> Json<J> {
    let cfg = app.cfg();
    Json(json!({
        "project": cfg.project,
        "drivers": cfg.drivers.iter().map(|d| json!({"name": d.name, "protocol": d.protocol, "area": d.area})).collect::<Vec<_>>(),
        "static": {
            "areas": cfg.areas.iter().filter(|a| !cfg.project.areas.iter().any(|x| x.name == a.name)).map(|a| &a.name).collect::<Vec<_>>(),
            "scan_classes": cfg.scan_classes.iter().filter(|a| !cfg.project.scan_classes.iter().any(|x| x.name == a.name)).map(|a| &a.name).collect::<Vec<_>>(),
        },
        "functions": app.scripts.status().functions,
    }))
}

async fn project_put(State(app): S, Json(p): Json<crate::config::Project>) -> R<Json<J>> {
    require_designer(&app)?;
    let _g = app.edit_lock.lock().await;
    app.apply_project(p)?;
    app.audit("designer", "save_project", String::new());
    let cfg = app.cfg();
    Ok(Json(json!({"ok": true, "tags": cfg.tags.len()})))
}

/// Verifica un progetto senza salvarlo (errori di validazione e indirizzi).
async fn project_check(State(app): S, Json(p): Json<crate::config::Project>) -> Json<J> {
    match app.check_project(p) {
        Ok(c) => Json(json!({"ok": true, "tags": c.tags.len(), "decision_points": c.decision_points.iter().map(|d| json!({
            "name": d.name, "request_tag": d.request_tag, "response_tag": d.response_tag, "inputs": d.inputs, "outputs": d.outputs
        })).collect::<Vec<_>>()})),
        Err(e) => Json(json!({"ok": false, "error": format!("{e:#}")})),
    }
}

#[derive(Deserialize)]
struct UdtPreview {
    #[serde(rename = "type")]
    udt: crate::config::UdtType,
    instance: crate::config::UdtInstance,
}

/// Anteprima dei tag generati da un'istanza UDT (indirizzi risolti).
async fn udt_preview(State(app): S, Json(r): Json<UdtPreview>) -> Json<J> {
    match r.udt.expand(&r.instance) {
        Ok(tags) => {
            let cfg = app.cfg();
            let check = cfg.drivers.iter().find(|d| d.name == r.instance.driver).map(|d| crate::drivers::create(d, &tags));
            let err = match check {
                Some(Err(e)) => Some(format!("{e:#}")),
                None => Some(format!("driver '{}' sconosciuto", r.instance.driver)),
                _ => None,
            };
            Json(json!({"ok": err.is_none(), "error": err, "tags": tags}))
        }
        Err(e) => Json(json!({"ok": false, "error": format!("{e:#}"), "tags": []})),
    }
}

async fn areas_summary(State(app): S) -> Json<J> {
    Json(json!(app.alarms.summary(&app)))
}

async fn sorting_status(State(app): S) -> Json<J> {
    Json(json!({"wms": app.sorting.status()}))
}

#[derive(Deserialize)]
struct LookupQ {
    barcode: String,
    #[serde(default)]
    wms: bool,
}

/// Classi di appartenenza di un barcode (e risposta WMS se richiesta).
async fn sorting_lookup(State(app): S, Query(q): Query<LookupQ>) -> Json<J> {
    let wms = if q.wms {
        Some(match app.sorting.wms_lookup(&q.barcode).await {
            Ok(a) => json!(a),
            Err(e) => json!({"error": format!("{e:#}")}),
        })
    } else {
        None
    };
    Json(json!({
        "barcode": q.barcode,
        "classes": app.sorting.classes_of(&q.barcode),
        "class": app.sorting.classify(&q.barcode),
        "wms": wms,
    }))
}

async fn sorting_sync(State(app): S) -> R<Json<J>> {
    let n = app.sorting.sync_classes().await?;
    Ok(Json(json!({"synced": n, "status": app.sorting.status()})))
}

// ---------------------------------------------------------------- regole di smistamento

fn point(app: &AppState, name: &str) -> R<crate::config::DecisionPointDef> {
    app.cfg()
        .decision_points
        .iter()
        .find(|d| d.name == name)
        .cloned()
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("punto di decisione {name} non trovato")))
}

async fn rules_list(State(app): S) -> Json<J> {
    Json(json!(app.cfg().decision_points.iter().map(|d| json!({
        "name": d.name, "description": d.description, "function": d.function,
        "inputs": d.inputs, "outputs": d.outputs, "fallback": d.fallback,
        "rules": app.rules.get(&d.name).map(|r| r.rules.len()).unwrap_or(0),
    })).collect::<Vec<_>>()))
}

async fn rules_get(State(app): S, Path(p): Path<String>) -> R<Json<J>> {
    let dp = point(&app, &p)?;
    Ok(Json(json!({
        "point": {"name": dp.name, "description": dp.description, "function": dp.function,
                  "inputs": dp.inputs, "outputs": dp.outputs, "fallback": dp.fallback},
        "ruleset": app.rules.get(&p).unwrap_or_default(),
        "hits": app.rules.hits(&p),
        "functions": app.scripts.status().functions,
    })))
}

async fn rules_put(State(app): S, Path(p): Path<String>, Json(set): Json<crate::rules::RuleSet>) -> R<Json<J>> {
    require_designer(&app)?;
    let dp = point(&app, &p)?;
    let cfg = app.cfg();
    let classes = |c: &str| cfg.sort_classes.iter().any(|x| x.name == c);
    app.rules.save(&dp, set, &crate::rules::Checker { tag: &|t| app.db.def(t).is_some(), class: &classes })?;
    app.audit("designer", "save_rules", p);
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct RuleTest {
    #[serde(default)]
    inputs: HashMap<String, J>,
    /// Regole non ancora salvate da provare (opzionale).
    #[serde(default)]
    ruleset: Option<crate::rules::RuleSet>,
}

/// Prova le regole con input di esempio, senza toccare contatori né PLC.
async fn rules_test(State(app): S, Path(p): Path<String>, Json(t): Json<RuleTest>) -> R<Json<J>> {
    let dp = point(&app, &p)?;
    let inputs: HashMap<String, crate::value::Value> = t.inputs.iter().map(|(k, v)| (k.clone(), json_to_value(v))).collect();
    let (outcome, trace) = match t.ruleset {
        Some(set) => {
            // motore temporaneo in memoria per provare la bozza
            let dir = std::env::temp_dir().join(format!("wcs_rules_try_{}", std::process::id()));
            let eng = crate::rules::RulesEngine::load(dir, &[])?;
            let cfg = app.cfg();
            let classes = |c: &str| cfg.sort_classes.iter().any(|x| x.name == c);
            eng.save(&dp, set, &crate::rules::Checker { tag: &|n| app.db.def(n).is_some(), class: &classes })?;
            crate::decision::evaluate_rules_on(&app, &eng, &p, &inputs, true).await
        }
        None => crate::decision::evaluate_rules(&app, &p, &inputs, true).await,
    };
    let classes = inputs.values().map(|v| v.to_string()).flat_map(|v| app.sorting.classes_of(&v)).collect::<Vec<_>>();
    Ok(Json(json!({"outcome": outcome, "trace": trace, "function": dp.function, "fallback": dp.fallback, "classes": classes})))
}

async fn rules_reset(State(app): S, Path(p): Path<String>) -> R<Json<J>> {
    require_designer(&app)?;
    app.rules.reset_hits(&p);
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct WriteReq {
    tag: String,
    value: J,
    #[serde(default)]
    user: Option<String>,
}

async fn write_tag(State(app): S, Json(r): Json<WriteReq>) -> R<Json<J>> {
    let user = r.user.unwrap_or_else(|| "hmi".into());
    app.db.write(&r.tag, json_to_value(&r.value)).await?;
    app.audit(&user, "write", format!("{}={}", r.tag, r.value));
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------- allarmi / log

async fn alarms(State(app): S) -> Json<J> {
    Json(json!(app.alarms.list()))
}

#[derive(Deserialize)]
struct AckReq {
    id: Option<String>,
    #[serde(default)]
    area: Option<String>,
    user: Option<String>,
}

async fn ack(State(app): S, Json(r): Json<AckReq>) -> Json<J> {
    let n = app.alarms.ack(&app, r.id.as_deref(), r.area.as_deref(), r.user.as_deref().unwrap_or("hmi"));
    Json(json!({"acked": n}))
}

#[derive(Deserialize)]
struct LimitQ {
    limit: Option<u32>,
    point: Option<String>,
}

async fn alarm_history(State(app): S, Query(q): Query<LimitQ>) -> R<Json<J>> {
    Ok(Json(json!(app.store.alarm_history(q.limit.unwrap_or(200)).await?)))
}

async fn decisions(State(app): S, Query(q): Query<LimitQ>) -> R<Json<J>> {
    Ok(Json(json!(app.store.decisions(q.point, q.limit.unwrap_or(100).min(5000)).await?)))
}

#[derive(Deserialize)]
struct TrendQ {
    tag: String,
    from: Option<i64>,
    to: Option<i64>,
}

async fn trend(State(app): S, Query(q): Query<TrendQ>) -> R<Json<J>> {
    let to = q.to.unwrap_or_else(now_ms);
    let from = q.from.unwrap_or(to - 3_600_000);
    Ok(Json(json!(app.store.trend(q.tag, from, to).await?)))
}

// ---------------------------------------------------------------- viste

async fn list_views(State(app): S) -> R<Json<J>> {
    let mut out = vec![];
    for e in std::fs::read_dir(&app.views_dir)? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "json") {
            let id = p.file_stem().unwrap().to_string_lossy().to_string();
            let v: J = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(J::Null);
            out.push(json!({"id": id, "title": v.get("title").cloned().unwrap_or(json!(id)),
                            "home": v.get("home").cloned().unwrap_or(json!(false))}));
        }
    }
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Json(J::Array(out)))
}

async fn get_view(State(app): S, Path(id): Path<String>) -> R<Response> {
    safe_id(&id)?;
    let p = app.views_dir.join(format!("{id}.json"));
    let s = std::fs::read_to_string(&p).map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("vista {id} non trovata")))?;
    Ok(([(header::CONTENT_TYPE, "application/json")], s).into_response())
}

/// Raccoglie tutti i riferimenti a tag in una vista (chiavi "tag").
fn collect_tag_refs(v: &J, out: &mut Vec<String>) {
    match v {
        J::Object(m) => {
            for (k, x) in m {
                if k == "tag"
                    && let Some(s) = x.as_str() {
                        out.push(s.to_string());
                    }
                collect_tag_refs(x, out);
            }
        }
        J::Array(a) => a.iter().for_each(|x| collect_tag_refs(x, out)),
        _ => {}
    }
}

async fn put_view(State(app): S, Path(id): Path<String>, Json(view): Json<J>) -> R<Json<J>> {
    require_designer(&app)?;
    safe_id(&id)?;
    if !view.get("components").is_some_and(J::is_array) {
        return Err(bad("la vista deve avere un array 'components'"));
    }
    let mut refs = vec![];
    collect_tag_refs(&view, &mut refs);
    let mut warnings: Vec<String> = refs
        .into_iter()
        .filter(|t| !t.is_empty() && !t.contains("{") && app.db.def(t).is_none())
        .map(|t| format!("tag sconosciuto: {t}"))
        .collect();
    warnings.dedup();
    let p = app.views_dir.join(format!("{id}.json"));
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&view).unwrap())?;
    std::fs::rename(tmp, p)?;
    app.audit("designer", "save_view", id.clone());
    Ok(Json(json!({"ok": true, "warnings": warnings})))
}

async fn del_view(State(app): S, Path(id): Path<String>) -> R<Json<J>> {
    require_designer(&app)?;
    safe_id(&id)?;
    std::fs::remove_file(app.views_dir.join(format!("{id}.json")))?;
    app.audit("designer", "delete_view", id);
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------- simboli SVG

async fn list_symbols(State(app): S) -> R<Json<J>> {
    let mut out = vec![];
    for e in std::fs::read_dir(&app.symbols_dir)? {
        let p = e?.path();
        if p.extension().is_some_and(|x| x == "svg") {
            let id = p.file_stem().unwrap().to_string_lossy().to_string();
            let svg = std::fs::read_to_string(&p).unwrap_or_default();
            out.push(json!({"id": id, "svg": svg}));
        }
    }
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Json(J::Array(out)))
}

async fn get_symbol(State(app): S, Path(id): Path<String>) -> R<Response> {
    safe_id(&id)?;
    let s = std::fs::read_to_string(app.symbols_dir.join(format!("{id}.svg")))
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "simbolo non trovato".into()))?;
    Ok(([(header::CONTENT_TYPE, "image/svg+xml")], s).into_response())
}

/// Controllo di sicurezza sugli SVG importati: niente script, handler di
/// eventi, foreignObject o riferimenti esterni (l'editor sanifica già lato
/// client; questo è il controllo autorevole lato server).
pub fn check_svg(svg: &str) -> Result<(), String> {
    let l = svg.to_ascii_lowercase();
    if !l.contains("<svg") {
        return Err("non è un file SVG".into());
    }
    for bad in ["<script", "javascript:", "<foreignobject", "<iframe", "<!entity", "<embed", "<object"] {
        if l.contains(bad) {
            return Err(format!("contenuto non ammesso: {bad}"));
        }
    }
    // attributi on*= (onclick, onload, ...)
    let b = l.as_bytes();
    for i in 1..b.len().saturating_sub(3) {
        if (b[i - 1].is_ascii_whitespace() || b[i - 1] == b'"' || b[i - 1] == b'\'' || b[i - 1] == b'/')
            && b[i] == b'o'
            && b[i + 1] == b'n'
        {
            let mut j = i + 2;
            while j < b.len() && b[j].is_ascii_alphabetic() {
                j += 1;
            }
            while j < b.len() && b[j].is_ascii_whitespace() {
                j += 1;
            }
            if j > i + 2 && j < b.len() && b[j] == b'=' {
                return Err("attributi evento (on*) non ammessi".into());
            }
        }
    }
    for pat in ["href=\"http", "href='http", "href=\"//", "url(http", "url(//", "@import"] {
        if l.replace(' ', "").contains(pat) {
            return Err("riferimenti esterni non ammessi".into());
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct SymbolBody {
    svg: String,
}

async fn put_symbol(State(app): S, Path(id): Path<String>, Json(b): Json<SymbolBody>) -> R<Json<J>> {
    require_designer(&app)?;
    safe_id(&id)?;
    check_svg(&b.svg).map_err(bad)?;
    std::fs::write(app.symbols_dir.join(format!("{id}.svg")), b.svg)?;
    app.audit("designer", "save_symbol", id);
    Ok(Json(json!({"ok": true})))
}

async fn del_symbol(State(app): S, Path(id): Path<String>) -> R<Json<J>> {
    require_designer(&app)?;
    safe_id(&id)?;
    std::fs::remove_file(app.symbols_dir.join(format!("{id}.svg")))?;
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------- script

async fn script_status(State(app): S) -> Json<J> {
    Json(json!(app.scripts.status()))
}

async fn script_reload(State(app): S) -> R<Json<J>> {
    require_designer(&app)?;
    let st = app.reload_scripts().await?;
    app.audit("designer", "reload_scripts", String::new());
    Ok(Json(json!(st)))
}

async fn script_source(State(app): S) -> R<Json<J>> {
    let p = app.script_path.as_ref().ok_or_else(|| bad("nessun modulo script configurato"))?;
    Ok(Json(json!({"path": p.display().to_string(), "source": std::fs::read_to_string(p).unwrap_or_default()})))
}

#[derive(Deserialize)]
struct SourceBody {
    source: String,
}

/// Salva il modulo Python e lo ricarica. Prima verifica la sintassi con
/// py_compile: un errore non sostituisce il file in produzione.
async fn put_script_source(State(app): S, Json(b): Json<SourceBody>) -> R<Json<J>> {
    require_designer(&app)?;
    let p = app.script_path.clone().ok_or_else(|| bad("nessun modulo script configurato"))?;
    let tmp = p.with_extension("py.check");
    std::fs::write(&tmp, &b.source)?;
    let out = tokio::process::Command::new(&app.cfg().scripting.python)
        .args(["-c", "import sys,py_compile; py_compile.compile(sys.argv[1], doraise=True)"])
        .arg(&tmp)
        .output()
        .await?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(bad(format!("errore di sintassi:\n{}", String::from_utf8_lossy(&out.stderr))));
    }
    // backup della versione precedente
    if p.exists() {
        let _ = std::fs::copy(&p, p.with_extension("py.bak"));
    }
    std::fs::rename(&tmp, &p)?;
    let st = app.reload_scripts().await?;
    app.audit("designer", "save_script", p.display().to_string());
    Ok(Json(json!(st)))
}

#[derive(Deserialize)]
struct CallReq {
    #[serde(rename = "fn")]
    func: String,
    #[serde(default)]
    payload: J,
    #[serde(default)]
    source: String,
    #[serde(default)]
    user: Option<String>,
}

/// Esegue una funzione Python su evento HMI (es. click su pulsante).
async fn script_call(State(app): S, Json(r): Json<CallReq>) -> R<Json<J>> {
    let user = r.user.unwrap_or_else(|| "hmi".into());
    let ctx = json!({"tags": tags_ctx(&app), "point": r.source, "user": user});
    let rep = app.hmi_scripts.call(&r.func, r.payload, ctx, Duration::from_secs(5)).await?;
    let errors = apply_script_writes(&app, &rep.writes, &format!("hmi:{}", r.source)).await;
    for l in &rep.logs {
        app.log("info", &format!("script:{}", r.func), l.clone());
    }
    app.audit(&user, "script_call", format!("{} ({})", r.func, r.source));
    Ok(Json(json!({"result": rep.result, "logs": rep.logs, "write_errors": errors})))
}

// ---------------------------------------------------------------- websocket

async fn ws(State(app): S, up: WebSocketUpgrade) -> Response {
    up.on_upgrade(move |s| ws_session(app, s))
}

/// Invia snapshot iniziale e poi aggiornamenti tag a lotti (ogni 50 ms) + eventi.
async fn ws_session(app: Arc<AppState>, mut sock: WebSocket) {
    let mut tag_rx = app.db.subscribe();
    let mut ev_rx = app.events.subscribe();
    let snap: HashMap<_, _> = app.db.snapshot();
    let init = json!({"type": "snapshot", "tags": snap, "alarms": app.alarms.list()});
    if sock.send(Message::Text(init.to_string().into())).await.is_err() {
        return;
    }
    let mut batch: serde_json::Map<String, J> = Default::default();
    let mut flush = tokio::time::interval(Duration::from_millis(50));
    let mut status = tokio::time::interval(Duration::from_secs(2));
    loop {
        tokio::select! {
            r = tag_rx.recv() => match r {
                Ok(u) => { batch.insert(u.name.clone(), json!(u.state)); }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    for (k, v) in app.db.snapshot() { batch.insert(k, json!(v)); }
                }
                Err(_) => return,
            },
            r = ev_rx.recv() => if let Ok(ev) = r {
                let msg = match &ev { Event::Project => json!({"type": "project"}), _ => json!(ev) };
                if sock.send(Message::Text(msg.to_string().into())).await.is_err() { return; }
            },
            _ = flush.tick() => if !batch.is_empty() {
                let msg = json!({"type": "tags", "tags": std::mem::take(&mut batch)});
                if sock.send(Message::Text(msg.to_string().into())).await.is_err() { return; }
            },
            _ = status.tick() => {
                let drivers: Vec<_> = app.board.read().unwrap().values().cloned().collect();
                let msg = json!({"type": "status", "data": {"drivers": drivers, "scripts": app.scripts.status()}});
                if sock.send(Message::Text(msg.to_string().into())).await.is_err() { return; }
            },
            m = sock.recv() => match m {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::check_svg;
    #[test]
    fn svg_checks() {
        assert!(check_svg(r#"<svg viewBox="0 0 10 10"><rect id="body" fill="red"/></svg>"#).is_ok());
        assert!(check_svg(r#"<svg><script>alert(1)</script></svg>"#).is_err());
        assert!(check_svg(r#"<svg onload="x()"></svg>"#).is_err());
        assert!(check_svg(r#"<svg><a href="javascript:x"/></svg>"#).is_err());
        assert!(check_svg(r#"<svg><image href="http://evil/x.png"/></svg>"#).is_err());
        // "one" / "online" come testo o id non sono attributi evento
        assert!(check_svg(r#"<svg><g id="motor_one"/><text>online</text></svg>"#).is_ok());
    }
}
