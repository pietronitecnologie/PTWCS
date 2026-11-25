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
        .route("/api/library", get(library))
        .route("/api/wh", get(wh_list))
        .route("/api/wh/{name}/summary", get(wh_summary))
        .route("/api/wh/{name}/side", get(wh_side))
        .route("/api/wh/{name}/missions", get(wh_missions))
        .route("/api/wh/{name}/outbound", post(wh_outbound))
        .route("/api/wh/{name}/inbound", post(wh_inbound))
        .route("/api/wh/{name}/location", get(wh_location).post(wh_location_set))
        .route("/api/wh/{name}/find", get(wh_find))
        .route("/api/wh/{name}/mission/{id}/{action}", post(wh_mission_action))
        .route("/api/wh/{name}/kpi", get(wh_kpi))
        .route("/api/wh/{name}/stock", get(wh_stock))
        .route("/api/wh/{name}/lus", get(wh_lus))
        .route("/api/wh/{name}/lu/{lu}", get(wh_lu_get).post(wh_lu_set))
        .route("/api/wh/{name}/orders", get(wh_orders).post(wh_order_create))
        .route("/api/wh/{name}/order/{id}/{action}", post(wh_order_action))
        .route("/api/wh/{name}/move", post(wh_move))
        .route("/api/wh/{name}/reorg", get(wh_reorg_plan).post(wh_reorg_set))
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
        .route("/api/scripts/files", get(script_files))
        .route("/api/scripts/file/{name}", get(script_file_get).put(script_file_put).delete(script_file_del))
        .route("/api/scripts/check", post(script_check))
        .route("/api/scripts/test", post(script_test))
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

// ---------------------------------------------------------------- magazzini automatici

fn wh(app: &AppState, name: &str) -> R<Arc<crate::warehouse::Warehouse>> {
    app.warehouse(name).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("magazzino {name} non trovato")))
}

async fn wh_list(State(app): S) -> Json<J> {
    let mut names = app.warehouse_names();
    names.sort();
    Json(J::Array(names.iter().filter_map(|n| app.warehouse(n)).map(|w| w.summary()).collect()))
}

async fn wh_summary(State(app): S, Path(name): Path<String>) -> R<Json<J>> {
    Ok(Json(wh(&app, &name)?.summary()))
}

#[derive(Deserialize)]
struct SideQ {
    aisle: u16,
    side: u16,
}

/// Prospetto di un lato di corsia: celle [livello][colonna] con stato e UdC.
async fn wh_side(State(app): S, Path(name): Path<String>, Query(q): Query<SideQ>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let cells = w.side_cells(q.aisle, q.side)?;
    let (zones, classes) = w.side_abc(q.aisle, q.side)?;
    let codes = |s: crate::warehouse::LocState| match s {
        crate::warehouse::LocState::Free => 0,
        crate::warehouse::LocState::Occupied => 1,
        crate::warehouse::LocState::Reserved => 2,
        crate::warehouse::LocState::Picking => 3,
        crate::warehouse::LocState::Blocked => 4,
    };
    Ok(Json(json!({
        "aisle": q.aisle, "side": q.side, "columns": w.def.columns, "levels": w.def.levels,
        "states": cells.iter().map(|r| r.iter().map(|c| codes(c.0)).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "lus": cells.iter().map(|r| r.iter().map(|c| c.1.clone()).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "zones": zones, "classes": classes,
    })))
}

async fn wh_missions(State(app): S, Path(name): Path<String>, Query(q): Query<LimitQ>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let open = w.missions();
    let recent = app.store.wh_recent_missions(name, q.limit.unwrap_or(30).min(500)).await?;
    Ok(Json(json!({"open": open, "recent": recent})))
}

#[derive(Deserialize)]
struct OutReq {
    #[serde(default)]
    lu: Option<String>,
    #[serde(default)]
    bay: Option<String>,
}

async fn wh_outbound(State(app): S, Path(name): Path<String>, Json(r): Json<OutReq>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let m = w.create_outbound(r.lu.as_deref().filter(|s| !s.is_empty()), r.bay.as_deref().filter(|s| !s.is_empty()), &w.unavailable_aisles(&app.db))?;
    app.audit("hmi", "wh_outbound", format!("{name}: {} da {} → {}", m.lu, m.src.code(), m.bay));
    Ok(Json(json!(m)))
}

#[derive(Deserialize)]
struct InReq {
    lu: String,
    #[serde(default)]
    bay: Option<String>,
    #[serde(default)]
    sku: String,
    #[serde(default)]
    qty: f64,
    #[serde(default)]
    lot: String,
    #[serde(default)]
    height_mm: u16,
    #[serde(default)]
    weight_kg: f64,
}

async fn wh_inbound(State(app): S, Path(name): Path<String>, Json(r): Json<InReq>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let info = crate::warehouse::LuInfo { sku: r.sku.trim().into(), qty: r.qty, lot: r.lot.trim().into(), height_mm: r.height_mm, weight_kg: r.weight_kg, entry: 0 };
    let (aisle, pos) = w.allocate_inbound_with(&r.lu, r.bay.as_deref().unwrap_or("manuale"), info, &w.unavailable_aisles(&app.db), false)?;
    app.audit("hmi", "wh_inbound", format!("{name}: {} → {}", r.lu, pos.code()));
    Ok(Json(json!({"aisle": aisle, "location": pos, "code": pos.code()})))
}

#[derive(Deserialize)]
struct LocQ {
    aisle: u16,
    side: u16,
    col: u16,
    lvl: u16,
    #[serde(default)]
    action: Option<String>,
}

async fn wh_location(State(app): S, Path(name): Path<String>, Query(q): Query<LocQ>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let p = crate::warehouse::Pos { aisle: q.aisle, side: q.side, col: q.col, lvl: q.lvl };
    Ok(Json(w.location_detail(p).ok_or_else(|| bad("locazione non valida"))?))
}

async fn wh_location_set(State(app): S, Path(name): Path<String>, Json(q): Json<LocQ>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let p = crate::warehouse::Pos { aisle: q.aisle, side: q.side, col: q.col, lvl: q.lvl };
    match q.action.as_deref() {
        Some("block") => w.set_blocked(p, true)?,
        Some("unblock") => w.set_blocked(p, false)?,
        _ => return Err(bad("azione: block | unblock")),
    }
    app.audit("hmi", "wh_location", format!("{name}: {} {:?}", p.code(), q.action));
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize)]
struct FindQ {
    lu: String,
}

async fn wh_find(State(app): S, Path(name): Path<String>, Query(q): Query<FindQ>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    Ok(Json(match w.find(&q.lu) {
        Some((p, l)) => json!({"found": true, "code": p.code(), "pos": p, "loc": l, "info": w.lu_info(&q.lu)}),
        None => json!({"found": false}),
    }))
}

#[derive(Deserialize, Default)]
struct PrioReq {
    #[serde(default)]
    priority: Option<u8>,
}

/// Azioni sulle missioni: hold, release, priority, cancel, requeue, done.
async fn wh_mission_action(State(app): S, Path((name, id, action)): Path<(String, u32, String)>, body: Option<Json<PrioReq>>) -> R<Json<J>> {
    let p = body.map(|b| b.0).unwrap_or_default();
    let m = wh(&app, &name)?.mission_action(id, &action, p.priority)?;
    app.audit("hmi", "wh_mission", format!("{name}: missione {id} {action}{}", p.priority.map(|p| format!(" {p}")).unwrap_or_default()));
    Ok(Json(json!(m)))
}

async fn wh_kpi(State(app): S, Path(name): Path<String>) -> R<Json<J>> {
    Ok(Json(wh(&app, &name)?.kpi()))
}

async fn wh_stock(State(app): S, Path(name): Path<String>) -> R<Json<J>> {
    Ok(Json(wh(&app, &name)?.stock()))
}

#[derive(Deserialize)]
struct LusQ {
    sku: Option<String>,
    q: Option<String>,
    limit: Option<usize>,
}

async fn wh_lus(State(app): S, Path(name): Path<String>, Query(q): Query<LusQ>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    Ok(Json(json!(w.lus(q.sku.as_deref().filter(|s| !s.is_empty()), q.q.as_deref().filter(|s| !s.is_empty()), q.limit.unwrap_or(200).min(5000)))))
}

async fn wh_lu_get(State(app): S, Path((name, lu)): Path<(String, String)>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let (p, l) = w.find(&lu).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("UdC {lu} non trovata")))?;
    Ok(Json(json!({"lu": lu, "code": p.code(), "pos": p, "loc": l, "info": w.lu_info(&lu)})))
}

async fn wh_lu_set(State(app): S, Path((name, lu)): Path<(String, String)>, Json(info): Json<crate::warehouse::LuInfo>) -> R<Json<J>> {
    let i = wh(&app, &name)?.set_lu_info(&lu, info)?;
    app.audit("hmi", "wh_lu", format!("{name}: rettifica {lu} → {} q.tà {}", i.sku, i.qty));
    Ok(Json(json!(i)))
}

async fn wh_orders(State(app): S, Path(name): Path<String>) -> R<Json<J>> {
    Ok(Json(json!(wh(&app, &name)?.orders())))
}

async fn wh_order_create(State(app): S, Path(name): Path<String>, Json(r): Json<crate::warehouse::OrderReq>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let o = w.create_order(r, &w.unavailable_aisles(&app.db))?;
    app.audit("hmi", "wh_order", format!("{name}: ordine {} ({}) {} righe", o.id, o.reference, o.lines.len()));
    Ok(Json(json!(o)))
}

async fn wh_order_action(State(app): S, Path((name, id, action)): Path<(String, u32, String)>, body: Option<Json<PrioReq>>) -> R<Json<J>> {
    let p = body.map(|b| b.0).unwrap_or_default();
    let o = wh(&app, &name)?.order_action(id, &action, p.priority)?;
    app.audit("hmi", "wh_order", format!("{name}: ordine {id} {action}"));
    Ok(Json(json!(o)))
}

#[derive(Deserialize)]
struct MoveReq {
    lu: String,
    #[serde(default)]
    dst: Option<crate::warehouse::Pos>,
}

async fn wh_move(State(app): S, Path(name): Path<String>, Json(r): Json<MoveReq>) -> R<Json<J>> {
    let m = wh(&app, &name)?.create_move(&r.lu, r.dst)?;
    app.audit("hmi", "wh_move", format!("{name}: {} {} → {}", m.lu, m.src.code(), m.dst.code()));
    Ok(Json(json!(m)))
}

#[derive(Deserialize)]
struct PlanQ {
    aisle: Option<u16>,
    max: Option<usize>,
}

/// Piano di riorganizzazione simulato.
async fn wh_reorg_plan(State(app): S, Path(name): Path<String>, Query(q): Query<PlanQ>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    let plan = w.plan(q.aisle.filter(|&a| a > 0), q.max.unwrap_or(50).min(2000));
    Ok(Json(json!({"plan": plan, "config": w.def.reorg, "enabled": w.summary()["reorg"]["enabled"]})))
}

#[derive(Deserialize)]
struct ReorgReq {
    #[serde(default)]
    enabled: Option<bool>,
    /// crea subito N movimentazioni dal piano
    #[serde(default)]
    run: Option<usize>,
    #[serde(default)]
    aisle: Option<u16>,
}

async fn wh_reorg_set(State(app): S, Path(name): Path<String>, Json(r): Json<ReorgReq>) -> R<Json<J>> {
    let w = wh(&app, &name)?;
    if let Some(e) = r.enabled {
        w.set_reorg(e);
        app.audit("hmi", "wh_reorg", format!("{name}: riorganizzazione automatica {}", if e { "attiva" } else { "disattiva" }));
    }
    let mut created = vec![];
    if let Some(n) = r.run.filter(|&n| n > 0) {
        created = w.reorg_now(r.aisle.filter(|&a| a > 0), n.min(500), &w.unavailable_aisles(&app.db));
        app.audit("hmi", "wh_reorg", format!("{name}: {} movimentazioni create", created.len()));
    }
    Ok(Json(json!({"ok": true, "created": created})))
}

/// Libreria: metadati simboli (categoria, famiglia UDT, faceplate, binding
/// predefiniti) e tipi UDT standard importabili nel progetto.
async fn library(State(app): S) -> Json<J> {
    let dir = app.library_dir.clone();
    let read = |f: &str| -> J {
        std::fs::read_to_string(dir.join(f)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(J::Null)
    };
    Json(json!({"symbols": read("symbols.json"), "udts": read("udts.json")}))
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
    app.rules.save(&dp, set, &crate::rules::Checker { tag: &|t| app.db.def(t).is_some(), class: &classes, wh: &|w| app.warehouse(w).is_some() })?;
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
            eng.save(&dp, set, &crate::rules::Checker { tag: &|n| app.db.def(n).is_some(), class: &classes, wh: &|w| app.warehouse(w).is_some() })?;
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

/// File di una vista: progetto (personalizzazione) o libreria standard.
fn view_file(app: &AppState, id: &str) -> Option<(std::path::PathBuf, bool)> {
    let p = app.views_dir.join(format!("{id}.json"));
    if p.exists() {
        return Some((p, false));
    }
    let l = app.library_dir.join("faceplates").join(format!("{id}.json"));
    l.exists().then_some((l, true))
}

fn lib_view_exists(app: &AppState, id: &str) -> bool {
    app.library_dir.join("faceplates").join(format!("{id}.json")).exists()
}

fn json_files(dir: &std::path::Path, ext: &str) -> Vec<(String, std::path::PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .map(|p| (p.file_stem().unwrap().to_string_lossy().to_string(), p))
        .collect()
}

/// Viste del progetto + faceplate della libreria standard (`library: true`);
/// una vista di progetto con lo stesso id di un faceplate standard lo personalizza (`overrides: true`).
async fn list_views(State(app): S) -> R<Json<J>> {
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    let lib = json_files(&app.library_dir.join("faceplates"), "json");
    for (from_lib, files) in [(false, json_files(&app.views_dir, "json")), (true, lib)] {
        for (id, p) in files {
            if !seen.insert(id.clone()) {
                continue;
            }
            let v: J = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(J::Null);
            out.push(json!({"id": id, "title": v.get("title").cloned().unwrap_or(json!(id)),
                            "home": v.get("home").cloned().unwrap_or(json!(false)),
                            "area": v.get("area").cloned().unwrap_or(J::Null),
                            "faceplate": v.get("faceplate").cloned().unwrap_or(json!(from_lib)),
                            "menu": v.get("menu").cloned().unwrap_or(json!(true)),
                            "params": v.get("params").cloned().unwrap_or(json!({})),
                            "library": from_lib,
                            "overrides": !from_lib && lib_view_exists(&app, &id)}));
        }
    }
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Json(J::Array(out)))
}

async fn get_view(State(app): S, Path(id): Path<String>) -> R<Response> {
    safe_id(&id)?;
    let (p, _) = view_file(&app, &id).ok_or_else(|| ApiError(StatusCode::NOT_FOUND, format!("vista {id} non trovata")))?;
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
    // le viste si salvano sempre nel progetto: per un faceplate standard è una personalizzazione
    let p = app.views_dir.join(format!("{id}.json"));
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(&view).unwrap())?;
    std::fs::rename(tmp, p)?;
    let overrides = lib_view_exists(&app, &id);
    app.audit("designer", "save_view", if overrides { format!("{id} (personalizzazione del faceplate standard)") } else { id.clone() });
    Ok(Json(json!({"ok": true, "warnings": warnings, "overrides": overrides})))
}

async fn del_view(State(app): S, Path(id): Path<String>) -> R<Json<J>> {
    require_designer(&app)?;
    safe_id(&id)?;
    let p = app.views_dir.join(format!("{id}.json"));
    let lib = lib_view_exists(&app, &id);
    if !p.exists() {
        return Err(if lib {
            ApiError(StatusCode::FORBIDDEN, format!("{id} è un faceplate della libreria standard (sola lettura)"))
        } else {
            ApiError(StatusCode::NOT_FOUND, format!("vista {id} non trovata"))
        });
    }
    std::fs::remove_file(p)?;
    app.audit("designer", "delete_view", if lib { format!("{id}: ripristinato il faceplate standard") } else { id });
    Ok(Json(json!({"ok": true, "restored": lib})))
}

// ---------------------------------------------------------------- simboli SVG

/// Simboli del progetto + simboli della libreria standard (`library: true`).
async fn list_symbols(State(app): S) -> R<Json<J>> {
    let mut out = vec![];
    let mut seen = std::collections::HashSet::new();
    let lib_dir = app.library_dir.join("symbols");
    for (from_lib, files) in [(false, json_files(&app.symbols_dir, "svg")), (true, json_files(&lib_dir, "svg"))] {
        for (id, p) in files {
            if !seen.insert(id.clone()) {
                continue;
            }
            let svg = std::fs::read_to_string(&p).unwrap_or_default();
            out.push(json!({"id": id, "svg": svg, "library": from_lib, "overrides": !from_lib && lib_dir.join(format!("{id}.svg")).exists()}));
        }
    }
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Json(J::Array(out)))
}

async fn get_symbol(State(app): S, Path(id): Path<String>) -> R<Response> {
    safe_id(&id)?;
    let s = std::fs::read_to_string(app.symbols_dir.join(format!("{id}.svg")))
        .or_else(|_| std::fs::read_to_string(app.library_dir.join("symbols").join(format!("{id}.svg"))))
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
    let p = app.symbols_dir.join(format!("{id}.svg"));
    let lib = app.library_dir.join("symbols").join(format!("{id}.svg")).exists();
    if !p.exists() {
        return Err(if lib {
            ApiError(StatusCode::FORBIDDEN, format!("{id} è un simbolo della libreria standard (sola lettura)"))
        } else {
            ApiError(StatusCode::NOT_FOUND, "simbolo non trovato".into())
        });
    }
    std::fs::remove_file(p)?;
    app.audit("designer", "delete_symbol", id);
    Ok(Json(json!({"ok": true, "restored": lib})))
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

/// Cartella degli script (un file per funzione).
fn scripts_dir(app: &AppState) -> R<std::path::PathBuf> {
    let p = app.script_path.clone().ok_or_else(|| bad("nessuna cartella script configurata (scripting.module)"))?;
    if !p.is_dir() {
        return Err(bad(format!("{} non è una cartella: gli script sono un file per funzione", p.display())));
    }
    Ok(p)
}

fn script_name(name: &str) -> R<()> {
    let ok = !name.is_empty() && name.len() <= 64 && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ok { Ok(()) } else { Err(bad("nome script non valido (identificatore Python)")) }
}

/// Dove è usata ogni funzione: punti di decisione, script a timer, azioni delle viste.
fn script_usage(app: &AppState) -> HashMap<String, Vec<J>> {
    let mut u: HashMap<String, Vec<J>> = HashMap::new();
    let cfg = app.cfg();
    for d in &cfg.decision_points {
        if let Some(f) = &d.function {
            u.entry(f.clone()).or_default().push(json!({"kind": "decision", "label": format!("Punto di decisione {}", d.name), "point": d.name}));
        }
    }
    for t in &cfg.timer_scripts {
        u.entry(t.function.clone()).or_default().push(json!({"kind": "timer", "label": format!("Timer {} ({} ms{})", t.name, t.interval_ms, if t.enabled { "" } else { ", disattivo" })}));
    }
    fn walk(v: &J, view: &str, u: &mut HashMap<String, Vec<J>>) {
        match v {
            J::Object(m) => {
                if m.get("kind").and_then(J::as_str) == Some("script")
                    && let Some(f) = m.get("fn").and_then(J::as_str)
                {
                    let e = u.entry(f.to_string()).or_default();
                    if !e.iter().any(|x| x["view"] == view) {
                        e.push(json!({"kind": "hmi", "label": format!("Vista {view}"), "view": view}));
                    }
                }
                m.values().for_each(|x| walk(x, view, u));
            }
            J::Array(a) => a.iter().for_each(|x| walk(x, view, u)),
            _ => {}
        }
    }
    for (id, p) in json_files(&app.views_dir, "json") {
        if let Some(v) = std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str::<J>(&s).ok()) {
            walk(&v, &id, &mut u);
        }
    }
    u
}

/// Simboli di primo livello di un modulo (def/class/assegnazioni) per i suggerimenti.
fn top_symbols(src: &str) -> Vec<String> {
    let mut out = vec![];
    for l in src.lines() {
        let l2 = l.trim_end();
        if l2.starts_with(' ') || l2.starts_with('\t') {
            continue;
        }
        let name = if let Some(r) = l2.strip_prefix("def ").or_else(|| l2.strip_prefix("class ")) {
            r.split(['(', ':']).next().unwrap_or("").trim().to_string()
        } else {
            let lhs = l2.split('=').next().unwrap_or("");
            if l2.contains('=') && !l2.contains("==") && !lhs.contains('(') && !lhs.contains('[') {
                lhs.split(',').map(str::trim).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(",")
            } else {
                String::new()
            }
        };
        for n in name.split(',') {
            if !n.is_empty() && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !out.contains(&n.to_string()) {
                out.push(n.to_string());
            }
        }
    }
    out
}

/// Elenco degli script: file, tipo, documentazione, errori di caricamento, utilizzi.
async fn script_files(State(app): S) -> R<Json<J>> {
    let dir = scripts_dir(&app)?;
    let st = app.scripts.status();
    let usage = script_usage(&app);
    let mut files = vec![];
    for (name, p) in json_files(&dir, "py") {
        let src = std::fs::read_to_string(&p).unwrap_or_default();
        let meta = std::fs::metadata(&p).ok();
        let loaded = st.files.iter().find(|f| f["name"] == name.as_str());
        let used = usage.get(&name).cloned().unwrap_or_default();
        let kind = if name.starts_with('_') { "module" } else { "function" };
        let group = if kind == "module" {
            "module"
        } else if used.iter().any(|u| u["kind"] == "decision") {
            "decision"
        } else if used.iter().any(|u| u["kind"] == "timer") {
            "timer"
        } else if used.iter().any(|u| u["kind"] == "hmi") {
            "hmi"
        } else {
            "unused"
        };
        files.push(json!({
            "name": name, "kind": kind, "group": group,
            "doc": loaded.map(|f| f["doc"].clone()).unwrap_or(J::Null),
            "error": loaded.map(|f| f["error"].clone()).unwrap_or(J::Null),
            "loaded": loaded.is_some(),
            "usage": used, "symbols": top_symbols(&src), "lines": src.lines().count(),
            "mtime": meta.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64),
        }));
    }
    files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let hmi = app.hmi_scripts.status();
    Ok(Json(json!({"dir": dir.display().to_string(), "files": files,
        "status": {"decisions": st, "hmi": {"running": hmi.running, "load_error": hmi.load_error, "restarts": hmi.restarts}}})))
}

async fn script_file_get(State(app): S, Path(name): Path<String>) -> R<Json<J>> {
    script_name(&name)?;
    let p = scripts_dir(&app)?.join(format!("{name}.py"));
    let src = std::fs::read_to_string(&p).map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("script {name} non trovato")))?;
    Ok(Json(json!({"name": name, "source": src, "path": p.display().to_string()})))
}

#[derive(Deserialize)]
struct SourceBody {
    source: String,
}

/// Controllo statico con l'interprete: sintassi (riga/colonna), definizione della
/// funzione con lo stesso nome del file, nomi non definiti (pyflakes, se installato).
async fn check_source(app: &AppState, name: &str, source: &str) -> R<J> {
    const CHECK: &str = r#"
import ast, json, sys
name, src = sys.argv[1], sys.stdin.read()
out = {"ok": True, "errors": [], "warnings": []}
try:
    tree = ast.parse(src, name + ".py")
except SyntaxError as e:
    out["ok"] = False
    out["errors"].append({"line": e.lineno or 1, "col": e.offset or 1, "msg": "SyntaxError: " + (e.msg or "")})
    print(json.dumps(out)); sys.exit()
if not name.startswith("_"):
    fns = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == name]
    if not fns:
        out["ok"] = False
        out["errors"].append({"line": 1, "col": 1, "msg": "manca def %s(payload, ctx): il file deve definire la funzione con il suo nome" % name})
    elif len(fns[0].args.args) != 2:
        out["warnings"].append({"line": fns[0].lineno, "col": 1, "msg": "la funzione deve avere due argomenti: (payload, ctx)"})
try:
    from pyflakes.api import check
    from pyflakes.reporter import Reporter
    import io
    class R(Reporter):
        def __init__(self): pass
        def unexpectedError(self, f, m): pass
        def syntaxError(self, f, m, l, c, t): pass
        def flake(self, m):
            out["warnings"].append({"line": m.lineno, "col": (m.col or 0) + 1, "msg": m.message % m.message_args})
    check(src, name + ".py", R())
except ImportError:
    # controllo semplificato: nomi mai definiti nel file e import non usati
    import builtins
    bound, loads, imports = set(dir(builtins)) | {"__name__", "__file__", "__doc__"}, [], {}
    for n in ast.walk(tree):
        if isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            bound.add(n.name)
        elif isinstance(n, ast.arg):
            bound.add(n.arg)
        elif isinstance(n, ast.Name):
            (bound.add(n.id) if isinstance(n.ctx, (ast.Store, ast.Del)) else loads.append(n))
        elif isinstance(n, (ast.Import, ast.ImportFrom)):
            for a in n.names:
                nm = (a.asname or a.name).split(".")[0]
                if a.name == "*":
                    bound.add("*")
                bound.add(nm)
                imports[nm] = n.lineno
        elif isinstance(n, ast.ExceptHandler) and n.name:
            bound.add(n.name)
        elif isinstance(n, (ast.Global, ast.Nonlocal)):
            bound.update(n.names)
    used = {n.id for n in loads} | {n.value.id for n in ast.walk(tree) if isinstance(n, ast.Attribute) and isinstance(n.value, ast.Name)}
    if "*" not in bound:
        seen = set()
        for n in loads:
            if n.id not in bound and (n.id, n.lineno) not in seen:
                seen.add((n.id, n.lineno))
                out["warnings"].append({"line": n.lineno, "col": n.col_offset + 1, "msg": "nome non definito: '%s'" % n.id})
    for nm, ln in imports.items():
        if nm not in used:
            out["warnings"].append({"line": ln, "col": 1, "msg": "'%s' importato ma non usato" % nm})
print(json.dumps(out))
"#;
    let mut child = tokio::process::Command::new(&app.cfg().scripting.python)
        .args(["-c", CHECK, name])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    {
        use tokio::io::AsyncWriteExt;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(source.as_bytes()).await?;
    }
    let out = tokio::time::timeout(Duration::from_secs(10), child.wait_with_output()).await.map_err(|_| bad("timeout controllo script"))??;
    serde_json::from_slice(&out.stdout).map_err(|_| bad(format!("controllo script fallito: {}", String::from_utf8_lossy(&out.stderr))))
}

#[derive(Deserialize)]
struct CheckReq {
    name: String,
    source: String,
}

async fn script_check(State(app): S, Json(r): Json<CheckReq>) -> R<Json<J>> {
    script_name(&r.name)?;
    Ok(Json(check_source(&app, &r.name, &r.source).await?))
}

/// Salva uno script (dopo il controllo sintattico) e ricarica i processi Python.
async fn script_file_put(State(app): S, Path(name): Path<String>, Json(b): Json<SourceBody>) -> R<Json<J>> {
    require_designer(&app)?;
    script_name(&name)?;
    let dir = scripts_dir(&app)?;
    let check = check_source(&app, &name, &b.source).await?;
    if check["ok"] != json!(true) {
        return Ok(Json(json!({"ok": false, "check": check})));
    }
    let p = dir.join(format!("{name}.py"));
    // storico: copia della versione precedente
    if p.exists() {
        let hist = dir.join(".history");
        std::fs::create_dir_all(&hist)?;
        let _ = std::fs::copy(&p, hist.join(format!("{name}.{}.py", crate::tagdb::now_ms())));
    }
    let tmp = dir.join(format!(".{name}.py.tmp"));
    std::fs::write(&tmp, &b.source)?;
    std::fs::rename(&tmp, &p)?;
    let st = app.reload_scripts().await?;
    app.audit("designer", "save_script", format!("{name}.py"));
    Ok(Json(json!({"ok": true, "check": check, "status": st})))
}

#[derive(Deserialize, Default)]
struct DelQ {
    #[serde(default)]
    force: bool,
}

async fn script_file_del(State(app): S, Path(name): Path<String>, Query(q): Query<DelQ>) -> R<Json<J>> {
    require_designer(&app)?;
    script_name(&name)?;
    let dir = scripts_dir(&app)?;
    let used = script_usage(&app).remove(&name).unwrap_or_default();
    if !used.is_empty() && !q.force {
        return Err(ApiError(StatusCode::CONFLICT, format!("{name} è usato da: {}", used.iter().filter_map(|u| u["label"].as_str()).collect::<Vec<_>>().join(", "))));
    }
    let p = dir.join(format!("{name}.py"));
    let hist = dir.join(".history");
    std::fs::create_dir_all(&hist)?;
    std::fs::rename(&p, hist.join(format!("{name}.{}.deleted.py", crate::tagdb::now_ms()))).map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("script {name} non trovato")))?;
    let st = app.reload_scripts().await?;
    app.audit("designer", "delete_script", format!("{name}.py"));
    Ok(Json(json!({"ok": true, "status": st})))
}

#[derive(Deserialize)]
struct TestReq {
    name: String,
    /// sorgente non salvato (se assente: il file su disco)
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    payload: J,
    /// tag da sovrascrivere rispetto ai valori live
    #[serde(default)]
    tags: serde_json::Map<String, J>,
    #[serde(default)]
    point: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
}

/// Esegue una funzione in un interprete isolato (copia della cartella script con
/// il sorgente in modifica): le scritture NON vengono applicate ai tag.
async fn script_test(State(app): S, Json(r): Json<TestReq>) -> R<Json<J>> {
    require_designer(&app)?;
    script_name(&r.name)?;
    let dir = scripts_dir(&app)?;
    let tmp = std::env::temp_dir().join(format!("ptwcs_test_{}_{}", std::process::id(), crate::tagdb::now_ms()));
    std::fs::create_dir_all(&tmp)?;
    for (n, p) in json_files(&dir, "py") {
        std::fs::copy(&p, tmp.join(format!("{n}.py")))?;
    }
    if let Some(src) = &r.source {
        std::fs::write(tmp.join(format!("{}.py", r.name)), src)?;
    }
    let mut tags = match tags_ctx(&app) {
        J::Object(m) => m,
        _ => Default::default(),
    };
    tags.extend(r.tags.clone());
    let ctx = json!({"tags": tags, "point": r.point.clone().unwrap_or_else(|| format!("test:{}", r.name)), "user": "test"});
    let host = crate::script::ScriptHost::new(app.cfg().scripting.python.clone(), Some(tmp.clone()));
    let res = async {
        let st = host.start().await?;
        let t = Duration::from_millis(r.timeout_ms.unwrap_or(5000).clamp(100, 30000));
        let rep = host.test(&r.name, r.payload.clone(), ctx, t).await?;
        anyhow::Ok((st, rep))
    }
    .await;
    drop(host);
    let _ = std::fs::remove_dir_all(&tmp);
    app.audit("designer", "test_script", r.name.clone());
    Ok(Json(match res {
        Ok((st, rep)) => json!({"reply": rep, "load_error": st.load_error, "files": st.files}),
        Err(e) => json!({"reply": {"ok": false, "error": format!("{e:#}")}}),
    }))
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
