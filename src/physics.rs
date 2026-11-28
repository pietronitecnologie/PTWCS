//! Layer fisico: simulazione di componenti e materiali per il collaudo virtuale.
//!
//! Il layout disegnato per lo SCADA (le viste) diventa il modello: ogni componente
//! può avere una sezione `physics` (nastro, fotocellula, scanner, deviatore, sorgente,
//! pozzo, fermo). Il PLC (PLCSIM Advanced o reale) comanda gli attuatori scrivendo i
//! tag (marcia nastro, posizione deviatore…); il motore fisico fa muovere i materiali e
//! restituisce i sensori (fotocellule, letture barcode, finecorsa, velocità) scrivendo
//! i tag "sensore", che nel PLC finiscono negli ingressi simulati.
//!
//! Modello cinematico 1D: ogni nastro è un percorso lungo il suo asse; le unità di
//! carico hanno posizione e lunghezza, si accumulano senza sovrapporsi e passano al
//! componente successivo (nastro, deviatore, pozzo) quando questo può accettarle.

mod crane;
mod fluids;

use crate::app::{AppState, Event};
use crate::config::{Config, Material, PhysicsCfg, Protocol};
use crate::tagdb::TagDb;
use crate::value::Value;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use serde::{Deserialize, Serialize};
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

const EPS: f64 = 1e-6;

// ------------------------------------------------------------------ definizione (JSON nella vista)

/// Proprietà fisiche di un componente (campo `physics` del componente nella vista).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PhysDef {
    /// conveyor | photocell | scanner | diverter | source | sink | stopper
    pub kind: String,
    // --- nastro
    pub length_m: Option<f64>,
    /// Velocità massima (m/s).
    pub speed: Option<f64>,
    /// Accelerazione e decelerazione (m/s²).
    pub accel: Option<f64>,
    pub decel: Option<f64>,
    /// Attuatore: marcia (bool).
    pub run: String,
    /// Attuatore opzionale: setpoint di velocità; `sp_max` = valore che vale la velocità massima.
    pub speed_sp: String,
    pub sp_max: Option<f64>,
    /// Sensore opzionale: velocità reale × `fb_scale`.
    pub speed_fb: String,
    pub fb_scale: Option<f64>,
    /// Componente successivo (id nella stessa vista): nastro, deviatore o pozzo.
    pub next: String,
    /// Le unità escono dall'impianto alla fine del nastro.
    pub sink: bool,
    /// Distanza minima tra le unità in accumulo (m).
    pub gap_m: Option<f64>,
    // --- sensori e componenti montati su un nastro
    /// Nastro su cui è montato (vuoto = il più vicino nel disegno).
    pub on: String,
    /// Posizione lungo il nastro (m dall'inizio; vuoto = dal disegno).
    pub at_m: Option<f64>,
    /// Fotocellula: tag del segnale; `invert` = contatto NC (true a fotocellula libera).
    pub tag: String,
    pub invert: bool,
    // --- scanner
    pub code_tag: String,
    pub count_tag: String,
    pub weight_tag: String,
    pub weight_scale: Option<f64>,
    pub noread_pct: f64,
    pub noread_text: Option<String>,
    // --- deviatore
    pub position: String,
    pub outputs: Vec<PhysOut>,
    pub switch_s: Option<f64>,
    pub fb_tag: String,
    // --- sorgente
    pub interval_min: Option<f64>,
    pub interval_max: Option<f64>,
    /// Materiali generati (nomi separati da virgola; vuoto = tutti).
    pub materials: String,
    /// Barcode: elenco separato da virgole, oppure modello con `{n}` / `{n:6}` (contatore).
    pub barcodes: String,
    pub enable: String,
    pub max_items: Option<u64>,
    // --- fermo / attuatore lineare
    pub cmd: String,
    pub time_s: Option<f64>,
    pub fb_ext: String,
    pub fb_ret: String,
    // --- fluidi (vedi physics/fluids.rs)
    /// Serbatoio: volume (m³), altezza utile (m), quota del fondo (m).
    pub volume_m3: Option<f64>,
    pub height_m: Option<f64>,
    pub elevation_m: Option<f64>,
    /// Sensori del serbatoio: livello (valore a serbatoio pieno = `level_max`, default 100 = %),
    /// temperatura (°C), pressione sul fondo (bar).
    pub level_tag: String,
    pub level_max: Option<f64>,
    pub temp_tag: String,
    pub pressure_tag: String,
    /// Stato iniziale: livello (% del volume) e temperatura (°C).
    pub init_level: Option<f64>,
    pub init_temp: Option<f64>,
    /// Dispersione termica verso l'ambiente (W/K).
    pub loss_w_k: Option<f64>,
    /// Alimentazione: pressione (bar) e temperatura (°C).
    pub pressure_bar: Option<f64>,
    pub temp_c: Option<f64>,
    /// Valvola: Kv (m³/h a 1 bar); con `analog` il tag `position` è l'apertura 0–100 %.
    pub kv: Option<f64>,
    pub analog: bool,
    pub fb_open: String,
    pub fb_closed: String,
    pub fb_pos: String,
    /// Pompa: prevalenza a portata nulla (m) e portata massima (m³/h); retroazione marcia.
    pub head_m: Option<f64>,
    pub q_max: Option<f64>,
    pub fb_run: String,
    /// Tubazione: da / a (serbatoio, alimentazione, scarico, giunzione), valvole e pompe in serie.
    pub from: String,
    pub to: String,
    pub elements: String,
    pub flow_tag: String,
    pub flow_scale: Option<f64>,
    /// Riscaldatore: potenza (kW), termostato locale (setpoint) con isteresi.
    pub power_kw: Option<f64>,
    pub setpoint_tag: String,
    pub hysteresis: Option<f64>,
    /// Motore / agitatore: velocità massima (es. giri/min).
    pub rpm: Option<f64>,
    // --- trasloelevatore (vedi physics/crane.rs); quote in mm, velocità in m/s, accelerazioni in m/s²
    pub cols: Option<u64>,
    pub levels: Option<u64>,
    pub pitch_x_mm: Option<f64>,
    pub pitch_y_mm: Option<f64>,
    pub x0_mm: Option<f64>,
    pub y0_mm: Option<f64>,
    pub x_max_mm: Option<f64>,
    pub y_max_mm: Option<f64>,
    pub z_reach_mm: Option<f64>,
    pub lift_mm: Option<f64>,
    pub x_target: String,
    pub y_target: String,
    pub z_target: String,
    pub x_pos: String,
    pub y_pos: String,
    pub z_pos: String,
    pub x_inpos: String,
    pub y_inpos: String,
    pub z_inpos: String,
    pub x_vmax: Option<f64>,
    pub y_vmax: Option<f64>,
    pub z_vmax: Option<f64>,
    pub x_acc: Option<f64>,
    pub y_acc: Option<f64>,
    pub z_acc: Option<f64>,
    pub load_tag: String,
    pub fault_tag: String,
    pub moving_tag: String,
    pub reset_tag: String,
    /// Occupazione delle celle tracciata dalla fisica (doppio deposito, cella vuota).
    pub track: bool,
    pub init_fill_pct: Option<f64>,
    /// Carichi trasportabili insieme (2 = traslo a doppia UdC).
    pub capacity: Option<u64>,
    /// Nastri dei P&D (colonna 0): lato 1 = ingresso, lato 2 = uscita.
    pub pd_in: String,
    pub pd_out: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PhysOut {
    /// Valore del tag posizione che seleziona questa uscita.
    pub value: String,
    pub next: String,
}

/// Geometria di un componente sulla vista (px).
#[derive(Debug, Clone, Default)]
struct Geo {
    x0: f64,
    y0: f64,
    ux: f64,
    uy: f64,
    w: f64,
    h: f64,
    cx: f64,
    cy: f64,
    /// Pixel per metro lungo l'asse.
    ppm: f64,
    angle: f64,
}

impl Geo {
    fn from(c: &J) -> Geo {
        let n = |k: &str, d: f64| c.get(k).and_then(J::as_f64).unwrap_or(d);
        let (x, y, w, h, rot) = (n("x", 0.0), n("y", 0.0), n("w", 100.0).max(1.0), n("h", 40.0).max(1.0), n("rotation", 0.0));
        let left = c.pointer("/props/direction").and_then(J::as_str) == Some("left");
        let a = (rot + if left { 180.0 } else { 0.0 }).to_radians();
        let (ux, uy) = (a.cos(), a.sin());
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        Geo { x0: cx - ux * w / 2.0, y0: cy - uy * w / 2.0, ux, uy, w, h, cx, cy, ppm: 1.0, angle: a.to_degrees() }
    }
    /// Proiezione di un punto sull'asse: (posizione lungo l'asse in px, distanza dall'asse in px).
    fn project(&self, px: f64, py: f64) -> (f64, f64) {
        let (dx, dy) = (px - self.x0, py - self.y0);
        (dx * self.ux + dy * self.uy, (dx * -self.uy + dy * self.ux).abs())
    }
    fn at(&self, s_m: f64) -> (f64, f64) {
        (self.x0 + self.ux * s_m * self.ppm, self.y0 + self.uy * s_m * self.ppm)
    }
}

// ------------------------------------------------------------------ modello compilato

#[derive(Debug, Clone, Copy, PartialEq)]
enum Next {
    None,
    Conv(usize),
    Div(usize),
    Sink(usize),
    /// Uscita dall'impianto senza contatore.
    Out,
}

#[derive(Debug, Clone)]
struct Conv {
    key: String,
    view: String,
    len: f64,
    vmax: f64,
    acc: f64,
    dec: f64,
    run: String,
    sp: String,
    sp_max: f64,
    fb: String,
    fb_scale: f64,
    next: Next,
    gap: f64,
    geo: Geo,
    v: f64,
    failed: bool,
    last_fb: f64,
}

#[derive(Debug, Clone)]
struct Photo {
    key: String,
    conv: usize,
    at: f64,
    tag: String,
    invert: bool,
    stuck: Option<bool>,
    state: bool,
}

#[derive(Debug, Clone)]
struct Scanner {
    key: String,
    conv: usize,
    at: f64,
    code_tag: String,
    count_tag: String,
    weight_tag: String,
    weight_scale: f64,
    noread_pct: f64,
    noread_text: String,
    force_noread: bool,
    count: i64,
    reads: u64,
    last_code: String,
}

#[derive(Debug, Clone)]
struct Div {
    key: String,
    pos_tag: String,
    outputs: Vec<(String, Next)>,
    switch_s: f64,
    fb_tag: String,
    cmd: Option<String>,
    timer: f64,
    current: Option<usize>,
    stuck: bool,
}

#[derive(Debug, Clone)]
struct Source {
    key: String,
    conv: usize,
    imin: f64,
    imax: f64,
    mats: Vec<usize>,
    barcodes: Vec<String>,
    pattern: Option<String>,
    enable: String,
    max_items: Option<u64>,
    next_at: f64,
    n: u64,
    spawned: u64,
    enabled: Option<bool>,
}

#[derive(Debug, Clone)]
struct Sink {
    key: String,
    count_tag: String,
    count: i64,
    received: u64,
}

#[derive(Debug, Clone)]
struct Stopper {
    key: String,
    conv: usize,
    at: f64,
    cmd: String,
    invert: bool,
    time_s: f64,
    fb_ext: String,
    fb_ret: String,
    pos: f64,
    stuck: bool,
}

/// Unità di carico in movimento.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub id: u64,
    #[serde(skip)]
    conv: usize,
    /// Posizione del fronte sul nastro corrente (m).
    pub front: f64,
    pub len: f64,
    pub width: f64,
    pub code: String,
    pub weight: f64,
    pub material: String,
    pub color: String,
    #[serde(skip)]
    prev: Option<usize>,
    pub jammed: bool,
    pub born: f64,
}

#[derive(Debug, Default)]
struct Model {
    convs: Vec<Conv>,
    photos: Vec<Photo>,
    scanners: Vec<Scanner>,
    divs: Vec<Div>,
    sources: Vec<Source>,
    sinks: Vec<Sink>,
    stoppers: Vec<Stopper>,
    mats: Vec<Material>,
    errors: Vec<String>,
    /// Rete dei fluidi (serbatoi, valvole, pompe, tubazioni…).
    fl: fluids::Fluids,
    /// Trasloelevatori con assi.
    cranes: Vec<crane::Crane>,
}

fn default_material() -> Material {
    Material {
        name: "Collo".into(),
        length_m: 0.4,
        width_m: 0.3,
        height_m: None,
        mass_min: 1.0,
        mass_max: 20.0,
        color: "#c8915a".into(),
        share: None,
        description: String::new(),
    }
}

/// Componente letto da una vista.
struct RawComp {
    view: String,
    id: String,
    geo: Geo,
    def: PhysDef,
}

/// Legge le sezioni `physics` dei componenti di tutte le viste del progetto.
fn read_views(dir: &Path) -> (Vec<RawComp>, HashMap<String, f64>, Vec<String>) {
    let mut out = vec![];
    let mut scales = HashMap::new();
    let mut errors = vec![];
    let mut files: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")).collect();
    files.sort();
    for p in files {
        let id = p.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let Ok(v) = std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<J>(&t).map_err(|e| e.to_string())) else {
            continue;
        };
        if let Some(s) = v.pointer("/physics/scale_mm").and_then(J::as_f64) {
            scales.insert(id.clone(), s);
        }
        for c in v.get("components").and_then(J::as_array).into_iter().flatten() {
            let Some(ph) = c.get("physics").filter(|p| p.is_object()) else { continue };
            let cid = c.get("id").and_then(J::as_str).unwrap_or_default().to_string();
            match serde_json::from_value::<PhysDef>(ph.clone()) {
                Ok(def) if !def.kind.is_empty() => out.push(RawComp { view: id.clone(), id: cid, geo: Geo::from(c), def }),
                Ok(_) => {}
                Err(e) => errors.push(format!("{id}/{cid}: proprietà fisiche non valide: {e}")),
            }
        }
    }
    (out, scales, errors)
}

/// Riferimento a un componente: `id` nella stessa vista, `vista/id` in un'altra vista
/// (per impianti divisi su più pagine: un nastro può proseguire in un'altra vista).
fn full_key(view: &str, id: &str) -> String {
    if id.contains('/') { id.to_string() } else { format!("{view}/{id}") }
}

/// Elenco di tag separati da virgola (es. heartbeat di più PLC).
fn tag_list(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(',').map(str::trim).filter(|t| !t.is_empty()).map(str::to_string)
}

fn opt(s: &str) -> String {
    s.trim().to_string()
}

impl Model {
    /// Compila il modello dalle viste. I componenti con errori vengono esclusi
    /// (e l'errore è riportato), il resto funziona.
    fn build(cfg: &Config, db: &TagDb, views_dir: &Path) -> Model {
        let ph = &cfg.project.physics;
        let (raw, scales, mut errors) = read_views(views_dir);
        let mut m = Model { mats: if ph.materials.is_empty() { vec![default_material()] } else { ph.materials.clone() }, ..Default::default() };
        let allowed = |driver: &str| -> bool {
            ph.drivers.iter().any(|d| d == driver) || cfg.drivers.iter().any(|d| d.name == driver && d.protocol == Protocol::Sim)
        };
        // controllo dei tag: attuatori (letti) e sensori (scritti dalla simulazione)
        let mut errs: Vec<String> = vec![];
        let actuator = |key: &str, what: &str, t: &str, errs: &mut Vec<String>| -> String {
            let t = opt(t);
            if !t.is_empty() && db.def(&t).is_none() {
                errs.push(format!("{key}: {what}: tag sconosciuto '{t}'"));
            }
            t
        };
        let sensor = |key: &str, what: &str, t: &str, errs: &mut Vec<String>| -> String {
            let t = opt(t);
            if t.is_empty() {
                return t;
            }
            match db.def(&t) {
                None => errs.push(format!("{key}: {what}: tag sconosciuto '{t}'")),
                Some(d) if !allowed(&d.driver) => errs.push(format!(
                    "{key}: {what}: il tag '{t}' è sul driver {} che non è abilitato alla simulazione (Designer → Fisica → driver simulabili)",
                    d.driver
                )),
                Some(d) if !d.writable && !cfg.drivers.iter().any(|x| x.name == d.driver && x.protocol == Protocol::Sim) => {
                    errs.push(format!("{key}: {what}: il tag '{t}' non è scrivibile"))
                }
                _ => {}
            }
            t
        };
        let key_of = |r: &RawComp| format!("{}/{}", r.view, r.id);

        // 1. nastri
        let mut conv_idx: HashMap<String, usize> = HashMap::new();
        for r in raw.iter().filter(|r| r.def.kind == "conveyor") {
            let k = key_of(r);
            let d = &r.def;
            let mut geo = r.geo.clone();
            let scale = scales.get(&r.view).copied().unwrap_or(ph.scale_mm);
            let len = d.length_m.filter(|l| *l > 0.0).unwrap_or(geo.w * scale / 1000.0);
            geo.ppm = geo.w / len;
            let n0 = errs.len();
            let c = Conv {
                key: k.clone(),
                view: r.view.clone(),
                len,
                vmax: d.speed.filter(|v| *v > 0.0).unwrap_or(0.5),
                acc: d.accel.filter(|v| *v > 0.0).unwrap_or(1.0),
                dec: d.decel.filter(|v| *v > 0.0).unwrap_or(1.5),
                run: actuator(&k, "marcia", &d.run, &mut errs),
                sp: actuator(&k, "setpoint velocità", &d.speed_sp, &mut errs),
                sp_max: d.sp_max.filter(|v| *v > 0.0).unwrap_or(100.0),
                fb: sensor(&k, "velocità reale", &d.speed_fb, &mut errs),
                fb_scale: d.fb_scale.filter(|v| *v > 0.0).unwrap_or(1.0),
                next: if d.sink { Next::Out } else { Next::None },
                gap: d.gap_m.unwrap_or(0.02).max(0.0),
                geo,
                v: 0.0,
                failed: false,
                last_fb: f64::NAN,
            };
            if c.run.is_empty() {
                errs.push(format!("{k}: nastro senza tag di marcia"));
            }
            if errs.len() == n0 {
                conv_idx.insert(k, m.convs.len());
                m.convs.push(c);
            }
        }
        // 2. deviatori e pozzi (servono per risolvere i "next")
        let mut div_idx: HashMap<String, usize> = HashMap::new();
        let mut sink_idx: HashMap<String, usize> = HashMap::new();
        for r in raw.iter().filter(|r| r.def.kind == "sink") {
            let k = key_of(r);
            let n0 = errs.len();
            let count_tag = sensor(&k, "contatore", &r.def.count_tag, &mut errs);
            if errs.len() == n0 {
                sink_idx.insert(k.clone(), m.sinks.len());
                let count = if count_tag.is_empty() { 0 } else { db.value(&count_tag).as_i64().unwrap_or(0) };
                m.sinks.push(Sink { key: k, count_tag, count, received: 0 });
            }
        }
        for r in raw.iter().filter(|r| r.def.kind == "diverter") {
            let k = key_of(r);
            div_idx.insert(k, usize::MAX); // riservato: i riferimenti si risolvono dopo
        }
        let resolve = |view: &str, id: &str, from: &str, allow_div: bool, errs: &mut Vec<String>, div_idx: &HashMap<String, usize>| -> Next {
            let id = id.trim();
            if id.is_empty() {
                return Next::None;
            }
            let k = full_key(view, id);
            if let Some(i) = conv_idx.get(&k) {
                Next::Conv(*i)
            } else if let Some(i) = sink_idx.get(&k) {
                Next::Sink(*i)
            } else if allow_div && div_idx.contains_key(&k) {
                Next::Div(usize::MAX)
            } else {
                errs.push(format!("{from}: componente successivo '{id}' non trovato (deve essere un nastro{}, un pozzo, nella stessa vista o come vista/id)", if allow_div { ", un deviatore" } else { "" }));
                Next::None
            }
        };
        for r in raw.iter().filter(|r| r.def.kind == "diverter") {
            let k = key_of(r);
            let n0 = errs.len();
            let pos_tag = actuator(&k, "posizione", &r.def.position, &mut errs);
            if pos_tag.is_empty() {
                errs.push(format!("{k}: deviatore senza tag di posizione"));
            }
            let mut outputs = vec![];
            for o in &r.def.outputs {
                let n = resolve(&r.view, &o.next, &k, false, &mut errs, &div_idx);
                if n != Next::None {
                    outputs.push((o.value.trim().to_string(), n));
                }
            }
            if outputs.is_empty() {
                errs.push(format!("{k}: deviatore senza uscite"));
            }
            let fb_tag = sensor(&k, "posizione raggiunta", &r.def.fb_tag, &mut errs);
            if errs.len() == n0 {
                div_idx.insert(k.clone(), m.divs.len());
                m.divs.push(Div { key: k, pos_tag, outputs, switch_s: r.def.switch_s.unwrap_or(0.4).max(0.0), fb_tag, cmd: None, timer: 0.0, current: None, stuck: false });
            } else {
                div_idx.remove(&k);
            }
        }
        // collegamenti dei nastri
        for r in raw.iter().filter(|r| r.def.kind == "conveyor") {
            let k = key_of(r);
            let Some(&ci) = conv_idx.get(&k) else { continue };
            if r.def.next.trim().is_empty() {
                continue;
            }
            let n = match resolve(&r.view, &r.def.next, &k, true, &mut errs, &div_idx) {
                Next::Div(_) => div_idx.get(&full_key(&r.view, r.def.next.trim())).map(|i| Next::Div(*i)).unwrap_or(Next::None),
                Next::Conv(i) if i == ci => {
                    errs.push(format!("{k}: un nastro non può essere il successivo di sé stesso"));
                    Next::None
                }
                n => n,
            };
            if n != Next::None {
                m.convs[ci].next = n;
            }
        }
        // 3. componenti montati su un nastro: nastro esplicito o il più vicino nel disegno
        let mount = |r: &RawComp, errs: &mut Vec<String>| -> Option<(usize, f64)> {
            let k = key_of(r);
            let ci = if r.def.on.trim().is_empty() {
                let mut best: Option<(f64, usize)> = None;
                for (i, c) in m.convs.iter().enumerate().filter(|(_, c)| c.view == r.view) {
                    let (s, d) = c.geo.project(r.geo.cx, r.geo.cy);
                    if s < -10.0 || s > c.geo.w + 10.0 {
                        continue;
                    }
                    let lim = c.geo.h / 2.0 + r.geo.h.min(r.geo.w) / 2.0 + 30.0;
                    if d <= lim && best.is_none_or(|b| d < b.0) {
                        best = Some((d, i));
                    }
                }
                match best {
                    Some((_, i)) => i,
                    None => {
                        errs.push(format!("{k}: nessun nastro sotto il componente (indicare il nastro in 'Montato su')"));
                        return None;
                    }
                }
            } else {
                match conv_idx.get(&full_key(&r.view, r.def.on.trim())) {
                    Some(i) => *i,
                    None => {
                        errs.push(format!("{k}: nastro '{}' non trovato", r.def.on.trim()));
                        return None;
                    }
                }
            };
            let c = &m.convs[ci];
            let at = r.def.at_m.unwrap_or_else(|| (c.geo.project(r.geo.cx, r.geo.cy).0 / c.geo.ppm).clamp(0.0, c.len));
            Some((ci, at.clamp(0.0, c.len)))
        };
        let mut photos = vec![];
        let mut scanners = vec![];
        let mut sources = vec![];
        let mut stoppers = vec![];
        for r in &raw {
            let k = key_of(r);
            let d = &r.def;
            let n0 = errs.len();
            match d.kind.as_str() {
                "conveyor" | "diverter" | "sink" => {}
                k if fluids::KINDS.contains(&k) || k == "crane" => {}
                "photocell" => {
                    let tag = sensor(&k, "segnale", &d.tag, &mut errs);
                    if tag.is_empty() {
                        errs.push(format!("{k}: fotocellula senza tag"));
                    }
                    if let (Some((conv, at)), true) = (mount(r, &mut errs), errs.len() == n0) {
                        photos.push(Photo { key: k, conv, at, tag, invert: d.invert, stuck: None, state: false });
                    }
                }
                "scanner" => {
                    let code_tag = sensor(&k, "barcode", &d.code_tag, &mut errs);
                    let count_tag = sensor(&k, "contatore letture", &d.count_tag, &mut errs);
                    let weight_tag = sensor(&k, "peso", &d.weight_tag, &mut errs);
                    if code_tag.is_empty() {
                        errs.push(format!("{k}: scanner senza tag barcode"));
                    }
                    if let (Some((conv, at)), true) = (mount(r, &mut errs), errs.len() == n0) {
                        let count = if count_tag.is_empty() { 0 } else { db.value(&count_tag).as_i64().unwrap_or(0) };
                        scanners.push(Scanner {
                            key: k,
                            conv,
                            at,
                            code_tag,
                            count_tag,
                            weight_tag,
                            weight_scale: d.weight_scale.unwrap_or(1.0),
                            noread_pct: d.noread_pct.clamp(0.0, 100.0),
                            noread_text: d.noread_text.clone().unwrap_or_else(|| "NOREAD".into()),
                            force_noread: false,
                            count,
                            reads: 0,
                            last_code: String::new(),
                        });
                    }
                }
                "source" => {
                    let enable = actuator(&k, "abilitazione", &d.enable, &mut errs);
                    let names: Vec<&str> = d.materials.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
                    let mut mats = vec![];
                    for n in &names {
                        match m.mats.iter().position(|x| x.name == *n) {
                            Some(i) => mats.push(i),
                            None => errs.push(format!("{k}: materiale '{n}' non definito")),
                        }
                    }
                    if mats.is_empty() {
                        mats = (0..m.mats.len()).collect();
                    }
                    let bc = d.barcodes.trim();
                    let (pattern, barcodes) = if bc.contains("{n") {
                        (Some(bc.to_string()), vec![])
                    } else {
                        (None, bc.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect())
                    };
                    let imin = d.interval_min.unwrap_or(2.0).max(0.05);
                    let imax = d.interval_max.unwrap_or(imin).max(imin);
                    if let (Some((conv, _)), true) = (mount(r, &mut errs), errs.len() == n0) {
                        sources.push(Source { key: k, conv, imin, imax, mats, barcodes, pattern, enable, max_items: d.max_items, next_at: 0.5, n: 0, spawned: 0, enabled: None });
                    }
                }
                "stopper" => {
                    let cmd = actuator(&k, "comando", &d.cmd, &mut errs);
                    if cmd.is_empty() {
                        errs.push(format!("{k}: fermo senza tag di comando"));
                    }
                    let fb_ext = sensor(&k, "finecorsa fuori", &d.fb_ext, &mut errs);
                    let fb_ret = sensor(&k, "finecorsa dentro", &d.fb_ret, &mut errs);
                    if let (Some((conv, at)), true) = (mount(r, &mut errs), errs.len() == n0) {
                        stoppers.push(Stopper { key: k, conv, at, cmd, invert: d.invert, time_s: d.time_s.unwrap_or(0.3).max(0.0), fb_ext, fb_ret, pos: 0.0, stuck: false });
                    }
                }
                other => errs.push(format!("{k}: tipo fisico sconosciuto '{other}'")),
            }
        }
        m.fl = fluids::Fluids::build(&raw, ph.density, ph.cp, ph.ambient_c, &actuator, &sensor, &mut errs);
        m.cranes = crane::Crane::build(&raw, &actuator, &sensor, &|v, id| conv_idx.get(&full_key(v, id)).copied(), &mut errs);
        m.photos = photos;
        m.scanners = scanners;
        m.sources = sources;
        m.stoppers = stoppers;
        errors.extend(errs);
        m.errors = errors;
        m
    }
}

// ------------------------------------------------------------------ stato e passo di simulazione

#[derive(Debug, Default, Clone, Serialize)]
pub struct Stats {
    pub spawned: u64,
    pub delivered: u64,
    pub reads: u64,
    pub noreads: u64,
    pub removed: u64,
}

struct Sim {
    model: Model,
    items: Vec<Item>,
    t: f64,
    next_id: u64,
    rng: StdRng,
    stats: Stats,
    /// Ultimo valore scritto per tag (le scritture avvengono solo sui cambi).
    written: HashMap<String, Value>,
    running: bool,
    paused: bool,
    speed: f64,
    cycle_us: u64,
    started: Option<i64>,
    write_errors: u64,
    last_error: Option<String>,
    log: Vec<(f64, String)>,
    /// Scritture da fare al prossimo ciclo (es. fattore tempo del PLC virtuale).
    pending: Vec<(String, Value)>,
}

impl Sim {
    fn new(model: Model) -> Sim {
        Sim {
            model,
            items: vec![],
            t: 0.0,
            next_id: 1,
            rng: StdRng::seed_from_u64(0x5eed),
            stats: Stats::default(),
            written: HashMap::new(),
            running: false,
            paused: false,
            speed: 1.0,
            cycle_us: 0,
            started: None,
            write_errors: 0,
            last_error: None,
            log: vec![],
            pending: vec![],
        }
    }

    fn note(&mut self, s: String) {
        self.log.push((self.t, s));
        if self.log.len() > 200 {
            self.log.remove(0);
        }
    }

    fn put(&mut self, out: &mut Vec<(String, Value)>, tag: &str, v: Value) {
        if tag.is_empty() || self.written.get(tag) == Some(&v) {
            return;
        }
        self.written.insert(tag.to_string(), v.clone());
        out.push((tag.to_string(), v));
    }

    /// Sostituisce il modello mantenendo le unità sui nastri che esistono ancora.
    fn replace_model(&mut self, model: Model) {
        let keys: Vec<String> = self.model.convs.iter().map(|c| c.key.clone()).collect();
        let map = |i: usize| keys.get(i).and_then(|k| model.convs.iter().position(|c| &c.key == k));
        let speeds: HashMap<String, (f64, bool)> = self.model.convs.iter().map(|c| (c.key.clone(), (c.v, c.failed))).collect();
        let mut items = std::mem::take(&mut self.items);
        items.retain_mut(|it| match map(it.conv) {
            Some(n) => {
                it.conv = n;
                it.prev = it.prev.and_then(map);
                it.front = it.front.min(model.convs[n].len);
                true
            }
            None => false,
        });
        self.items = items;
        let mut model = model;
        model.fl.carry_from(&self.model.fl);
        for c in &mut model.cranes {
            if let Some(o) = self.model.cranes.iter().find(|o| o.key == c.key) {
                c.carry_from(o);
            }
        }
        self.model = model;
        for c in &mut self.model.convs {
            if let Some((v, f)) = speeds.get(&c.key) {
                c.v = *v;
                c.failed = *f;
            }
        }
    }

    fn reset_items(&mut self) {
        self.items.clear();
        self.model.fl.reset();
        for c in &mut self.model.cranes {
            c.reset();
        }
        self.t = 0.0;
        self.stats = Stats::default();
        for c in &mut self.model.convs {
            c.v = 0.0;
        }
        for s in &mut self.model.sources {
            s.next_at = 0.5;
            s.spawned = 0;
        }
    }

    /// Un passo di simulazione di `dt` secondi. Legge gli attuatori da `read`,
    /// restituisce le scritture dei sensori (solo i cambi, in ordine).
    fn step(&mut self, dt: f64, read: &dyn Fn(&str) -> Value) -> Vec<(String, Value)> {
        let mut out = vec![];
        self.t += dt;
        let t = self.t;
        let b = |v: Value| v.as_bool().unwrap_or(false);

        // --- attuatori
        for c in &mut self.model.convs {
            let run = b(read(&c.run)) && !c.failed;
            let k = if c.sp.is_empty() { 1.0 } else { (read(&c.sp).as_f64().unwrap_or(0.0) / c.sp_max).clamp(0.0, 1.0) };
            let target = if run { c.vmax * k } else { 0.0 };
            if c.v < target {
                c.v = (c.v + c.acc * dt).min(target);
            } else {
                c.v = (c.v - c.dec * dt).max(target);
            }
        }
        let mut div_fb = vec![];
        for d in &mut self.model.divs {
            if d.stuck {
                continue;
            }
            let v = read(&d.pos_tag);
            let cmd = match &v {
                Value::Float(f) if f.fract() == 0.0 => format!("{}", *f as i64),
                v => v.to_string(),
            };
            if d.cmd.as_deref() != Some(cmd.as_str()) {
                d.cmd = Some(cmd);
                d.timer = d.switch_s;
                d.current = None;
            }
            if d.current.is_none() {
                d.timer -= dt;
                if d.timer <= 0.0 {
                    d.current = d.outputs.iter().position(|o| Some(&o.0) == d.cmd.as_ref());
                    if !d.fb_tag.is_empty() {
                        div_fb.push((d.fb_tag.clone(), if d.current.is_some() { read(&d.pos_tag) } else { Value::Int(0) }));
                    }
                }
            }
        }
        for (tg, v) in div_fb {
            self.put(&mut out, &tg, v);
        }
        let mut stop_fb = vec![];
        for s in &mut self.model.stoppers {
            if !s.stuck {
                let on = b(read(&s.cmd)) != s.invert;
                let rate = if s.time_s <= 0.0 { 1e9 } else { dt / s.time_s };
                s.pos = if on { (s.pos + rate).min(1.0) } else { (s.pos - rate).max(0.0) };
            }
            stop_fb.push((s.fb_ext.clone(), s.pos >= 1.0 - EPS));
            stop_fb.push((s.fb_ret.clone(), s.pos <= EPS));
        }
        for (tg, v) in stop_fb {
            self.put(&mut out, &tg, Value::Bool(v));
        }

        // --- sorgenti
        for si in 0..self.model.sources.len() {
            let s = &self.model.sources[si];
            let enabled = s.enabled.unwrap_or_else(|| s.enable.is_empty() || b(read(&s.enable)));
            if !enabled || t < s.next_at || s.max_items.is_some_and(|m| s.spawned >= m) {
                continue;
            }
            // mix dei materiali pesato con `share`
            let tot: f64 = s.mats.iter().map(|i| self.model.mats[*i].share.unwrap_or(1.0).max(0.0)).sum();
            let mut pick = if tot > 0.0 { self.rng.random_range(0.0..tot) } else { 0.0 };
            let mut mi = s.mats[0];
            for i in &s.mats {
                let w = self.model.mats[*i].share.unwrap_or(1.0).max(0.0);
                if pick < w {
                    mi = *i;
                    break;
                }
                pick -= w;
            }
            let mat = self.model.mats[mi].clone();
            let conv = s.conv;
            let gap = self.model.convs[conv].gap;
            let free = self.items.iter().filter(|i| i.conv == conv).map(|i| i.front - i.len).fold(f64::INFINITY, f64::min);
            if free < mat.length_m + gap {
                continue; // inizio nastro occupato: riprova al prossimo passo
            }
            let s = &mut self.model.sources[si];
            s.n += 1;
            s.spawned += 1;
            let code = if let Some(p) = &s.pattern {
                fill_counter(p, s.n)
            } else if s.barcodes.is_empty() {
                format!("UDC{:06}", self.next_id)
            } else {
                s.barcodes[self.rng.random_range(0..s.barcodes.len())].clone()
            };
            let gapt = if s.imax > s.imin { self.rng.random_range(s.imin..s.imax) } else { s.imin };
            s.next_at = t + gapt;
            let weight = if mat.mass_max > mat.mass_min { self.rng.random_range(mat.mass_min..mat.mass_max) } else { mat.mass_min };
            let id = self.next_id;
            self.next_id += 1;
            self.stats.spawned += 1;
            self.items.push(Item {
                id,
                conv,
                front: mat.length_m,
                len: mat.length_m,
                width: mat.width_m,
                code,
                weight: (weight * 100.0).round() / 100.0,
                material: mat.name.clone(),
                color: if mat.color.is_empty() { "#c8915a".into() } else { mat.color.clone() },
                prev: None,
                jammed: false,
                born: t,
            });
        }

        // --- movimento delle unità (dal fronte più avanzato, nastro per nastro)
        let n_conv = self.model.convs.len();
        let old: Vec<(u64, usize, f64)> = self.items.iter().map(|i| (i.id, i.conv, i.front)).collect();
        let mut moved = vec![false; self.items.len()];
        let mut removed: Vec<(usize, Next)> = vec![];
        for c in 0..n_conv {
            let mut idx: Vec<usize> = (0..self.items.len()).filter(|&i| self.items[i].conv == c && !moved[i]).collect();
            idx.sort_by(|a, b| self.items[*b].front.total_cmp(&self.items[*a].front));
            let conv = &self.model.convs[c];
            let (len, gap) = (conv.len, conv.gap);
            // code delle unità a cavallo che arrivano da questo nastro
            let straddle = self.items.iter().filter(|i| i.prev == Some(c)).map(|i| len + i.front - i.len).fold(f64::INFINITY, f64::min);
            // uscita effettiva in fondo al nastro
            let exit = match conv.next {
                Next::Div(d) => self.model.divs[d].current.map(|o| self.model.divs[d].outputs[o].1).unwrap_or(Next::None),
                n => n,
            };
            let space_next = match exit {
                Next::Conv(n) => {
                    let g = self.model.convs[n].gap;
                    Some(self.items.iter().filter(|i| i.conv == n).map(|i| i.front - i.len - g).fold(self.model.convs[n].len, f64::min))
                }
                Next::Sink(_) | Next::Out => Some(f64::INFINITY),
                _ => None,
            };
            let mut ahead = straddle - gap;
            for &i in &idx {
                let it = &self.items[i];
                let ride = match it.prev {
                    Some(p) if it.front < it.len / 2.0 => self.model.convs[p].v,
                    _ => self.model.convs[c].v,
                };
                let mut limit = ahead;
                limit = limit.min(match space_next {
                    Some(sp) => len + sp,
                    None => len,
                });
                for s in self.model.stoppers.iter().filter(|s| s.conv == c && s.pos > 0.5) {
                    if it.front <= s.at + EPS {
                        limit = limit.min(s.at);
                    }
                }
                let it = &mut self.items[i];
                if !it.jammed {
                    let want = it.front + ride * dt;
                    it.front = want.min(limit).max(it.front);
                }
                if it.prev.is_some() && it.front >= it.len {
                    it.prev = None;
                }
                ahead = it.front - it.len - gap;
                moved[i] = true;
                // passaggio al componente successivo
                match exit {
                    Next::Conv(n) if it.front > len + EPS => {
                        it.front -= len;
                        it.conv = n;
                        it.prev = Some(c);
                    }
                    Next::Sink(_) | Next::Out if it.front - it.len / 2.0 >= len => removed.push((i, exit)),
                    _ => {}
                }
            }
        }
        // sensori a passaggio: scanner (fronte che supera la posizione)
        let mut reads = vec![];
        for (k, it) in self.items.iter().enumerate() {
            let (_, oc, of) = old.iter().find(|o| o.0 == it.id).copied().unwrap_or((it.id, it.conv, it.front));
            let of = if oc != it.conv { of - self.model.convs[oc].len } else { of };
            for (si, s) in self.model.scanners.iter().enumerate() {
                if s.conv == it.conv && of < s.at && it.front >= s.at {
                    reads.push((si, k));
                }
            }
        }
        for (si, k) in reads {
            let it = self.items[k].clone();
            let s = &self.model.scanners[si];
            let noread = s.force_noread || it.code.is_empty() || (s.noread_pct > 0.0 && self.rng.random_range(0.0..100.0) < s.noread_pct);
            let code = if noread { s.noread_text.clone() } else { it.code.clone() };
            let (ct, wt, nt, ws) = (s.code_tag.clone(), s.weight_tag.clone(), s.count_tag.clone(), s.weight_scale);
            let s = &mut self.model.scanners[si];
            s.reads += 1;
            s.count = (s.count + 1) % 32768;
            s.last_code = code.clone();
            let count = s.count;
            self.stats.reads += 1;
            if noread {
                self.stats.noreads += 1;
            }
            // dati prima, contatore per ultimo (il PLC vede la lettura completa)
            self.written.remove(&ct);
            self.put(&mut out, &ct, Value::Str(code));
            if !wt.is_empty() {
                self.written.remove(&wt);
                self.put(&mut out, &wt, Value::Float((it.weight * ws * 100.0).round() / 100.0));
            }
            if !nt.is_empty() {
                self.put(&mut out, &nt, Value::Int(count));
            }
        }
        // uscite dall'impianto
        removed.sort_by_key(|r| std::cmp::Reverse(r.0));
        for (i, n) in removed {
            let it = self.items.remove(i);
            self.stats.delivered += 1;
            if let Next::Sink(k) = n {
                let s = &mut self.model.sinks[k];
                s.received += 1;
                if !s.count_tag.is_empty() {
                    s.count = (s.count + 1) % 32768;
                    let (tg, v) = (s.count_tag.clone(), s.count);
                    self.put(&mut out, &tg, Value::Int(v));
                }
                let _ = it;
            }
        }

        // --- sensori di presenza
        for p in 0..self.model.photos.len() {
            let ph = &self.model.photos[p];
            let covered = ph.stuck.unwrap_or_else(|| self.covers(ph.conv, ph.at));
            let ph = &mut self.model.photos[p];
            ph.state = covered;
            let (tg, v) = (ph.tag.clone(), covered != ph.invert);
            self.put(&mut out, &tg, Value::Bool(v));
        }
        // --- retroazione velocità
        let mut fbs = vec![];
        for c in &mut self.model.convs {
            if c.fb.is_empty() {
                continue;
            }
            let v = c.v * c.fb_scale;
            let step = (c.vmax * c.fb_scale * 0.02).max(1e-3);
            if c.last_fb.is_nan() || (v - c.last_fb).abs() >= step || (v == 0.0 && c.last_fb != 0.0) {
                c.last_fb = v;
                fbs.push((c.fb.clone(), Value::Float((v * 100.0).round() / 100.0)));
            }
        }
        for (tg, v) in fbs {
            self.put(&mut out, &tg, v);
        }
        // --- fluidi
        for (tg, v) in self.model.fl.step(dt, read) {
            self.put(&mut out, &tg, v);
        }
        // --- trasloelevatori
        for ci in 0..self.model.cranes.len() {
            let mut w = vec![];
            if let Some(tr) = self.model.cranes[ci].step(ci, dt, read, &mut w) {
                self.crane_transfer(tr);
            }
            for (tg, v) in w {
                self.put(&mut out, &tg, v);
            }
        }
        out
    }

    /// Prelievo o deposito di un traslo al P&D: prende l'unità in fondo al nastro, o la
    /// mette all'inizio del nastro (lato 1 = nastro di ingresso, lato 2 = di uscita).
    fn crane_transfer(&mut self, tr: crane::Transfer) {
        let c = &self.model.cranes[tr.crane];
        let Some(conv) = (if tr.side == 0 { c.pd_in } else { c.pd_out }) else { return };
        let len = self.model.convs[conv].len;
        if tr.pick {
            let at_end = self.items.iter().position(|i| i.conv == conv && i.front >= len - 0.05 && !i.jammed);
            match at_end {
                Some(k) => {
                    let it = self.items.remove(k);
                    let cr = &mut self.model.cranes[tr.crane];
                    cr.load_dims = (it.len, it.width, it.color.clone(), it.material.clone(), it.weight);
                    cr.complete(true, true, it.code, "");
                }
                None => self.model.cranes[tr.crane].complete(true, false, String::new(), ""),
            }
        } else {
            let (l, w, color, material, weight) = self.model.cranes[tr.crane].load_dims.clone();
            let free = self.items.iter().filter(|i| i.conv == conv).map(|i| i.front - i.len).fold(f64::INFINITY, f64::min);
            if free < l {
                self.model.cranes[tr.crane].complete(false, false, String::new(), "deposito sul P&D di uscita occupato");
                return;
            }
            let code = self.model.cranes[tr.crane].load_code.clone();
            let id = self.next_id;
            self.next_id += 1;
            self.items.push(Item { id, conv, front: l, len: l, width: w, code, weight, material, color, prev: None, jammed: false, born: self.t });
            self.model.cranes[tr.crane].complete(false, true, String::new(), "");
        }
        let _ = tr.col;
    }

    /// Un punto del nastro `c` è occupato da un'unità (anche a cavallo con il successivo)?
    fn covers(&self, c: usize, x: f64) -> bool {
        let len = self.model.convs[c].len;
        self.items.iter().any(|i| {
            (i.conv == c && i.front - i.len <= x + EPS && x <= i.front + EPS) || (i.prev == Some(c) && i.front < i.len && x >= len - (i.len - i.front) - EPS)
        })
    }

    /// Posizione di un'unità sulla vista: (vista, x, y, angolo, lunghezza px, larghezza px).
    fn pose(&self, it: &Item) -> (String, f64, f64, f64, f64, f64) {
        let mut c = &self.model.convs[it.conv];
        let mut s = it.front - it.len / 2.0;
        if s < 0.0
            && let Some(p) = it.prev
        {
            c = &self.model.convs[p];
            s += c.len;
        }
        let (x, y) = c.geo.at(s);
        (c.view.clone(), x, y, c.geo.angle, it.len * c.geo.ppm, it.width * c.geo.ppm)
    }

    fn frame(&self) -> J {
        let mut views: BTreeMap<String, Vec<J>> = BTreeMap::new();
        for it in &self.items {
            let (v, x, y, a, l, w) = self.pose(it);
            let r = |f: f64| (f * 10.0).round() / 10.0;
            views.entry(v).or_default().push(json!([it.id, r(x), r(y), r(a), r(l), r(w), it.color, it.jammed, it.code]));
        }
        let mut flows: BTreeMap<String, Vec<J>> = BTreeMap::new();
        self.model.fl.flows(&mut flows);
        let mut cranes: BTreeMap<String, Vec<J>> = BTreeMap::new();
        for c in &self.model.cranes {
            c.frame(&mut cranes);
        }
        json!({"running": self.running, "paused": self.paused, "t": (self.t * 10.0).round() / 10.0, "views": views, "flows": flows, "cranes": cranes})
    }

    fn status(&self) -> J {
        let m = &self.model;
        let conv_key = |i: usize| m.convs.get(i).map(|c| c.key.clone()).unwrap_or_default();
        let next_s = |n: Next| match n {
            Next::Conv(i) => conv_key(i),
            Next::Div(i) => m.divs[i].key.clone(),
            Next::Sink(i) => m.sinks[i].key.clone(),
            Next::Out => "(uscita)".into(),
            Next::None => String::new(),
        };
        let mut comps = vec![];
        for c in &m.convs {
            let n = self.items.iter().filter(|i| m.convs.get(i.conv).is_some_and(|x| x.key == c.key)).count();
            comps.push(json!({"key": c.key, "kind": "conveyor", "len": c.len, "vmax": c.vmax, "v": (c.v * 1000.0).round() / 1000.0, "failed": c.failed, "items": n, "next": next_s(c.next), "run": c.run}));
        }
        for p in &m.photos {
            comps.push(json!({"key": p.key, "kind": "photocell", "on": conv_key(p.conv), "at": p.at, "state": p.state, "stuck": p.stuck, "tag": p.tag}));
        }
        for s in &m.scanners {
            comps.push(json!({"key": s.key, "kind": "scanner", "on": conv_key(s.conv), "at": s.at, "reads": s.reads, "last": s.last_code, "noread": s.force_noread, "noread_pct": s.noread_pct}));
        }
        for d in &m.divs {
            comps.push(json!({"key": d.key, "kind": "diverter", "cmd": d.cmd, "current": d.current.map(|o| d.outputs[o].0.clone()), "stuck": d.stuck,
                "outputs": d.outputs.iter().map(|(v, n)| json!({"value": v, "next": next_s(*n)})).collect::<Vec<_>>()}));
        }
        for s in &m.sources {
            comps.push(json!({"key": s.key, "kind": "source", "on": conv_key(s.conv), "spawned": s.spawned, "interval": [s.imin, s.imax], "enabled": s.enabled,
                "materials": s.mats.iter().map(|i| m.mats[*i].name.clone()).collect::<Vec<_>>()}));
        }
        for s in &m.sinks {
            comps.push(json!({"key": s.key, "kind": "sink", "received": s.received}));
        }
        for s in &m.stoppers {
            comps.push(json!({"key": s.key, "kind": "stopper", "on": conv_key(s.conv), "at": s.at, "pos": s.pos, "stuck": s.stuck}));
        }
        m.fl.status(&mut comps);
        for c in &m.cranes {
            c.status(&mut comps);
        }
        let items: Vec<J> = self
            .items
            .iter()
            .map(|i| json!({"id": i.id, "code": i.code, "material": i.material, "weight": i.weight, "on": conv_key(i.conv), "front": (i.front * 1000.0).round() / 1000.0, "jammed": i.jammed, "age": ((self.t - i.born) * 10.0).round() / 10.0}))
            .collect();
        json!({
            "running": self.running, "paused": self.paused, "t": self.t, "speed": self.speed, "cycle_us": self.cycle_us, "started": self.started,
            "errors": m.errors, "components": comps, "items": items, "stats": self.stats, "materials": m.mats,
            "write_errors": self.write_errors, "last_error": self.last_error,
            "log": self.log.iter().rev().take(50).map(|(t, s)| json!({"t": (t * 10.0).round() / 10.0, "text": s})).collect::<Vec<_>>(),
        })
    }
}

fn fill_counter(p: &str, n: u64) -> String {
    if let Some(i) = p.find("{n")
        && let Some(j) = p[i..].find('}')
    {
        let spec = &p[i + 2..i + j];
        let width: usize = spec.trim_start_matches(':').parse().unwrap_or(0);
        return format!("{}{:0width$}{}", &p[..i], n, &p[i + j + 1..], width = width);
    }
    p.to_string()
}

// ------------------------------------------------------------------ servizio

/// Motore fisico condiviso (stato dietro un mutex sincrono: il passo è breve).
pub struct Physics {
    sim: Mutex<Option<Sim>>,
    dirty: AtomicBool,
}

impl Default for Physics {
    fn default() -> Self {
        Self { sim: Mutex::new(None), dirty: AtomicBool::new(true) }
    }
}

impl Physics {
    /// Viste o progetto cambiati: il modello verrà ricompilato al prossimo passo.
    pub fn invalidate(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    fn ensure(&self, app: &AppState) {
        if !self.dirty.swap(false, Ordering::Relaxed) && self.sim.lock().unwrap().is_some() {
            return;
        }
        let model = Model::build(&app.cfg(), &app.db, &app.views_dir);
        let mut g = self.sim.lock().unwrap();
        match g.as_mut() {
            Some(s) => s.replace_model(model),
            None => *g = Some(Sim::new(model)),
        }
    }

    pub fn status(&self, app: &AppState) -> J {
        self.ensure(app);
        self.sim.lock().unwrap().as_ref().map(Sim::status).unwrap_or(J::Null)
    }

    pub fn running(&self) -> bool {
        self.sim.lock().unwrap().as_ref().is_some_and(|s| s.running)
    }

    /// Comandi: start, stop, pause, resume, reset, speed.
    pub fn command(&self, app: &AppState, cmd: &str, arg: &J) -> anyhow::Result<String> {
        self.ensure(app);
        let ts_tag = app.cfg().project.physics.time_scale_tag.clone();
        let mut g = self.sim.lock().unwrap();
        let s = g.as_mut().unwrap();
        let msg = match cmd {
            "start" => {
                if !s.model.errors.is_empty() && s.model.convs.is_empty() && s.model.fl.is_empty() && s.model.cranes.is_empty() {
                    anyhow::bail!("modello fisico vuoto o non valido: {}", s.model.errors.join("; "));
                }
                if s.running {
                    anyhow::bail!("la simulazione è già in corso");
                }
                if !ts_tag.is_empty() {
                    s.pending.push((ts_tag.clone(), Value::Int((s.speed * 100.0).round() as i64)));
                }
                s.running = true;
                s.paused = false;
                s.written.clear(); // al primo passo scrive tutti i sensori
                s.started = Some(crate::tagdb::now_ms());
                "simulazione avviata".to_string()
            }
            "stop" => {
                s.running = false;
                s.paused = false;
                s.started = None;
                "simulazione arrestata".to_string()
            }
            "pause" => {
                s.paused = true;
                "simulazione in pausa".to_string()
            }
            "resume" => {
                s.paused = false;
                "simulazione ripresa".to_string()
            }
            "reset" => {
                s.reset_items();
                "unità rimosse, tempo azzerato".to_string()
            }
            "speed" => {
                let f = arg.get("factor").and_then(J::as_f64).unwrap_or(1.0);
                if !(0.1..=20.0).contains(&f) {
                    anyhow::bail!("fattore tempo fuori dai limiti (0,1–20)");
                }
                s.speed = f;
                if !ts_tag.is_empty() {
                    s.pending.push((ts_tag.clone(), Value::Int((f * 100.0).round() as i64)));
                }
                format!("fattore tempo ×{f}")
            }
            _ => anyhow::bail!("comando sconosciuto: {cmd}"),
        };
        s.note(msg.clone());
        Ok(msg)
    }

    /// Guasti e interventi: jam/unjam/remove (unità), photocell, conveyor, diverter,
    /// scanner, stopper, source, spawn, clear.
    pub fn inject(&self, app: &AppState, req: &J) -> anyhow::Result<String> {
        self.ensure(app);
        let mut g = self.sim.lock().unwrap();
        let s = g.as_mut().unwrap();
        let action = req.get("action").and_then(J::as_str).unwrap_or_default();
        let key = req.get("key").and_then(J::as_str).unwrap_or_default();
        let flag = |k: &str| req.get(k).and_then(J::as_bool).unwrap_or(false);
        let item = req.get("item").and_then(J::as_u64);
        let find = |v: &[String]| v.iter().position(|k| k == key).ok_or_else(|| anyhow::anyhow!("componente '{key}' non trovato"));
        let m = &mut s.model;
        let msg = match action {
            "jam" | "unjam" | "remove" => {
                let id = item.ok_or_else(|| anyhow::anyhow!("unità non indicata"))?;
                let i = s.items.iter().position(|x| x.id == id).ok_or_else(|| anyhow::anyhow!("unità {id} non trovata"))?;
                match action {
                    "jam" => s.items[i].jammed = true,
                    "unjam" => s.items[i].jammed = false,
                    _ => {
                        s.items.remove(i);
                        s.stats.removed += 1;
                    }
                }
                format!("unità {id}: {}", match action { "jam" => "incastrata", "unjam" => "liberata", _ => "rimossa" })
            }
            "photocell" => {
                let i = find(&m.photos.iter().map(|x| x.key.clone()).collect::<Vec<_>>())?;
                m.photos[i].stuck = req.get("stuck").and_then(J::as_bool);
                format!("{key}: {}", match m.photos[i].stuck { Some(true) => "bloccata occupata", Some(false) => "bloccata libera", None => "normale" })
            }
            "conveyor" => {
                let i = find(&m.convs.iter().map(|x| x.key.clone()).collect::<Vec<_>>())?;
                m.convs[i].failed = flag("failed");
                format!("{key}: {}", if m.convs[i].failed { "motore in avaria" } else { "motore ripristinato" })
            }
            "diverter" => {
                let i = find(&m.divs.iter().map(|x| x.key.clone()).collect::<Vec<_>>())?;
                m.divs[i].stuck = flag("stuck");
                format!("{key}: {}", if m.divs[i].stuck { "bloccato" } else { "sbloccato" })
            }
            "scanner" => {
                let i = find(&m.scanners.iter().map(|x| x.key.clone()).collect::<Vec<_>>())?;
                m.scanners[i].force_noread = flag("noread");
                if let Some(p) = req.get("noread_pct").and_then(J::as_f64) {
                    m.scanners[i].noread_pct = p.clamp(0.0, 100.0);
                }
                format!("{key}: {}", if m.scanners[i].force_noread { "nessuna lettura (guasto)" } else { "lettura normale" })
            }
            "stopper" => {
                let i = find(&m.stoppers.iter().map(|x| x.key.clone()).collect::<Vec<_>>())?;
                m.stoppers[i].stuck = flag("stuck");
                format!("{key}: {}", if m.stoppers[i].stuck { "bloccato" } else { "sbloccato" })
            }
            "source" => {
                let i = find(&m.sources.iter().map(|x| x.key.clone()).collect::<Vec<_>>())?;
                let src = &mut m.sources[i];
                src.enabled = req.get("enabled").and_then(J::as_bool);
                if let Some(a) = req.get("interval_min").and_then(J::as_f64) {
                    src.imin = a.max(0.05);
                    src.imax = src.imax.max(src.imin);
                }
                if let Some(b) = req.get("interval_max").and_then(J::as_f64) {
                    src.imax = b.max(src.imin);
                }
                format!("{key}: {} · ogni {}–{} s", match src.enabled { Some(true) => "forzata attiva", Some(false) => "ferma", None => "automatica" }, src.imin, src.imax)
            }
            "spawn" => {
                let i = find(&m.sources.iter().map(|x| x.key.clone()).collect::<Vec<_>>())?;
                let src = &mut m.sources[i];
                src.next_at = s.t;
                let code = req.get("code").and_then(J::as_str).unwrap_or_default().to_string();
                if !code.is_empty() {
                    src.barcodes = vec![code];
                    src.pattern = None;
                }
                format!("{key}: nuova unità richiesta")
            }
            "clear" => {
                s.reset_items();
                "tutte le unità rimosse".into()
            }
            "crane" => {
                let c = m.cranes.iter_mut().find(|c| c.key_matches(key)).ok_or_else(|| anyhow::anyhow!("traslo '{key}' non trovato"))?;
                c.inject(req)?
            }
            other => match m.fl.inject(other, key, req) {
                Some(r) => r?,
                None => anyhow::bail!("intervento sconosciuto: {action}"),
            },
        };
        s.note(msg.clone());
        Ok(msg)
    }
}

/// Task della simulazione: passo fisso, scritture dei sensori in un task separato
/// (in ordine), heartbeat e attivo per il PLC, fotogrammi per l'HMI.
pub async fn run(app: Arc<AppState>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<(String, Value)>();
    {
        let app = app.clone();
        tokio::spawn(async move {
            while let Some((tag, v)) = rx.recv().await {
                if let Err(e) = app.db.write_ext(&tag, v, true).await {
                    let mut g = app.physics.sim.lock().unwrap();
                    if let Some(s) = g.as_mut() {
                        s.write_errors += 1;
                        s.last_error = Some(format!("{tag}: {e:#}"));
                    }
                }
            }
        });
    }
    app.physics.ensure(&app);
    if app.cfg().project.physics.autostart {
        let _ = app.physics.command(&app, "start", &J::Null);
    }
    let mut last = Instant::now();
    let mut last_frame = Instant::now();
    let mut last_hb = Instant::now();
    let mut hb: i64 = 0;
    let mut was_running = false;
    loop {
        let ph: PhysicsCfg = app.cfg().project.physics.clone();
        tokio::time::sleep(Duration::from_millis(ph.step_ms.max(5))).await;
        app.physics.ensure(&app);
        let now = Instant::now();
        let real_dt = (now - last).as_secs_f64().min(0.25);
        last = now;
        let mut writes = vec![];
        let mut frame = None;
        {
            let mut g = app.physics.sim.lock().unwrap();
            let Some(s) = g.as_mut() else { continue };
            let running = s.running;
            if running && !s.paused {
                let t0 = Instant::now();
                let db = app.db.clone();
                let read = move |t: &str| if t.is_empty() { Value::Bool(false) } else { db.value(t) };
                // passi interni da al massimo 20 ms simulati (stabilità ad alto fattore tempo)
                let mut left = real_dt * s.speed;
                while left > EPS {
                    let dt = left.min(0.02);
                    writes.extend(s.step(dt, &read));
                    left -= dt;
                }
                s.cycle_us = t0.elapsed().as_micros() as u64;
            }
            writes.append(&mut s.pending);
            if running != was_running {
                for t in tag_list(&ph.active_tag) {
                    writes.push((t, Value::Bool(running)));
                }
                was_running = running;
                frame = Some(s.frame());
            }
            if running && now - last_hb >= Duration::from_millis(500) {
                last_hb = now;
                hb = (hb + 1) % 32768;
                for t in tag_list(&ph.heartbeat_tag) {
                    writes.push((t, Value::Int(hb)));
                }
                // riscritto a ogni heartbeat: un PLC riavviato (o aggiunto dopo l'avvio) lo riceve
                for t in tag_list(&ph.active_tag) {
                    writes.push((t, Value::Bool(true)));
                }
            }
            if running && now - last_frame >= Duration::from_millis(100) {
                last_frame = now;
                frame = Some(s.frame());
            }
        }
        for w in writes {
            let _ = tx.send(w);
        }
        if let Some(f) = frame {
            let _ = app.events.send(Event::Physics(f));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(key: &str, len: f64, next: Next) -> Conv {
        Conv {
            key: key.into(),
            view: "v".into(),
            len,
            vmax: 1.0,
            acc: 100.0,
            dec: 100.0,
            run: format!("{key}.Run"),
            sp: String::new(),
            sp_max: 100.0,
            fb: String::new(),
            fb_scale: 1.0,
            next,
            gap: 0.0,
            geo: Geo { ppm: 100.0, ux: 1.0, ..Default::default() },
            v: 0.0,
            failed: false,
            last_fb: f64::NAN,
        }
    }

    fn sim() -> Sim {
        let mut m = Model { mats: vec![default_material()], ..Default::default() };
        // A (2 m) → deviatore → B (uscita "1") / C (uscita "2"); B e C escono a pozzi
        m.convs = vec![conv("A", 2.0, Next::Div(0)), conv("B", 1.0, Next::Sink(0)), conv("C", 1.0, Next::Out)];
        m.divs = vec![Div { key: "D".into(), pos_tag: "D.Pos".into(), outputs: vec![("1".into(), Next::Conv(1)), ("2".into(), Next::Conv(2))], switch_s: 0.2, fb_tag: String::new(), cmd: None, timer: 0.0, current: None, stuck: false }];
        m.sinks = vec![Sink { key: "S".into(), count_tag: "S.Count".into(), count: 0, received: 0 }];
        m.photos = vec![Photo { key: "PE_END".into(), conv: 0, at: 1.9, tag: "PE_END".into(), invert: false, stuck: None, state: false }];
        m.scanners = vec![Scanner { key: "SC".into(), conv: 0, at: 1.0, code_tag: "SC.Code".into(), count_tag: "SC.Count".into(), weight_tag: String::new(), weight_scale: 1.0, noread_pct: 0.0, noread_text: "NOREAD".into(), force_noread: false, count: 0, reads: 0, last_code: String::new() }];
        m.sources = vec![Source { key: "SRC".into(), conv: 0, imin: 100.0, imax: 100.0, mats: vec![0], barcodes: vec![], pattern: Some("PKG{n:3}".into()), enable: String::new(), max_items: None, next_at: 0.0, n: 0, spawned: 0, enabled: None }];
        let mut s = Sim::new(m);
        s.running = true;
        s
    }

    fn run_for(s: &mut Sim, secs: f64, tags: &HashMap<&str, Value>, out: &mut Vec<(String, Value)>) {
        let read = |t: &str| tags.get(t).cloned().unwrap_or(Value::Bool(false));
        let mut left = secs;
        while left > 1e-9 {
            out.extend(s.step(0.01, &read));
            left -= 0.01;
        }
    }

    #[test]
    fn transport_scan_divert_and_sink() {
        let mut s = sim();
        let mut tags: HashMap<&str, Value> = HashMap::new();
        let mut out = vec![];
        // nastro fermo: l'unità resta all'inizio
        run_for(&mut s, 1.0, &tags, &mut out);
        assert_eq!(s.items.len(), 1);
        assert!((s.items[0].front - 0.4).abs() < 1e-6);
        // marcia A, deviatore non posizionato: l'unità si ferma in fondo ad A
        tags.insert("A.Run", Value::Bool(true));
        run_for(&mut s, 3.0, &tags, &mut out);
        assert!((s.items[0].front - 2.0).abs() < 1e-6, "fermo in fondo: {}", s.items[0].front);
        assert!(out.contains(&("SC.Code".into(), Value::Str("PKG001".into()))), "barcode letto");
        assert!(out.contains(&("SC.Count".into(), Value::Int(1))));
        assert!(out.contains(&("PE_END".into(), Value::Bool(true))));
        // posizione 1 → B (fermo: l'unità resta a cavallo, la fotocellula resta occupata)
        tags.insert("D.Pos", Value::Int(1));
        run_for(&mut s, 0.5, &tags, &mut out);
        // B ferma: A spinge il collo finché metà è su B, poi si blocca a cavallo
        assert_eq!(s.items[0].conv, 1);
        assert!((s.items[0].front - 0.2).abs() < 0.02, "a cavallo: {}", s.items[0].front);
        assert!(s.covers(0, 1.95), "coda ancora su A");
        tags.insert("B.Run", Value::Bool(true));
        run_for(&mut s, 0.1, &tags, &mut out);
        assert!(s.items[0].front > 0.25, "riparte con B");
        run_for(&mut s, 2.0, &tags, &mut out);
        assert!(s.items.is_empty(), "uscito al pozzo");
        assert_eq!(s.model.sinks[0].received, 1);
        assert!(out.contains(&("S.Count".into(), Value::Int(1))));
        assert_eq!(out.iter().rev().find(|(t, _)| t == "PE_END").unwrap().1, Value::Bool(false));
    }

    #[test]
    fn accumulation_without_overlap_and_jam() {
        let mut s = sim();
        s.model.sources[0].imin = 0.1;
        s.model.sources[0].imax = 0.1;
        let mut tags: HashMap<&str, Value> = HashMap::new();
        tags.insert("A.Run", Value::Bool(true));
        let mut out = vec![];
        run_for(&mut s, 10.0, &tags, &mut out);
        // 2 m di nastro, unità da 0,4 m: al massimo 5 in accumulo, nessuna sovrapposizione
        let mut f: Vec<f64> = s.items.iter().map(|i| i.front).collect();
        f.sort_by(|a, b| b.total_cmp(a));
        assert_eq!(f.len(), 5, "{f:?}");
        for w in f.windows(2) {
            assert!(w[0] - w[1] >= 0.4 - 1e-6, "sovrapposizione {f:?}");
        }
        // unità incastrata: le successive si fermano dietro
        s.reset_items();
        s.model.sources[0].imin = 1.0;
        s.model.sources[0].imax = 1.0;
        run_for(&mut s, 0.6, &tags, &mut out);
        let id = s.items[0].id;
        s.items[0].jammed = true;
        let pos = s.items[0].front;
        run_for(&mut s, 3.0, &tags, &mut out);
        let it = s.items.iter().find(|i| i.id == id).unwrap();
        assert!((it.front - pos).abs() < 1e-9);
        assert!(s.items.iter().all(|i| i.id == id || i.front <= pos - 0.4 + 1e-6));
    }

    #[test]
    fn counter_pattern() {
        assert_eq!(fill_counter("PKG-{n:5}", 42), "PKG-00042");
        assert_eq!(fill_counter("X{n}Y", 7), "X7Y");
    }
}
