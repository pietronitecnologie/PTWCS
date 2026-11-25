//! Magazzino automatico (AS/RS) – modulo WCS: locazioni, anagrafica UdC,
//! strategie di stoccaggio (ABC, distribuzione articoli), missioni con priorità,
//! ordini di prelievo FIFO, riorganizzazione (defrag) e KPI dei trasloelevatori.
//!
//! Geometria: corsie × lati × colonne × livelli. Colonna 0 = stazione P&D
//! (pick & deposit) in testa alla corsia. Ogni corsia ha un traslo collegato a
//! un'istanza UDT con il telegramma missione:
//!
//! | membro                       | scritto da | significato                           |
//! |------------------------------|------------|---------------------------------------|
//! | Mission (u32)                | WCS        | id missione                           |
//! | Type (u16)                   | WCS        | 1 deposito, 2 prelievo, 3 movimentazione cella→cella |
//! | SrcSide/SrcCol/SrcLvl (u16)  | WCS        | origine (col 0 = P&D)                 |
//! | DstSide/DstCol/DstLvl (u16)  | WCS        | destinazione (col 0 = P&D)            |
//! | DstOut (u16)                 | WCS        | baia d'uscita (prelievi)              |
//! | Start (bool)                 | WCS        | avvio; azzerato dal WCS quando Busy=1 |
//! | Busy (bool)                  | PLC        | missione in esecuzione                |
//! | Done (u32)                   | PLC        | id dell'ultima missione completata    |
//! | Fault (bool)                 | PLC        | traslo in guasto (non riceve missioni)|
//!
//! Traslo a **doppia UdC** (`slots: 2`): stessi campi con suffisso `2` per il
//! secondo posto (`Mission2`, `Type2`, `Src…2`, `Dst…2`, `DstOut2`, `Done2`).
//!
//! **Priorità**: 1 (bassa) … 9 (urgente). Il dispatcher sceglie la missione in
//! coda con priorità effettiva più alta (priorità + 1 punto ogni 2 minuti di
//! attesa, contro l'attesa indefinita), salta quelle sospese (`hold`).
//! Le movimentazioni di riorganizzazione hanno priorità 1.
//!
//! **Zone ABC**: le celle di ogni corsia sono ordinate per tempo di
//! traslazione dal P&D; le prime `a_zone_pct` sono zona A, le successive
//! `b_zone_pct` zona B, il resto zona C. Gli articoli sono classificati per
//! frequenza di prelievo (finestra `abc_window_h`): il primo `a_sku_pct` è
//! classe A, ecc. Una UdC di classe A va nella cella libera più vicina in zona
//! A, poi B, poi C; una di classe C nella zona C più vicina, poi B, poi A.
//!
//! **Riorganizzazione**: nei tempi morti di una corsia il WCS crea missioni di
//! movimentazione (tipo 3) calcolate su una simulazione del magazzino:
//! 1. promozione: UdC in una zona peggiore della propria classe → zona giusta;
//! 2. liberazione: se la zona giusta è piena, sposta fuori una UdC di classe
//!    inferiore che la occupa;
//! 3. compattazione: avvicina al P&D le UdC (stessa zona) lasciando i buchi in
//!    fondo, se il guadagno supera `min_gain_s`.

use crate::config::WarehouseDef;
use crate::store::Store;
use crate::tagdb::{TagDb, now_ms};
use crate::value::Value;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const PRIO_DEFAULT: u8 = 5;
pub const PRIO_MOVE: u8 = 1;
const KPI_WINDOW_MS: i64 = 3_600_000;
/// Pesi della frequenza d'accesso attesa per classe A/B/C (calcolo del costo).
const CLASS_W: [f64; 3] = [3.0, 2.0, 1.0];
const ZONES: [&str; 3] = ["A", "B", "C"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocState {
    Free,
    Occupied,
    /// prenotata da una missione di deposito/movimentazione in corso
    Reserved,
    /// in uscita: occupata ma con missione di prelievo/movimentazione in corso
    Picking,
    /// bloccata dall'operatore
    Blocked,
}

impl LocState {
    fn code(self) -> &'static str {
        match self {
            LocState::Free => "free",
            LocState::Occupied => "occupied",
            LocState::Reserved => "reserved",
            LocState::Picking => "picking",
            LocState::Blocked => "blocked",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "occupied" => LocState::Occupied,
            "reserved" => LocState::Reserved,
            "picking" => LocState::Picking,
            "blocked" => LocState::Blocked,
            _ => LocState::Free,
        }
    }
}

/// Matrice [livello][colonna].
pub type Grid<T> = Vec<Vec<T>>;

/// Cella del prospetto: stato e UdC.
pub type Cell = (LocState, Option<String>);

#[derive(Debug, Clone, Serialize)]
pub struct Loc {
    pub state: LocState,
    pub lu: Option<String>,
    pub ts: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pos {
    pub aisle: u16,
    pub side: u16,
    pub col: u16,
    pub lvl: u16,
}

impl Pos {
    pub fn code(&self) -> String {
        if self.col == 0 {
            format!("C{:02}-P&D", self.aisle)
        } else {
            format!("C{:02}-L{}-{:02}-{:02}", self.aisle, self.side, self.col, self.lvl)
        }
    }
}

/// Anagrafica dell'unità di carico.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LuInfo {
    #[serde(default)]
    pub sku: String,
    #[serde(default)]
    pub qty: f64,
    #[serde(default)]
    pub lot: String,
    #[serde(default)]
    pub height_mm: u16,
    #[serde(default)]
    pub weight_kg: f64,
    /// ingresso in magazzino (ms): base del FIFO
    #[serde(default)]
    pub entry: i64,
}

impl LuInfo {
    /// Dai campi del telegramma di ingresso; nomi riconosciuti senza distinzione
    /// di maiuscole: sku/article/articolo, qty/quantity/qta, lot/lotto,
    /// height/altezza, weight/peso.
    pub fn from_fields(f: &HashMap<String, Value>) -> Self {
        let get = |names: &[&str]| f.iter().find(|(k, _)| names.iter().any(|n| k.eq_ignore_ascii_case(n))).map(|(_, v)| v);
        let s = |names: &[&str]| get(names).filter(|v| !matches!(v, Value::Null)).map(|v| v.to_string().trim().to_string()).unwrap_or_default();
        let n = |names: &[&str]| get(names).and_then(|v| v.as_f64()).unwrap_or(0.0);
        LuInfo {
            sku: s(&["sku", "article", "articolo", "item"]),
            qty: n(&["qty", "quantity", "qta", "quantita"]),
            lot: s(&["lot", "lotto", "batch"]),
            height_mm: n(&["height", "altezza", "h"]).clamp(0.0, 65535.0) as u16,
            weight_kg: n(&["weight", "peso"]),
            entry: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MissionKind {
    In,
    Out,
    /// movimentazione cella → cella nella stessa corsia (riorganizzazione)
    Move,
}

impl MissionKind {
    fn type_code(self) -> i64 {
        match self {
            MissionKind::In => 1,
            MissionKind::Out => 2,
            MissionKind::Move => 3,
        }
    }
    fn slot(self) -> usize {
        self.type_code() as usize - 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MissionState {
    Queued,
    Sent,
    Running,
    Done,
    Cancelled,
}

fn prio_default() -> u8 {
    PRIO_DEFAULT
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: u32,
    pub kind: MissionKind,
    pub lu: String,
    pub aisle: u16,
    pub src: Pos,
    pub dst: Pos,
    /// ingresso di provenienza (depositi), baia d'uscita (prelievi) o motivo (movimentazioni)
    pub bay: String,
    pub state: MissionState,
    /// Posto UdC sul traslo (1 o 2) quando inviata.
    #[serde(default)]
    pub slot: u8,
    /// 1 (bassa) … 9 (urgente)
    #[serde(default = "prio_default")]
    pub priority: u8,
    /// sospesa dall'operatore (resta in coda ma non viene inviata)
    #[serde(default)]
    pub hold: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sku: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    pub created: i64,
    pub started: Option<i64>,
    pub finished: Option<i64>,
}

impl Mission {
    fn open(&self) -> bool {
        matches!(self.state, MissionState::Queued | MissionState::Sent | MissionState::Running)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderLine {
    pub sku: String,
    pub qty: f64,
    #[serde(default)]
    pub allocated: f64,
    #[serde(default)]
    pub picked: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderState {
    /// in attesa di giacenza (righe non coperte, nessuna missione aperta)
    Waiting,
    /// missioni di prelievo in corso
    Released,
    Done,
    /// chiuso con mancanze
    Partial,
    Cancelled,
}

impl OrderState {
    fn code(self) -> &'static str {
        match self {
            OrderState::Waiting => "waiting",
            OrderState::Released => "released",
            OrderState::Done => "done",
            OrderState::Partial => "partial",
            OrderState::Cancelled => "cancelled",
        }
    }
    fn closed(self) -> bool {
        matches!(self, OrderState::Done | OrderState::Partial | OrderState::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: u32,
    /// riferimento esterno (WMS/ERP)
    #[serde(default)]
    pub reference: String,
    pub bay: String,
    pub priority: u8,
    pub lines: Vec<OrderLine>,
    pub state: OrderState,
    #[serde(default)]
    pub missions: Vec<u32>,
    pub created: i64,
    pub finished: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderReq {
    #[serde(default)]
    pub reference: String,
    #[serde(default)]
    pub bay: Option<String>,
    #[serde(default)]
    pub priority: Option<u8>,
    pub lines: Vec<OrderLineReq>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OrderLineReq {
    pub sku: String,
    pub qty: f64,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SideSummary {
    pub free: u32,
    pub occupied: u32,
    pub reserved: u32,
    pub blocked: u32,
    /// saturazione per [livello][blocco di colonne] (0..1), livello 0 = basso
    pub blocks: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AisleSummary {
    pub name: String,
    pub crane: String,
    pub sides: Vec<SideSummary>,
    pub queued: u32,
    pub active: Option<Mission>,
    /// secondo posto UdC (traslo a doppia UdC)
    pub active2: Option<Mission>,
    pub slots: u8,
    pub done_hour: u32,
}

/// Mossa di riorganizzazione pianificata.
#[derive(Debug, Clone, Serialize)]
pub struct PlannedMove {
    pub lu: String,
    pub sku: String,
    pub class: String,
    pub from: Pos,
    pub to: Pos,
    pub from_code: String,
    pub to_code: String,
    /// riduzione del tempo di traslazione (s, sola andata) per accesso
    pub gain_s: f64,
    pub reason: String,
}

/// Indicatori di qualità di una corsia (o del magazzino).
#[derive(Debug, Clone, Serialize, Default)]
pub struct LayoutMetrics {
    pub occupied: u32,
    pub free: u32,
    /// celle libere dentro la regione compatta ideale / celle occupate (0 = compatto)
    pub frag_index: f64,
    /// UdC fuori dalla zona della propria classe
    pub misplaced: u32,
    /// costo pesato: Σ peso(classe) × tempo di traslazione (s)
    pub cost: f64,
    /// costo ideale (UdC ordinate per classe sulle celle più vicine)
    pub ideal_cost: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReorgPlan {
    pub moves: Vec<PlannedMove>,
    pub before: LayoutMetrics,
    pub after: LayoutMetrics,
    pub aisles: Vec<(u16, LayoutMetrics, LayoutMetrics, usize)>,
}

struct DoneRec {
    ts: i64,
    aisle: u16,
    kind: MissionKind,
}

struct CycleRec {
    aisle: u16,
    t0: i64,
    t1: i64,
    n: u8,
}

pub struct Warehouse {
    pub def: WarehouseDef,
    st: Mutex<State>,
    store: Store,
    /// zona ABC di ogni cella (0 = A)
    zone: Vec<u8>,
    /// celle di ogni corsia ordinate per tempo di traslazione dal P&D
    by_travel: Vec<Vec<usize>>,
    opened: i64,
}

struct State {
    locs: Vec<Loc>,
    lus: HashMap<String, usize>,
    info: HashMap<String, LuInfo>,
    missions: VecDeque<Mission>,
    /// missioni inviate/in corso per corsia (una per posto UdC)
    active: HashMap<u16, Vec<u32>>,
    next_id: u32,
    orders: Vec<Order>,
    next_order: u32,
    done_log: VecDeque<DoneRec>,
    cycles: VecDeque<CycleRec>,
    cycle_start: HashMap<u16, (i64, u8)>,
    faults: VecDeque<(u16, i64, i64)>,
    fault_since: HashMap<u16, i64>,
    out_rr: usize,
    /// classi ABC per articolo (0 = A)
    abc: HashMap<String, u8>,
    picks: VecDeque<(i64, String)>,
    reorg_enabled: bool,
    last_activity: HashMap<u16, i64>,
    moves_log: VecDeque<(i64, u16)>,
}

/// Generatore pseudo-casuale deterministico (riempimento demo).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn unit(&mut self) -> f64 {
        (self.next() % 1_000_000) as f64 / 1_000_000.0
    }
}

impl Warehouse {
    pub fn total(&self) -> usize {
        let d = &self.def;
        d.aisles.len() * d.sides as usize * d.columns as usize * d.levels as usize
    }

    fn per_aisle(&self) -> usize {
        let d = &self.def;
        d.sides as usize * d.columns as usize * d.levels as usize
    }

    fn idx(&self, p: &Pos) -> Option<usize> {
        let d = &self.def;
        if p.aisle == 0 || p.aisle as usize > d.aisles.len() || p.side == 0 || p.side > d.sides || p.col == 0 || p.col > d.columns || p.lvl == 0 || p.lvl > d.levels {
            return None;
        }
        Some(((((p.aisle - 1) as usize * d.sides as usize + (p.side - 1) as usize) * d.columns as usize + (p.col - 1) as usize) * d.levels as usize) + (p.lvl - 1) as usize)
    }

    fn pos(&self, i: usize) -> Pos {
        let d = &self.def;
        let lvl = i % d.levels as usize;
        let r = i / d.levels as usize;
        let col = r % d.columns as usize;
        let r = r / d.columns as usize;
        let side = r % d.sides as usize;
        let aisle = r / d.sides as usize;
        Pos { aisle: aisle as u16 + 1, side: side as u16 + 1, col: col as u16 + 1, lvl: lvl as u16 + 1 }
    }

    fn aisle_of(&self, i: usize) -> u16 {
        (i / self.per_aisle()) as u16 + 1
    }

    /// Tempo di traslazione stimato dal P&D (moto contemporaneo dei due assi).
    fn travel(&self, p: &Pos) -> f64 {
        (p.col as f64 * self.def.col_time_s).max(p.lvl as f64 * self.def.lvl_time_s)
    }

    fn travel_i(&self, i: usize) -> f64 {
        self.travel(&self.pos(i))
    }

    /// La cella accetta una UdC di quell'altezza?
    fn fits(&self, i: usize, height: u16) -> bool {
        if height == 0 {
            return true;
        }
        let lvl = self.pos(i).lvl as usize;
        self.def.level_max_height_mm.get(lvl - 1).is_none_or(|&m| m == 0 || height <= m)
    }

    fn abc_on(&self) -> bool {
        self.def.strategy.abc
    }

    /// Classe ABC di un articolo (articolo senza storico = C; nessun articolo = nessuna classe).
    fn class_of(&self, st: &State, sku: &str) -> Option<u8> {
        if !self.abc_on() || sku.is_empty() {
            return None;
        }
        Some(*st.abc.get(sku).unwrap_or(&2))
    }

    fn lu_class(&self, st: &State, lu: &str) -> Option<u8> {
        st.info.get(lu).and_then(|i| self.class_of(st, &i.sku))
    }

    /// Ordine di preferenza delle zone per una classe.
    fn zone_prefs(class: Option<u8>) -> [u8; 3] {
        match class {
            Some(0) | None => [0, 1, 2],
            Some(1) => [1, 2, 0],
            _ => [2, 1, 0],
        }
    }

    /// Cella libera migliore di una corsia per la classe/altezza date.
    fn best_free(&self, st: &State, aisle: u16, class: Option<u8>, height: u16) -> Option<usize> {
        let order = &self.by_travel[aisle as usize - 1];
        let ok = |i: usize| st.locs[i].state == LocState::Free && self.fits(i, height);
        if class.is_none() {
            return order.iter().copied().find(|&i| ok(i));
        }
        for z in Self::zone_prefs(class) {
            if let Some(i) = order.iter().copied().find(|&i| self.zone[i] == z && ok(i)) {
                return Some(i);
            }
        }
        None
    }

    /// Carica locazioni, anagrafica, missioni e ordini; se il magazzino è nuovo
    /// applica il riempimento iniziale demo.
    pub async fn open(def: WarehouseDef, store: Store) -> Result<Arc<Self>> {
        let mut wh = Self {
            st: Mutex::new(State {
                locs: vec![],
                lus: HashMap::new(),
                info: HashMap::new(),
                missions: VecDeque::new(),
                active: HashMap::new(),
                next_id: 1,
                orders: vec![],
                next_order: 1,
                done_log: VecDeque::new(),
                cycles: VecDeque::new(),
                cycle_start: HashMap::new(),
                faults: VecDeque::new(),
                fault_since: HashMap::new(),
                out_rr: 0,
                abc: HashMap::new(),
                picks: VecDeque::new(),
                reorg_enabled: def.reorg.enabled,
                last_activity: HashMap::new(),
                moves_log: VecDeque::new(),
            }),
            def,
            store,
            zone: vec![],
            by_travel: vec![],
            opened: now_ms(),
        };
        let total = wh.total();
        // zone ABC per tempo di traslazione (stessa geometria per tutte le corsie)
        let per = wh.per_aisle();
        let (za, zb) = (wh.def.strategy.a_zone_pct.clamp(0.0, 1.0), wh.def.strategy.b_zone_pct.clamp(0.0, 1.0));
        let mut zone = vec![2u8; total];
        let mut by_travel = vec![];
        for a in 0..wh.def.aisles.len() {
            let mut cells: Vec<usize> = (a * per..(a + 1) * per).collect();
            cells.sort_by(|&x, &y| wh.travel_i(x).total_cmp(&wh.travel_i(y)).then(x.cmp(&y)));
            for (rank, &i) in cells.iter().enumerate() {
                let f = rank as f64 / per.max(1) as f64;
                zone[i] = if f < za { 0 } else if f < za + zb { 1 } else { 2 };
            }
            by_travel.push(cells);
        }
        wh.zone = zone;
        wh.by_travel = by_travel;

        let name = wh.def.name.clone();
        let mut locs = vec![Loc { state: LocState::Free, lu: None, ts: now_ms() }; total];
        let saved = wh.store.wh_load_locs(name.clone()).await?;
        let fresh = saved.is_empty();
        for (i, state, lu, ts) in saved {
            if (i as usize) < total {
                locs[i as usize] = Loc { state: LocState::parse(&state), lu, ts };
            }
        }
        let missions = wh.store.wh_load_missions(name.clone()).await?;
        let next_id = wh.store.wh_max_mission(name.clone()).await? + 1;
        let infos = wh.store.wh_load_lus(name.clone()).await?;
        let orders = wh.store.wh_load_orders(name.clone(), 50).await?;
        let next_order = wh.store.wh_max_order(name.clone()).await? + 1;
        let since = now_ms() - wh.def.strategy.abc_window_h as i64 * 3_600_000;
        let picks = wh.store.wh_load_picks(name.clone(), since).await?;
        let mut new_lus = vec![];
        let mut new_picks = vec![];
        {
            let mut st = wh.st.lock().unwrap();
            st.locs = locs;
            st.next_id = next_id;
            st.next_order = next_order;
            for (lu, d) in infos {
                if let Ok(i) = serde_json::from_str::<LuInfo>(&d) {
                    st.info.insert(lu, i);
                }
            }
            let mut orders: Vec<Order> = orders.iter().filter_map(|d| serde_json::from_str(d).ok()).collect();
            orders.sort_by_key(|o| o.id);
            st.orders = orders;
            let mut picks = picks;
            picks.sort_by_key(|p| p.1);
            st.picks = picks.into_iter().map(|(s, t)| (t, s)).collect();
            for m in missions {
                // missioni interrotte: le reinviamo (lo stato della cella è già prenotato)
                let mut m = m;
                if matches!(m.state, MissionState::Sent | MissionState::Running) {
                    m.state = MissionState::Queued;
                    m.slot = 0;
                }
                st.missions.push_back(m);
            }
            if fresh && wh.def.initial_fill_pct > 0 {
                let mut rng = Rng(0x2545F4914F6CDD1D);
                let nsku = wh.def.demo_skus;
                let mut n = 0;
                for i in 0..total {
                    if (rng.next() % 100) < wh.def.initial_fill_pct as u64 {
                        n += 1;
                        let lu = format!("{}{:06}", wh.def.lu_prefix, n);
                        st.locs[i] = Loc { state: LocState::Occupied, lu: Some(lu.clone()), ts: now_ms() };
                        if nsku > 0 {
                            // giacenza poco sbilanciata, posizione casuale (nessuna strategia: materiale per la riorganizzazione)
                            let k = ((rng.unit().powf(1.4)) * nsku as f64) as u32 + 1;
                            let maxh = wh.def.level_max_height_mm.get(wh.pos(i).lvl as usize - 1).copied().filter(|&m| m > 0).unwrap_or(1700);
                            let info = LuInfo {
                                sku: format!("SKU{k:03}"),
                                qty: (10 + rng.next() % 90) as f64,
                                lot: format!("L{}", 2600 + rng.next() % 40),
                                height_mm: (900 + rng.next() % 800).min(maxh as u64) as u16,
                                weight_kg: (150 + rng.next() % 800) as f64,
                                entry: now_ms() - (rng.next() % (60 * 86_400_000)) as i64,
                            };
                            new_lus.push((lu.clone(), serde_json::to_string(&info).unwrap_or_default()));
                            st.info.insert(lu, info);
                        }
                    }
                }
                // storico prelievi simulato (distribuzione di Pareto: pochi articoli fanno molti prelievi)
                if nsku > 0 {
                    let win = (wh.def.strategy.abc_window_h as i64 * 3_600_000).max(3_600_000);
                    for _ in 0..(nsku * 25) {
                        let k = ((rng.unit().powf(3.0)) * nsku as f64) as u32 + 1;
                        let ts = now_ms() - (rng.next() % win as u64) as i64;
                        new_picks.push((format!("SKU{k:03}"), ts));
                        st.picks.push_back((ts, format!("SKU{k:03}")));
                    }
                    st.picks.make_contiguous().sort_by_key(|p| p.0);
                }
            }
            let lus: HashMap<String, usize> =
                st.locs.iter().enumerate().filter_map(|(i, l)| l.lu.clone().map(|u| (u, i))).collect();
            st.lus = lus;
            // le UdC in uscita (Picking) mappano la cella d'origine
            let moving: Vec<(String, usize)> = st.locs.iter().enumerate().filter(|(_, l)| l.state == LocState::Picking).filter_map(|(i, l)| l.lu.clone().map(|u| (u, i))).collect();
            for (u, i) in moving {
                st.lus.insert(u, i);
            }
        }
        wh.refresh_abc();
        if fresh {
            let rows: Vec<_> = {
                let st = wh.st.lock().unwrap();
                st.locs.iter().enumerate().filter(|(_, l)| l.state != LocState::Free).map(|(i, l)| (i as u32, l.state.code().to_string(), l.lu.clone(), l.ts)).collect()
            };
            wh.store.wh_save_locs(name.clone(), rows).await?;
            if !new_lus.is_empty() {
                wh.store.wh_save_lus(name.clone(), new_lus).await?;
            }
            if !new_picks.is_empty() {
                wh.store.wh_add_picks(name.clone(), new_picks).await?;
            }
            tracing::info!("magazzino {}: {} locazioni create", name, total);
        }
        Ok(Arc::new(wh))
    }

    // ------------------------------------------------------------ persistenza

    fn persist_loc(&self, i: usize, l: &Loc) {
        let s = self.store.clone();
        let (wh, st, lu, ts) = (self.def.name.clone(), l.state.code().to_string(), l.lu.clone(), l.ts);
        tokio::spawn(async move {
            let _ = s.wh_save_locs(wh, vec![(i as u32, st, lu, ts)]).await;
        });
    }

    fn persist_mission(&self, m: &Mission) {
        let s = self.store.clone();
        let (wh, m) = (self.def.name.clone(), m.clone());
        tokio::spawn(async move {
            let _ = s.wh_save_mission(wh, m).await;
        });
    }

    fn persist_lu(&self, lu: &str, info: Option<&LuInfo>) {
        let s = self.store.clone();
        let (wh, lu, d) = (self.def.name.clone(), lu.to_string(), info.and_then(|i| serde_json::to_string(i).ok()));
        tokio::spawn(async move {
            let _ = s.wh_save_lu(wh, lu, d).await;
        });
    }

    fn persist_order(&self, o: &Order) {
        let s = self.store.clone();
        let (wh, id, state, d) = (self.def.name.clone(), o.id, o.state.code().to_string(), serde_json::to_string(o).unwrap_or_default());
        tokio::spawn(async move {
            let _ = s.wh_save_order(wh, id, state, d).await;
        });
    }

    fn set_loc(&self, st: &mut State, i: usize, state: LocState, lu: Option<String>) {
        if let Some(old) = st.locs[i].lu.take()
            && st.lus.get(&old) == Some(&i)
        {
            st.lus.remove(&old);
        }
        if let Some(u) = &lu {
            st.lus.insert(u.clone(), i);
        }
        st.locs[i] = Loc { state, lu, ts: now_ms() };
        self.persist_loc(i, &st.locs[i]);
    }

    fn pd(aisle: u16) -> Pos {
        Pos { aisle, side: 0, col: 0, lvl: 0 }
    }

    #[allow(clippy::too_many_arguments)]
    fn push_mission(&self, st: &mut State, kind: MissionKind, lu: &str, aisle: u16, src: Pos, dst: Pos, bay: &str, priority: u8, order: Option<u32>, note: &str) -> Mission {
        let id = st.next_id;
        st.next_id += 1;
        let m = Mission {
            id,
            kind,
            lu: lu.into(),
            aisle,
            src,
            dst,
            bay: bay.into(),
            state: MissionState::Queued,
            slot: 0,
            priority: priority.clamp(1, 9),
            hold: false,
            order,
            sku: st.info.get(lu).map(|i| i.sku.clone()).unwrap_or_default(),
            note: note.into(),
            created: now_ms(),
            started: None,
            finished: None,
        };
        if kind != MissionKind::Move {
            st.last_activity.insert(aisle, now_ms());
        }
        self.persist_mission(&m);
        st.missions.push_back(m.clone());
        m
    }

    // ------------------------------------------------------------ depositi

    /// Deposito senza anagrafica (compatibilità): vedi [`Self::allocate_inbound_with`].
    pub fn allocate_inbound(&self, lu: &str, bay: &str, blocked_aisles: &[u16], dry_run: bool) -> Result<(u16, Pos)> {
        self.allocate_inbound_with(lu, bay, LuInfo::default(), blocked_aisles, dry_run)
    }

    /// Deposito: sceglie corsia e cella secondo la strategia, prenota la cella e
    /// accoda la missione. `blocked_aisles`: corsie con traslo in guasto/manuale.
    pub fn allocate_inbound_with(&self, lu: &str, bay: &str, mut info: LuInfo, blocked_aisles: &[u16], dry_run: bool) -> Result<(u16, Pos)> {
        let lu = lu.trim();
        if lu.is_empty() || lu == "NOREAD" {
            bail!("identificativo UdC mancante");
        }
        let mut st = self.st.lock().unwrap();
        if st.lus.contains_key(lu) || st.missions.iter().any(|m| m.lu == lu && m.open()) {
            bail!("UdC {lu} già presente in magazzino");
        }
        if let Some(maxh) = self.def.level_max_height_mm.iter().copied().filter(|&m| m > 0).max()
            && info.height_mm > maxh
        {
            bail!("UdC {lu} troppo alta ({} mm > {} mm)", info.height_mm, maxh);
        }
        let class = self.class_of(&st, &info.sku);
        // conteggio dell'articolo per corsia (distribuzione su più corsie)
        let mut sku_cnt = vec![0u32; self.def.aisles.len() + 1];
        if self.def.strategy.spread_sku && !info.sku.is_empty() {
            for (u, i) in st.info.iter() {
                if i.sku == info.sku
                    && let Some(&c) = st.lus.get(u)
                {
                    sku_cnt[self.aisle_of(c) as usize] += 1;
                }
            }
        }
        // corsia: punteggio = missioni in coda + 0.5 × UdC dello stesso articolo − saturazione libera
        let mut best: Option<(u16, f64, usize)> = None;
        for a in 1..=self.def.aisles.len() as u16 {
            if blocked_aisles.contains(&a) {
                continue;
            }
            let Some(cell) = self.best_free(&st, a, class, info.height_mm) else { continue };
            let free = self.by_travel[a as usize - 1].iter().filter(|&&i| st.locs[i].state == LocState::Free).count();
            let q = st.missions.iter().filter(|m| m.aisle == a && m.open() && m.kind != MissionKind::Move).count();
            let score = q as f64 + 0.5 * sku_cnt[a as usize] as f64 - 0.5 * free as f64 / self.per_aisle().max(1) as f64
                // a parità, preferisce la corsia con la cella nella zona migliore per la classe
                + 0.01 * Self::zone_prefs(class).iter().position(|&z| z == self.zone[cell]).unwrap_or(0) as f64;
            if best.is_none_or(|(_, bs, _)| score < bs - 1e-9) {
                best = Some((a, score, cell));
            }
        }
        let (aisle, _, i) = best.ok_or_else(|| anyhow!("magazzino pieno o corsie non disponibili"))?;
        let dst = self.pos(i);
        if dry_run {
            return Ok((aisle, dst));
        }
        self.set_loc(&mut st, i, LocState::Reserved, Some(lu.to_string()));
        info.entry = now_ms();
        self.persist_lu(lu, Some(&info));
        st.info.insert(lu.to_string(), info);
        let m = self.push_mission(&mut st, MissionKind::In, lu, aisle, Self::pd(aisle), dst, bay, PRIO_DEFAULT, None, "");
        tracing::info!(wh = %self.def.name, "deposito {lu} → {} zona {} (missione {})", dst.code(), ZONES[self.zone[i] as usize], m.id);
        Ok((aisle, dst))
    }

    // ------------------------------------------------------------ prelievi

    fn pick_bay(&self, st: &mut State, bay: Option<&str>) -> Result<String> {
        match bay {
            Some(b) if self.def.outputs.iter().any(|o| o == b) => Ok(b.to_string()),
            Some(b) => bail!("baia d'uscita sconosciuta: {b}"),
            None => {
                if self.def.outputs.is_empty() {
                    bail!("nessuna baia d'uscita configurata");
                }
                let b = self.def.outputs[st.out_rr % self.def.outputs.len()].clone();
                st.out_rr += 1;
                Ok(b)
            }
        }
    }

    fn out_mission(&self, st: &mut State, i: usize, bay: &str, priority: u8, order: Option<u32>) -> Mission {
        let src = self.pos(i);
        let lu = st.locs[i].lu.clone().unwrap_or_default();
        self.set_loc(st, i, LocState::Picking, Some(lu.clone()));
        self.push_mission(st, MissionKind::Out, &lu, src.aisle, src, Self::pd(src.aisle), bay, priority, order, "")
    }

    /// Prelievo di una UdC (o di una a caso se `lu` è None) verso una baia d'uscita.
    pub fn create_outbound(&self, lu: Option<&str>, bay: Option<&str>, blocked_aisles: &[u16]) -> Result<Mission> {
        self.create_outbound_prio(lu, bay, blocked_aisles, PRIO_DEFAULT)
    }

    pub fn create_outbound_prio(&self, lu: Option<&str>, bay: Option<&str>, blocked_aisles: &[u16], priority: u8) -> Result<Mission> {
        let mut st = self.st.lock().unwrap();
        let i = match lu {
            Some(u) => *st.lus.get(u.trim()).ok_or_else(|| anyhow!("UdC {u} non trovata"))?,
            None => {
                let cands: Vec<usize> = (0..st.locs.len()).filter(|&i| st.locs[i].state == LocState::Occupied && !blocked_aisles.contains(&self.aisle_of(i))).collect();
                if cands.is_empty() {
                    bail!("nessuna UdC prelevabile");
                }
                cands[(now_ms() as usize / 7) % cands.len()]
            }
        };
        if st.locs[i].state != LocState::Occupied {
            bail!("locazione {} non prelevabile (stato {:?})", self.pos(i).code(), st.locs[i].state);
        }
        let bay = self.pick_bay(&mut st, bay)?;
        Ok(self.out_mission(&mut st, i, &bay, priority, None))
    }

    // ------------------------------------------------------------ movimentazioni

    fn move_mission(&self, st: &mut State, from: usize, to: usize, note: &str) -> Result<Mission> {
        if st.locs[from].state != LocState::Occupied {
            bail!("origine {} non movimentabile ({:?})", self.pos(from).code(), st.locs[from].state);
        }
        if st.locs[to].state != LocState::Free {
            bail!("destinazione {} non libera ({:?})", self.pos(to).code(), st.locs[to].state);
        }
        let (src, dst) = (self.pos(from), self.pos(to));
        if src.aisle != dst.aisle {
            bail!("movimentazione solo nella stessa corsia");
        }
        let lu = st.locs[from].lu.clone().unwrap_or_default();
        if !self.fits(to, st.info.get(&lu).map(|i| i.height_mm).unwrap_or(0)) {
            bail!("UdC {lu} troppo alta per {}", dst.code());
        }
        // destinazione prima, origine per ultima: la UdC resta mappata dov'è fisicamente
        self.set_loc(st, to, LocState::Reserved, Some(lu.clone()));
        self.set_loc(st, from, LocState::Picking, Some(lu.clone()));
        let m = self.push_mission(st, MissionKind::Move, &lu, src.aisle, src, dst, "riorg", PRIO_MOVE, None, note);
        Ok(m)
    }

    /// Movimentazione manuale di una UdC; senza destinazione sceglie la cella migliore della corsia.
    pub fn create_move(&self, lu: &str, dst: Option<Pos>) -> Result<Mission> {
        let mut st = self.st.lock().unwrap();
        let from = *st.lus.get(lu.trim()).ok_or_else(|| anyhow!("UdC {lu} non trovata"))?;
        let aisle = self.aisle_of(from);
        let to = match dst {
            Some(p) => self.idx(&p).ok_or_else(|| anyhow!("destinazione non valida"))?,
            None => {
                let class = self.lu_class(&st, lu.trim());
                let h = st.info.get(lu.trim()).map(|i| i.height_mm).unwrap_or(0);
                self.best_free(&st, aisle, class, h).ok_or_else(|| anyhow!("nessuna cella libera nella corsia {aisle}"))?
            }
        };
        let mut m = self.move_mission(&mut st, from, to, "manuale")?;
        if let Some(x) = st.missions.iter_mut().find(|x| x.id == m.id) {
            x.priority = PRIO_DEFAULT;
            m = x.clone();
        }
        self.persist_mission(&m);
        Ok(m)
    }

    // ------------------------------------------------------------ gestione missioni

    /// Rilascia le celle di una missione non eseguita.
    fn undo_cells(&self, st: &mut State, m: &Mission) {
        match m.kind {
            MissionKind::In => {
                if let Some(i) = self.idx(&m.dst) {
                    self.set_loc(st, i, LocState::Free, None);
                }
                st.info.remove(&m.lu);
                self.persist_lu(&m.lu, None);
            }
            MissionKind::Out => {
                if let Some(i) = self.idx(&m.src) {
                    self.set_loc(st, i, LocState::Occupied, Some(m.lu.clone()));
                }
            }
            MissionKind::Move => {
                if let Some(i) = self.idx(&m.dst) {
                    self.set_loc(st, i, LocState::Free, None);
                }
                if let Some(i) = self.idx(&m.src) {
                    self.set_loc(st, i, LocState::Occupied, Some(m.lu.clone()));
                }
            }
        }
    }

    pub fn cancel(&self, id: u32) -> Result<()> {
        self.mission_action(id, "cancel", None).map(|_| ())
    }

    /// Azioni operatore: `hold`, `release`, `priority` (con `priority`), `cancel`
    /// (solo in coda), `requeue` (missione inviata ma persa dal PLC: torna in
    /// coda), `done` (conferma manuale dell'esecuzione).
    pub fn mission_action(&self, id: u32, action: &str, priority: Option<u8>) -> Result<Mission> {
        let mut st = self.st.lock().unwrap();
        let k = st.missions.iter().position(|m| m.id == id).ok_or_else(|| anyhow!("missione {id} non trovata"))?;
        let state = st.missions[k].state;
        match action {
            "hold" | "release" => {
                if state != MissionState::Queued {
                    bail!("solo le missioni in coda possono essere sospese/rilasciate");
                }
                st.missions[k].hold = action == "hold";
            }
            "priority" => {
                let p = priority.ok_or_else(|| anyhow!("priorità mancante"))?;
                st.missions[k].priority = p.clamp(1, 9);
            }
            "cancel" => {
                if state != MissionState::Queued {
                    bail!("solo le missioni in coda possono essere annullate");
                }
                let mut m = st.missions.remove(k).unwrap();
                self.undo_cells(&mut st, &m);
                m.state = MissionState::Cancelled;
                m.finished = Some(now_ms());
                self.persist_mission(&m);
                if let Some(oid) = m.order {
                    self.order_mission_lost(&mut st, oid, &m);
                }
                return Ok(m);
            }
            "requeue" => {
                if !matches!(state, MissionState::Sent | MissionState::Running) {
                    bail!("solo le missioni inviate possono essere rimesse in coda");
                }
                let aisle = st.missions[k].aisle;
                if let Some(v) = st.active.get_mut(&aisle) {
                    v.retain(|x| *x != id);
                    if v.is_empty() {
                        st.active.remove(&aisle);
                        st.cycle_start.remove(&aisle);
                    }
                }
                let m = &mut st.missions[k];
                m.state = MissionState::Queued;
                m.slot = 0;
                m.started = None;
            }
            "done" => {
                let mut m = st.missions[k].clone();
                drop(st);
                self.complete(m.aisle, id);
                m.state = MissionState::Done;
                m.finished = Some(now_ms());
                return Ok(m);
            }
            _ => bail!("azione sconosciuta: {action} (hold, release, priority, cancel, requeue, done)"),
        }
        let m = st.missions[k].clone();
        self.persist_mission(&m);
        Ok(m)
    }

    // ------------------------------------------------------------ ordini

    /// Nuovo ordine di prelievo: le righe sono coperte con UdC intere dell'articolo
    /// in ordine FIFO (data di ingresso), poi per vicinanza al P&D.
    pub fn create_order(&self, req: OrderReq, blocked_aisles: &[u16]) -> Result<Order> {
        if req.lines.is_empty() {
            bail!("ordine senza righe");
        }
        for l in &req.lines {
            if l.sku.trim().is_empty() || l.qty <= 0.0 {
                bail!("riga non valida: articolo e quantità > 0 obbligatori");
            }
        }
        let mut st = self.st.lock().unwrap();
        let bay = self.pick_bay(&mut st, req.bay.as_deref().filter(|b| !b.is_empty()))?;
        let id = st.next_order;
        st.next_order += 1;
        let o = Order {
            id,
            reference: if req.reference.trim().is_empty() { format!("ORD{id:05}") } else { req.reference.trim().to_string() },
            bay,
            priority: req.priority.unwrap_or(PRIO_DEFAULT).clamp(1, 9),
            lines: req.lines.iter().map(|l| OrderLine { sku: l.sku.trim().to_string(), qty: l.qty, allocated: 0.0, picked: 0.0 }).collect(),
            state: OrderState::Waiting,
            missions: vec![],
            created: now_ms(),
            finished: None,
        };
        st.orders.push(o);
        let k = st.orders.len() - 1;
        self.allocate_order(&mut st, k, blocked_aisles);
        let o = st.orders[k].clone();
        tracing::info!(wh = %self.def.name, "ordine {} ({}): {} righe, {} missioni", o.id, o.reference, o.lines.len(), o.missions.len());
        Ok(o)
    }

    fn allocate_order(&self, st: &mut State, k: usize, blocked_aisles: &[u16]) {
        let (oid, bay, prio) = (st.orders[k].id, st.orders[k].bay.clone(), st.orders[k].priority);
        for li in 0..st.orders[k].lines.len() {
            let (sku, need) = {
                let l = &st.orders[k].lines[li];
                (l.sku.clone(), l.qty - l.allocated)
            };
            if need <= 1e-9 {
                continue;
            }
            let mut cands: Vec<(i64, f64, usize, f64)> = st
                .info
                .iter()
                .filter(|(_, i)| i.sku == sku)
                .filter_map(|(u, i)| st.lus.get(u).map(|&c| (i.entry, self.travel_i(c), c, i.qty)))
                .filter(|&(_, _, c, _)| st.locs[c].state == LocState::Occupied && !blocked_aisles.contains(&self.aisle_of(c)))
                .collect();
            cands.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
            let mut got = 0.0;
            for (_, _, c, q) in cands {
                if got >= need - 1e-9 {
                    break;
                }
                let m = self.out_mission(st, c, &bay, prio, Some(oid));
                got += q.max(1.0);
                st.orders[k].missions.push(m.id);
            }
            st.orders[k].lines[li].allocated += got;
        }
        self.update_order_state(st, k);
    }

    fn update_order_state(&self, st: &mut State, k: usize) {
        let o = &st.orders[k];
        if o.state.closed() {
            return;
        }
        let open = st.missions.iter().filter(|m| m.order == Some(o.id) && m.open()).count();
        let short = o.lines.iter().any(|l| l.allocated < l.qty - 1e-9);
        let new = if open > 0 {
            OrderState::Released
        } else if short {
            OrderState::Waiting
        } else {
            OrderState::Done
        };
        if new != o.state {
            let o = &mut st.orders[k];
            o.state = new;
            if new.closed() {
                o.finished = Some(now_ms());
                tracing::info!(wh = %self.def.name, "ordine {} ({}) evaso", o.id, o.reference);
            }
        }
        let o = st.orders[k].clone();
        self.persist_order(&o);
        self.trim_orders(st);
    }

    fn trim_orders(&self, st: &mut State) {
        let closed = st.orders.iter().filter(|o| o.state.closed()).count();
        if closed > 50 {
            let mut drop = closed - 50;
            st.orders.retain(|o| {
                if drop > 0 && o.state.closed() {
                    drop -= 1;
                    false
                } else {
                    true
                }
            });
        }
    }

    /// Una missione dell'ordine è stata annullata: la quantità torna da coprire.
    fn order_mission_lost(&self, st: &mut State, oid: u32, m: &Mission) {
        let qty = st.info.get(&m.lu).map(|i| i.qty.max(1.0)).unwrap_or(1.0);
        if let Some(k) = st.orders.iter().position(|o| o.id == oid) {
            if let Some(l) = st.orders[k].lines.iter_mut().find(|l| l.sku == m.sku) {
                l.allocated = (l.allocated - qty).max(0.0);
            }
            self.update_order_state(st, k);
        }
    }

    /// Ritenta la copertura degli ordini in attesa (nuova giacenza in ingresso).
    pub fn retry_orders(&self, blocked_aisles: &[u16]) {
        let mut st = self.st.lock().unwrap();
        for k in 0..st.orders.len() {
            if matches!(st.orders[k].state, OrderState::Waiting | OrderState::Released) && st.orders[k].lines.iter().any(|l| l.allocated < l.qty - 1e-9) {
                self.allocate_order(&mut st, k, blocked_aisles);
            }
        }
    }

    /// Azioni sull'ordine: `cancel` (annulla le missioni in coda), `close`
    /// (chiude con mancanze, le missioni in corso proseguono), `priority`.
    pub fn order_action(&self, id: u32, action: &str, priority: Option<u8>) -> Result<Order> {
        let mut st = self.st.lock().unwrap();
        let k = st.orders.iter().position(|o| o.id == id).ok_or_else(|| anyhow!("ordine {id} non trovato"))?;
        if st.orders[k].state.closed() {
            bail!("ordine già chiuso");
        }
        match action {
            "cancel" | "close" => {
                let queued: Vec<u32> = st.missions.iter().filter(|m| m.order == Some(id) && m.state == MissionState::Queued).map(|m| m.id).collect();
                if action == "cancel" {
                    for mid in queued {
                        if let Some(p) = st.missions.iter().position(|m| m.id == mid) {
                            let mut m = st.missions.remove(p).unwrap();
                            self.undo_cells(&mut st, &m);
                            m.state = MissionState::Cancelled;
                            m.finished = Some(now_ms());
                            self.persist_mission(&m);
                        }
                    }
                }
                let o = &mut st.orders[k];
                o.state = if action == "cancel" { OrderState::Cancelled } else { OrderState::Partial };
                o.finished = Some(now_ms());
            }
            "priority" => {
                let p = priority.ok_or_else(|| anyhow!("priorità mancante"))?.clamp(1, 9);
                st.orders[k].priority = p;
                for m in st.missions.iter_mut().filter(|m| m.order == Some(id) && m.state == MissionState::Queued) {
                    m.priority = p;
                }
            }
            _ => bail!("azione sconosciuta: {action} (cancel, close, priority)"),
        }
        let o = st.orders[k].clone();
        self.persist_order(&o);
        Ok(o)
    }

    pub fn orders(&self) -> Vec<Order> {
        let st = self.st.lock().unwrap();
        let mut v = st.orders.clone();
        v.sort_by(|a, b| a.state.closed().cmp(&b.state.closed()).then(b.id.cmp(&a.id)));
        v
    }

    // ------------------------------------------------------------ ABC

    /// Ricalcola le classi ABC dalle frequenze di prelievo nella finestra.
    pub fn refresh_abc(&self) {
        let mut st = self.st.lock().unwrap();
        let since = now_ms() - self.def.strategy.abc_window_h as i64 * 3_600_000;
        while st.picks.front().is_some_and(|p| p.0 < since) {
            st.picks.pop_front();
        }
        let mut freq: HashMap<String, u32> = HashMap::new();
        for (_, s) in &st.picks {
            *freq.entry(s.clone()).or_default() += 1;
        }
        // articoli a giacenza mai prelevati: in coda alla classifica
        for i in st.info.values() {
            if !i.sku.is_empty() {
                freq.entry(i.sku.clone()).or_insert(0);
            }
        }
        let mut v: Vec<(String, u32)> = freq.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let n = v.len() as f64;
        let (pa, pb) = (self.def.strategy.a_sku_pct, self.def.strategy.b_sku_pct);
        st.abc = v
            .iter()
            .enumerate()
            .map(|(r, (s, c))| {
                let f = r as f64 / n.max(1.0);
                let cl = if *c == 0 { 2 } else if f < pa { 0 } else if f < pa + pb { 1 } else { 2 };
                (s.clone(), cl)
            })
            .collect();
    }

    fn record_pick(&self, st: &mut State, sku: &str) {
        if sku.is_empty() {
            return;
        }
        let ts = now_ms();
        st.picks.push_back((ts, sku.to_string()));
        let s = self.store.clone();
        let (wh, sku) = (self.def.name.clone(), sku.to_string());
        tokio::spawn(async move {
            let _ = s.wh_add_picks(wh, vec![(sku, ts)]).await;
        });
    }

    // ------------------------------------------------------------ riorganizzazione

    /// Indicatori di una corsia su uno stato (reale o simulato).
    fn metrics_of(&self, aisle: u16, cells: &[VCell]) -> LayoutMetrics {
        let order = &self.by_travel[aisle as usize - 1];
        let mut m = LayoutMetrics::default();
        let mut classes = vec![];
        for (k, &i) in order.iter().enumerate() {
            match cells[k] {
                VCell::Free => m.free += 1,
                VCell::Lu { class, .. } | VCell::Locked { class } => {
                    m.occupied += 1;
                    let w = class.map(|c| CLASS_W[c as usize]).unwrap_or(1.0);
                    m.cost += w * self.travel_i(i);
                    if let Some(c) = class
                        && self.zone[i] != c
                    {
                        m.misplaced += 1;
                    }
                    classes.push(class.map(|c| c as usize).unwrap_or(3));
                }
                VCell::Blocked => {}
            }
        }
        // regione compatta ideale: le prime `occupied` celle utilizzabili
        let usable: Vec<usize> = (0..order.len()).filter(|&k| !matches!(cells[k], VCell::Blocked)).collect();
        let holes = usable.iter().take(m.occupied as usize).filter(|&&k| matches!(cells[k], VCell::Free)).count();
        m.frag_index = if m.occupied > 0 { holes as f64 / m.occupied as f64 } else { 0.0 };
        classes.sort();
        m.ideal_cost = classes.iter().zip(usable.iter()).map(|(&c, &k)| CLASS_W.get(c).copied().unwrap_or(1.0) * self.travel_i(order[k])).sum();
        m
    }

    fn snapshot(&self, st: &State, aisle: u16) -> Vec<VCell> {
        self.by_travel[aisle as usize - 1]
            .iter()
            .map(|&i| {
                let l = &st.locs[i];
                let class = l.lu.as_deref().and_then(|u| self.lu_class(st, u));
                match l.state {
                    LocState::Free => VCell::Free,
                    LocState::Occupied => VCell::Lu {
                        class,
                        h: l.lu.as_deref().and_then(|u| st.info.get(u)).map(|i| i.height_mm).unwrap_or(0),
                    },
                    LocState::Reserved | LocState::Picking => VCell::Locked { class },
                    LocState::Blocked if l.lu.is_some() => VCell::Locked { class },
                    LocState::Blocked => VCell::Blocked,
                }
            })
            .collect()
    }

    /// Prossima mossa su uno stato simulato (indici locali in ordine di traslazione).
    fn next_move(&self, aisle: u16, cells: &[VCell]) -> Option<(usize, usize, String)> {
        let order = &self.by_travel[aisle as usize - 1];
        let n = order.len();
        let z = |k: usize| self.zone[order[k]];
        let t = |k: usize| self.travel_i(order[k]);
        let free_in = |zone: Option<u8>, h: u16, before: usize| -> Option<usize> {
            (0..before.min(n)).find(|&f| matches!(cells[f], VCell::Free) && zone.is_none_or(|zz| z(f) == zz) && self.fits(order[f], h))
        };
        if self.abc_on() {
            // 1) promozione delle UdC in una zona peggiore della propria classe (A prima, le più lontane prima)
            let mut bad: Vec<(u8, usize, u16)> = (0..n)
                .filter_map(|k| match cells[k] {
                    VCell::Lu { class: Some(c), h } if z(k) > c => Some((c, k, h)),
                    _ => None,
                })
                .collect();
            bad.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
            for &(c, k, h) in &bad {
                if let Some(f) = free_in(Some(c), h, n) {
                    return Some((k, f, format!("ABC: classe {} da zona {} a zona {}", ZONES[c as usize], ZONES[z(k) as usize], ZONES[c as usize])));
                }
                // 2) zona piena: libera una cella spostando fuori una UdC di classe inferiore
                let evict = (0..n)
                    .filter(|&e| z(e) == c)
                    .filter_map(|e| match cells[e] {
                        VCell::Lu { class: Some(ce), h: he } if ce > c => Some((e, ce, he)),
                        VCell::Lu { class: None, h: he } => Some((e, 2, he)),
                        _ => None,
                    })
                    .max_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
                if let Some((e, ce, he)) = evict {
                    let target = (0..n)
                        .filter(|&f| matches!(cells[f], VCell::Free) && z(f) > z(e) && z(f) <= ce.max(z(e) + 1) && self.fits(order[f], he))
                        .min_by(|&a, &b| (z(a) != ce).cmp(&(z(b) != ce)).then(a.cmp(&b)));
                    if let Some(f) = target {
                        return Some((e, f, format!("ABC: libera zona {} (classe {} verso zona {})", ZONES[c as usize], ZONES[ce as usize], ZONES[z(f) as usize])));
                    }
                }
            }
        }
        // 3) compattazione: la UdC più lontana nella cella libera più vicina della stessa zona
        let min_gain = self.def.reorg.min_gain_s;
        for k in (0..n).rev() {
            let VCell::Lu { class, h } = cells[k] else { continue };
            let zone = if self.abc_on() { Some(z(k)) } else { None };
            let _ = class;
            if let Some(f) = free_in(zone, h, k)
                && t(k) - t(f) >= min_gain
            {
                return Some((k, f, format!("compattazione: −{:.1} s", t(k) - t(f))));
            }
        }
        None
    }

    /// Piano di riorganizzazione simulato (non crea missioni).
    pub fn plan(&self, aisle: Option<u16>, max_moves: usize) -> ReorgPlan {
        let st = self.st.lock().unwrap();
        self.plan_locked(&st, aisle, max_moves)
    }

    fn plan_locked(&self, st: &State, aisle: Option<u16>, max_moves: usize) -> ReorgPlan {
        let mut moves = vec![];
        let mut before = LayoutMetrics::default();
        let mut after = LayoutMetrics::default();
        let mut per = vec![];
        let add = |acc: &mut LayoutMetrics, m: &LayoutMetrics| {
            let occ = (acc.occupied + m.occupied) as f64;
            acc.frag_index = if occ > 0.0 { (acc.frag_index * acc.occupied as f64 + m.frag_index * m.occupied as f64) / occ } else { 0.0 };
            acc.occupied += m.occupied;
            acc.free += m.free;
            acc.misplaced += m.misplaced;
            acc.cost += m.cost;
            acc.ideal_cost += m.ideal_cost;
        };
        for a in 1..=self.def.aisles.len() as u16 {
            if aisle.is_some_and(|x| x != a) {
                continue;
            }
            let order = &self.by_travel[a as usize - 1];
            let mut cells = self.snapshot(st, a);
            let mb = self.metrics_of(a, &cells);
            let mut n = 0;
            while n < max_moves {
                let Some((k, f, reason)) = self.next_move(a, &cells) else { break };
                let (from, to) = (order[k], order[f]);
                let lu = st.locs[from].lu.clone().unwrap_or_default();
                let class = match cells[k] {
                    VCell::Lu { class, .. } => class,
                    _ => None,
                };
                moves.push(PlannedMove {
                    sku: st.info.get(&lu).map(|i| i.sku.clone()).unwrap_or_default(),
                    lu,
                    class: class.map(|c| ZONES[c as usize].to_string()).unwrap_or_default(),
                    from: self.pos(from),
                    to: self.pos(to),
                    from_code: self.pos(from).code(),
                    to_code: self.pos(to).code(),
                    gain_s: self.travel_i(from) - self.travel_i(to),
                    reason,
                });
                cells[f] = cells[k];
                cells[k] = VCell::Free;
                n += 1;
            }
            let ma = self.metrics_of(a, &cells);
            add(&mut before, &mb);
            add(&mut after, &ma);
            per.push((a, mb, ma, n));
        }
        ReorgPlan { moves, before, after, aisles: per }
    }

    pub fn set_reorg(&self, enabled: bool) {
        self.st.lock().unwrap().reorg_enabled = enabled;
    }

    /// Crea subito fino a `max` movimentazioni dal piano (la catena di mosse
    /// dipendenti viene completata dai cicli successivi).
    pub fn reorg_now(&self, aisle: Option<u16>, max: usize, blocked_aisles: &[u16]) -> Vec<Mission> {
        let mut st = self.st.lock().unwrap();
        let plan = self.plan_locked(&st, aisle, max);
        let mut out = vec![];
        for mv in plan.moves {
            if blocked_aisles.contains(&mv.from.aisle) {
                continue;
            }
            let (Some(f), Some(t)) = (self.idx(&mv.from), self.idx(&mv.to)) else { continue };
            if let Ok(m) = self.move_mission(&mut st, f, t, &mv.reason) {
                st.moves_log.push_back((now_ms(), mv.from.aisle));
                out.push(m);
            }
        }
        out
    }

    /// Riorganizzazione nei tempi morti: una mossa per corsia inattiva.
    fn reorg_tick(&self, db: &TagDb) {
        let unavailable = self.unavailable_aisles(db);
        let mut st = self.st.lock().unwrap();
        if !st.reorg_enabled {
            return;
        }
        let now = now_ms();
        while st.moves_log.front().is_some_and(|m| m.0 < now - 3_600_000) {
            st.moves_log.pop_front();
        }
        for (ai, a) in self.def.aisles.iter().enumerate() {
            let aisle = ai as u16 + 1;
            if unavailable.contains(&aisle) || db.value(&format!("{}.Busy", a.crane)).as_bool().unwrap_or(false) {
                continue;
            }
            if st.missions.iter().any(|m| m.aisle == aisle && m.open()) {
                continue;
            }
            let idle = (now - st.last_activity.get(&aisle).copied().unwrap_or(self.opened)) as f64 / 1000.0;
            if idle < self.def.reorg.idle_s {
                continue;
            }
            if st.moves_log.iter().filter(|m| m.1 == aisle).count() as u32 >= self.def.reorg.max_per_hour {
                continue;
            }
            let cells = self.snapshot(&st, aisle);
            if let Some((k, f, reason)) = self.next_move(aisle, &cells) {
                let order = &self.by_travel[ai];
                if let Ok(m) = self.move_mission(&mut st, order[k], order[f], &reason) {
                    st.moves_log.push_back((now, aisle));
                    tracing::info!(wh = %self.def.name, "riorganizzazione corsia {aisle}: {} {} → {} ({reason})", m.lu, m.src.code(), m.dst.code());
                }
            }
        }
    }

    // ------------------------------------------------------------ interrogazioni

    pub fn set_blocked(&self, p: Pos, blocked: bool) -> Result<()> {
        let mut st = self.st.lock().unwrap();
        let i = self.idx(&p).ok_or_else(|| anyhow!("locazione non valida"))?;
        let cur = st.locs[i].state;
        match (blocked, cur) {
            (true, LocState::Free) | (true, LocState::Occupied) => {
                let lu = st.locs[i].lu.clone();
                self.set_loc(&mut st, i, LocState::Blocked, lu);
            }
            (false, LocState::Blocked) => {
                let lu = st.locs[i].lu.clone();
                let s = if lu.is_some() { LocState::Occupied } else { LocState::Free };
                self.set_loc(&mut st, i, s, lu);
            }
            _ => bail!("operazione non ammessa nello stato {cur:?}"),
        }
        Ok(())
    }

    pub fn find(&self, lu: &str) -> Option<(Pos, Loc)> {
        let st = self.st.lock().unwrap();
        st.lus.get(lu.trim()).map(|&i| (self.pos(i), st.locs[i].clone()))
    }

    pub fn lu_info(&self, lu: &str) -> Option<LuInfo> {
        self.st.lock().unwrap().info.get(lu.trim()).cloned()
    }

    /// Rettifica dell'anagrafica di una UdC a magazzino (inventario).
    pub fn set_lu_info(&self, lu: &str, info: LuInfo) -> Result<LuInfo> {
        let mut st = self.st.lock().unwrap();
        let lu = lu.trim();
        if !st.lus.contains_key(lu) {
            bail!("UdC {lu} non a magazzino");
        }
        let mut info = info;
        info.entry = st.info.get(lu).map(|i| i.entry).filter(|&e| e > 0).unwrap_or_else(now_ms);
        self.persist_lu(lu, Some(&info));
        st.info.insert(lu.to_string(), info.clone());
        Ok(info)
    }

    pub fn location(&self, p: Pos) -> Option<Loc> {
        let st = self.st.lock().unwrap();
        self.idx(&p).map(|i| st.locs[i].clone())
    }

    /// Dettaglio cella con anagrafica, zona e missione.
    pub fn location_detail(&self, p: Pos) -> Option<serde_json::Value> {
        let st = self.st.lock().unwrap();
        let i = self.idx(&p)?;
        let l = &st.locs[i];
        let info = l.lu.as_deref().and_then(|u| st.info.get(u));
        let mission = l.lu.as_deref().and_then(|u| st.missions.iter().find(|m| m.lu == u && m.open()));
        let class = info.and_then(|i| self.class_of(&st, &i.sku));
        Some(serde_json::json!({
            "code": p.code(), "pos": p, "loc": l, "info": info, "mission": mission,
            "zone": ZONES[self.zone[i] as usize], "class": class.map(|c| ZONES[c as usize]),
            "travel_s": self.travel(&p),
            "max_height_mm": self.def.level_max_height_mm.get(p.lvl as usize - 1),
        }))
    }

    /// Celle di un lato di corsia: [livello][colonna] (livello 0 = basso).
    pub fn side_cells(&self, aisle: u16, side: u16) -> Result<Vec<Vec<Cell>>> {
        let st = self.st.lock().unwrap();
        let mut out = vec![];
        for lvl in 1..=self.def.levels {
            let mut row = vec![];
            for col in 1..=self.def.columns {
                let i = self.idx(&Pos { aisle, side, col, lvl }).ok_or_else(|| anyhow!("corsia/lato non validi"))?;
                row.push((st.locs[i].state, st.locs[i].lu.clone()));
            }
            out.push(row);
        }
        Ok(out)
    }

    /// Zone ABC [livello][colonna] e classi delle UdC di un lato.
    pub fn side_abc(&self, aisle: u16, side: u16) -> Result<(Grid<u8>, Grid<Option<u8>>)> {
        let st = self.st.lock().unwrap();
        let mut zones = vec![];
        let mut classes = vec![];
        for lvl in 1..=self.def.levels {
            let mut zr = vec![];
            let mut cr = vec![];
            for col in 1..=self.def.columns {
                let i = self.idx(&Pos { aisle, side, col, lvl }).ok_or_else(|| anyhow!("corsia/lato non validi"))?;
                zr.push(self.zone[i]);
                cr.push(st.locs[i].lu.as_deref().and_then(|u| self.lu_class(&st, u)));
            }
            zones.push(zr);
            classes.push(cr);
        }
        Ok((zones, classes))
    }

    /// Giacenza per articolo.
    pub fn stock(&self) -> serde_json::Value {
        let st = self.st.lock().unwrap();
        let mut freq: HashMap<&str, u32> = HashMap::new();
        for (_, s) in &st.picks {
            *freq.entry(s.as_str()).or_default() += 1;
        }
        #[derive(Default)]
        struct Agg {
            lus: u32,
            qty: f64,
            aisles: Vec<u32>,
            oldest: i64,
            zones: [u32; 3],
            moving: u32,
        }
        let mut by: BTreeMap<String, Agg> = BTreeMap::new();
        for (u, i) in &st.info {
            let Some(&c) = st.lus.get(u) else { continue };
            let a = by.entry(if i.sku.is_empty() { "—".into() } else { i.sku.clone() }).or_insert_with(|| Agg { aisles: vec![0; self.def.aisles.len()], oldest: i64::MAX, ..Default::default() });
            a.lus += 1;
            a.qty += i.qty;
            a.aisles[self.aisle_of(c) as usize - 1] += 1;
            a.oldest = a.oldest.min(i.entry);
            a.zones[self.zone[c] as usize] += 1;
            if st.locs[c].state != LocState::Occupied {
                a.moving += 1;
            }
        }
        let mut rows: Vec<serde_json::Value> = by
            .into_iter()
            .map(|(sku, a)| {
                let class = self.class_of(&st, &sku).map(|c| ZONES[c as usize]);
                serde_json::json!({"sku": sku, "class": class, "picks": freq.get(sku.as_str()).copied().unwrap_or(0), "lus": a.lus, "qty": a.qty,
                    "aisles": a.aisles, "zones": a.zones, "oldest": if a.oldest == i64::MAX { None } else { Some(a.oldest) }, "moving": a.moving})
            })
            .collect();
        rows.sort_by(|a, b| b["picks"].as_u64().cmp(&a["picks"].as_u64()).then(a["sku"].as_str().cmp(&b["sku"].as_str())));
        let mut cls = [0u32; 3];
        for c in st.abc.values() {
            cls[*c as usize] += 1;
        }
        let mut zone_cells = [0u32; 3];
        for &z in &self.zone {
            zone_cells[z as usize] += 1;
        }
        serde_json::json!({"skus": rows, "classes": cls, "zone_cells": zone_cells, "picks_window": st.picks.len(),
            "strategy": self.def.strategy, "lus": st.info.len()})
    }

    /// UdC a magazzino (filtro per articolo o testo), ordinate FIFO.
    pub fn lus(&self, sku: Option<&str>, q: Option<&str>, limit: usize) -> Vec<serde_json::Value> {
        let st = self.st.lock().unwrap();
        let q = q.map(|s| s.to_lowercase());
        let mut v: Vec<(&String, &LuInfo, usize)> = st
            .info
            .iter()
            .filter(|(_, i)| sku.is_none_or(|s| i.sku == s))
            .filter(|(u, i)| q.as_deref().is_none_or(|q| u.to_lowercase().contains(q) || i.sku.to_lowercase().contains(q) || i.lot.to_lowercase().contains(q)))
            .filter_map(|(u, i)| st.lus.get(u).map(|&c| (u, i, c)))
            .collect();
        v.sort_by(|a, b| a.1.entry.cmp(&b.1.entry).then(a.0.cmp(b.0)));
        v.into_iter()
            .take(limit)
            .map(|(u, i, c)| {
                let p = self.pos(c);
                serde_json::json!({"lu": u, "info": i, "code": p.code(), "pos": p, "state": st.locs[c].state, "zone": ZONES[self.zone[c] as usize],
                    "class": self.class_of(&st, &i.sku).map(|c| ZONES[c as usize])})
            })
            .collect()
    }

    pub fn summary(&self) -> serde_json::Value {
        let st = self.st.lock().unwrap();
        let d = &self.def;
        let bs = d.block_cols.max(1);
        let nblocks = (d.columns as usize).div_ceil(bs as usize);
        let hour_ago = now_ms() - 3_600_000;
        let mut aisles = vec![];
        let mut tot = SideSummary::default();
        for (ai, a) in d.aisles.iter().enumerate() {
            let aisle = ai as u16 + 1;
            let mut sides = vec![];
            for side in 1..=d.sides {
                let mut s = SideSummary { blocks: vec![vec![0.0; nblocks]; d.levels as usize], ..Default::default() };
                let mut counts = vec![vec![(0u32, 0u32); nblocks]; d.levels as usize];
                for col in 1..=d.columns {
                    for lvl in 1..=d.levels {
                        let i = self.idx(&Pos { aisle, side, col, lvl }).unwrap();
                        let c = &mut counts[(lvl - 1) as usize][((col - 1) / bs) as usize];
                        c.1 += 1;
                        match st.locs[i].state {
                            LocState::Free => s.free += 1,
                            LocState::Occupied | LocState::Picking => {
                                s.occupied += 1;
                                c.0 += 1;
                            }
                            LocState::Reserved => {
                                s.reserved += 1;
                                c.0 += 1;
                            }
                            LocState::Blocked => s.blocked += 1,
                        }
                    }
                }
                for (l, row) in counts.iter().enumerate() {
                    for (b, (o, n)) in row.iter().enumerate() {
                        s.blocks[l][b] = if *n > 0 { *o as f32 / *n as f32 } else { 0.0 };
                    }
                }
                tot.free += s.free;
                tot.occupied += s.occupied;
                tot.reserved += s.reserved;
                tot.blocked += s.blocked;
                sides.push(s);
            }
            let act: Vec<Mission> = st.active.get(&aisle).map(|ids| ids.iter().filter_map(|id| st.missions.iter().find(|m| m.id == *id)).cloned().collect()).unwrap_or_default();
            let active = act.iter().find(|m| m.slot != 2).cloned().or_else(|| act.first().cloned());
            let active2 = act.iter().find(|m| m.slot == 2).cloned();
            aisles.push(AisleSummary {
                name: a.name.clone(),
                crane: a.crane.clone(),
                sides,
                queued: st.missions.iter().filter(|m| m.aisle == aisle && m.state == MissionState::Queued).count() as u32,
                active,
                active2,
                slots: a.slots,
                done_hour: st.done_log.iter().filter(|r| r.ts >= hour_ago && r.aisle == aisle).count() as u32,
            });
        }
        let total = self.total() as u32;
        let cnt = |k: MissionKind| st.done_log.iter().filter(|r| r.ts >= hour_ago && r.kind == k).count();
        serde_json::json!({
            "name": d.name, "description": d.description,
            "geometry": {"aisles": d.aisles.len(), "sides": d.sides, "columns": d.columns, "levels": d.levels, "block_cols": bs, "total": total},
            "totals": {"free": tot.free, "occupied": tot.occupied, "reserved": tot.reserved, "blocked": tot.blocked,
                       "fill_pct": if total > 0 { (tot.occupied + tot.reserved) as f64 * 100.0 / total as f64 } else { 0.0 }},
            "aisles": aisles,
            "missions": {
                "queued": st.missions.iter().filter(|m| m.state == MissionState::Queued).count(),
                "running": st.missions.iter().filter(|m| matches!(m.state, MissionState::Sent | MissionState::Running)).count(),
                "held": st.missions.iter().filter(|m| m.hold).count(),
                "in_hour": cnt(MissionKind::In), "out_hour": cnt(MissionKind::Out), "move_hour": cnt(MissionKind::Move),
            },
            "orders": {"open": st.orders.iter().filter(|o| !o.state.closed()).count(), "waiting": st.orders.iter().filter(|o| o.state == OrderState::Waiting).count()},
            "reorg": {"enabled": st.reorg_enabled, "moves_hour": st.moves_log.len()},
            "outputs": d.outputs, "inputs": d.inputs,
        })
    }

    /// KPI dei trasli sull'ultima ora (o dall'avvio, se più recente).
    pub fn kpi(&self) -> serde_json::Value {
        let st = self.st.lock().unwrap();
        let now = now_ms();
        let w0 = (now - KPI_WINDOW_MS).max(self.opened);
        let win = (now - w0).max(1) as f64;
        let overlap = |a: i64, b: i64| (b.min(now) - a.max(w0)).max(0) as f64;
        let mut cranes = vec![];
        for (ai, a) in self.def.aisles.iter().enumerate() {
            let aisle = ai as u16 + 1;
            let mut by = [0u32; 3];
            for r in st.done_log.iter().filter(|r| r.aisle == aisle && r.ts >= w0) {
                by[r.kind.slot()] += 1;
            }
            let cyc: Vec<&CycleRec> = st.cycles.iter().filter(|c| c.aisle == aisle && c.t1 >= w0).collect();
            let mut busy: f64 = cyc.iter().map(|c| overlap(c.t0, c.t1)).sum();
            if let Some((t0, _)) = st.cycle_start.get(&aisle) {
                busy += overlap(*t0, now);
            }
            let mut fault: f64 = st.faults.iter().filter(|f| f.0 == aisle).map(|f| overlap(f.1, f.2)).sum();
            if let Some(t0) = st.fault_since.get(&aisle) {
                fault += overlap(*t0, now);
            }
            let n = cyc.len().max(1) as f64;
            let avg = cyc.iter().map(|c| (c.t1 - c.t0) as f64 / 1000.0).sum::<f64>() / n;
            // tasso orario: almeno 10 minuti di base per non gonfiare i valori subito dopo l'avvio
            let hours = win.max(600_000.0) / 3_600_000.0;
            cranes.push(serde_json::json!({
                "aisle": aisle, "name": a.name, "crane": a.crane, "slots": a.slots,
                "in": by[0], "out": by[1], "move": by[2],
                "per_hour": (by[0] + by[1] + by[2]) as f64 / hours,
                "cycles": cyc.len(), "double_cycles": cyc.iter().filter(|c| c.n >= 2).count(),
                "avg_cycle_s": if cyc.is_empty() { 0.0 } else { avg },
                "utilization": (busy / win).min(1.0),
                "availability": (1.0 - fault / win).max(0.0),
                "queued": st.missions.iter().filter(|m| m.aisle == aisle && m.state == MissionState::Queued).count(),
                "busy": st.cycle_start.contains_key(&aisle),
                "faulted": st.fault_since.contains_key(&aisle),
            }));
        }
        // serie a intervalli di 5 minuti
        let bucket = 300_000i64;
        let start = now - KPI_WINDOW_MS;
        let mut series = vec![[0u32; 3]; 12];
        for r in st.done_log.iter().filter(|r| r.ts >= start) {
            let b = (((r.ts - start) / bucket) as usize).min(11);
            series[b][r.kind.slot()] += 1;
        }
        serde_json::json!({"window_s": win / 1000.0, "cranes": cranes, "series": series, "bucket_s": bucket / 1000})
    }

    pub fn missions(&self) -> Vec<Mission> {
        self.st.lock().unwrap().missions.iter().cloned().collect()
    }

    // ------------------------------------------------------------ dispatcher

    /// Priorità effettiva: priorità + 1 punto ogni 2 minuti di attesa.
    fn eff_prio(m: &Mission, now: i64) -> f64 {
        m.priority as f64 + (now - m.created).max(0) as f64 / 120_000.0
    }

    /// Sceglie le missioni del prossimo ciclo di una corsia: la più prioritaria
    /// in coda e, per i trasli a doppia UdC, una compagna (stesso tipo e celle
    /// vicine preferite). Con una sola missione in coda attende fino a
    /// `pair_wait_s` per poterla accoppiare (non le urgenti).
    fn pick_cycle(&self, aisle: u16, slots: u8, pair_wait_s: f64) -> Vec<Mission> {
        let st = self.st.lock().unwrap();
        let now = now_ms();
        let mut queued: Vec<&Mission> = st.missions.iter().filter(|m| m.aisle == aisle && m.state == MissionState::Queued && !m.hold).collect();
        queued.sort_by(|a, b| Self::eff_prio(b, now).total_cmp(&Self::eff_prio(a, now)).then(a.id.cmp(&b.id)));
        let Some(first) = queued.first().copied() else { return vec![] };
        if slots < 2 {
            return vec![first.clone()];
        }
        let cell = |m: &Mission| if m.kind == MissionKind::In { m.dst } else { m.src };
        let dist = |a: &Mission, b: &Mission| {
            let (p, q) = (cell(a), cell(b));
            (p.col as f64 - q.col as f64).abs() * self.def.col_time_s + (p.lvl as f64 - q.lvl as f64).abs() * self.def.lvl_time_s + if p.side != q.side { 0.5 } else { 0.0 }
        };
        let partner = queued.iter().skip(1).copied().min_by(|a, b| {
            let ka = (a.kind != first.kind) as u8;
            let kb = (b.kind != first.kind) as u8;
            ka.cmp(&kb).then(dist(first, a).total_cmp(&dist(first, b)))
        });
        match partner {
            Some(p) => vec![first.clone(), p.clone()],
            None if first.priority < 8 && (now - first.created) as f64 / 1000.0 < pair_wait_s => vec![],
            None => vec![first.clone()],
        }
    }

    /// Un ciclo del dispatcher: dialogo con i trasli via tag.
    pub async fn dispatch(&self, db: &TagDb) {
        for (ai, a) in self.def.aisles.iter().enumerate() {
            let aisle = ai as u16 + 1;
            let t = |m: &str| format!("{}.{}", a.crane, m);
            let busy = db.value(&t("Busy")).as_bool().unwrap_or(false);
            let start = db.value(&t("Start")).as_bool().unwrap_or(false);
            let done = db.value(&t("Done")).as_i64().unwrap_or(0) as u32;
            let done2 = if a.slots == 2 { db.value(&t("Done2")).as_i64().unwrap_or(0) as u32 } else { 0 };
            let fault = db.value(&t("Fault")).as_bool().unwrap_or(false);
            {
                // disponibilità: intervalli di guasto
                let mut st = self.st.lock().unwrap();
                match (fault, st.fault_since.get(&aisle).copied()) {
                    (true, None) => {
                        st.fault_since.insert(aisle, now_ms());
                    }
                    (false, Some(t0)) => {
                        st.fault_since.remove(&aisle);
                        st.faults.push_back((aisle, t0, now_ms()));
                        while st.faults.len() > 500 {
                            st.faults.pop_front();
                        }
                    }
                    _ => {}
                }
            }
            // il PLC ha preso in carico: azzera lo Start
            if start && busy {
                let _ = db.write(&t("Start"), Value::Bool(false)).await;
                let mut st = self.st.lock().unwrap();
                let ids = st.active.get(&aisle).cloned().unwrap_or_default();
                for id in ids {
                    if let Some(m) = st.missions.iter_mut().find(|m| m.id == id && m.state == MissionState::Sent) {
                        m.state = MissionState::Running;
                        let m = m.clone();
                        self.persist_mission(&m);
                    }
                }
                continue;
            }
            // completamento (ogni posto UdC indipendentemente)
            let active = self.st.lock().unwrap().active.get(&aisle).cloned().unwrap_or_default();
            if !active.is_empty() {
                if !start {
                    for id in &active {
                        if *id == done || (a.slots == 2 && *id == done2) {
                            self.complete(aisle, *id);
                        }
                    }
                }
                let left = self.st.lock().unwrap().active.get(&aisle).map(|v| v.len()).unwrap_or(0);
                if left > 0 || busy {
                    continue;
                }
            }
            if busy || start || fault {
                continue;
            }
            let cycle = self.pick_cycle(aisle, a.slots, a.pair_wait_s);
            if cycle.is_empty() {
                continue;
            }
            let mut writes: Vec<(String, Value)> = vec![];
            for slot in 1..=a.slots {
                let sfx = if slot == 1 { "" } else { "2" };
                let m = cycle.get(slot as usize - 1);
                let (typ, src, dst, out_bay, id) = match m {
                    Some(m) => (
                        m.kind.type_code(),
                        m.src,
                        m.dst,
                        if m.kind == MissionKind::Out { self.def.outputs.iter().position(|o| *o == m.bay).map(|p| p as i64 + 1).unwrap_or(0) } else { 0 },
                        m.id as i64,
                    ),
                    None => (0, Self::pd(aisle), Self::pd(aisle), 0, 0),
                };
                writes.extend([
                    (format!("Mission{sfx}"), Value::Int(id)),
                    (format!("Type{sfx}"), Value::Int(typ)),
                    (format!("SrcSide{sfx}"), Value::Int(src.side as i64)),
                    (format!("SrcCol{sfx}"), Value::Int(src.col as i64)),
                    (format!("SrcLvl{sfx}"), Value::Int(src.lvl as i64)),
                    (format!("DstSide{sfx}"), Value::Int(dst.side as i64)),
                    (format!("DstCol{sfx}"), Value::Int(dst.col as i64)),
                    (format!("DstLvl{sfx}"), Value::Int(dst.lvl as i64)),
                    (format!("DstOut{sfx}"), Value::Int(out_bay)),
                ]);
            }
            let mut ok = true;
            for (k, v) in writes {
                if let Err(e) = db.write(&t(&k), v).await {
                    tracing::warn!(wh = %self.def.name, "traslo {}: scrittura {k}: {e:#}", a.crane);
                    ok = false;
                    break;
                }
            }
            if ok && db.write(&t("Start"), Value::Bool(true)).await.is_ok() {
                let mut st = self.st.lock().unwrap();
                st.active.insert(aisle, cycle.iter().map(|m| m.id).collect());
                st.cycle_start.insert(aisle, (now_ms(), cycle.len() as u8));
                for (k, m) in cycle.iter().enumerate() {
                    if let Some(x) = st.missions.iter_mut().find(|x| x.id == m.id) {
                        x.state = MissionState::Sent;
                        x.slot = k as u8 + 1;
                        x.started = Some(now_ms());
                        let x = x.clone();
                        self.persist_mission(&x);
                    }
                }
                if cycle.len() == 2 {
                    tracing::info!(wh = %self.def.name, "traslo {}: ciclo doppio missioni {} + {}", a.crane, cycle[0].id, cycle[1].id);
                }
            }
        }
    }

    fn complete(&self, aisle: u16, id: u32) {
        let mut st = self.st.lock().unwrap();
        let now = now_ms();
        if let Some(v) = st.active.get_mut(&aisle) {
            v.retain(|x| *x != id);
            if v.is_empty() {
                st.active.remove(&aisle);
                if let Some((t0, n)) = st.cycle_start.remove(&aisle) {
                    st.cycles.push_back(CycleRec { aisle, t0, t1: now, n });
                    while st.cycles.front().is_some_and(|c| c.t1 < now - KPI_WINDOW_MS) || st.cycles.len() > 20000 {
                        st.cycles.pop_front();
                    }
                }
            }
        }
        let Some(k) = st.missions.iter().position(|m| m.id == id) else { return };
        let mut m = st.missions.remove(k).unwrap();
        match m.kind {
            MissionKind::In => {
                if let Some(i) = self.idx(&m.dst) {
                    self.set_loc(&mut st, i, LocState::Occupied, Some(m.lu.clone()));
                }
            }
            MissionKind::Out => {
                if let Some(i) = self.idx(&m.src) {
                    self.set_loc(&mut st, i, LocState::Free, None);
                }
                let info = st.info.remove(&m.lu);
                self.persist_lu(&m.lu, None);
                let sku = info.as_ref().map(|i| i.sku.clone()).unwrap_or_else(|| m.sku.clone());
                self.record_pick(&mut st, &sku);
                if let Some(oid) = m.order
                    && let Some(ok) = st.orders.iter().position(|o| o.id == oid)
                {
                    if let Some(l) = st.orders[ok].lines.iter_mut().find(|l| l.sku == sku) {
                        l.picked += info.map(|i| i.qty.max(1.0)).unwrap_or(1.0);
                    }
                    // la missione non è più aperta: aggiorna lo stato dopo averla tolta dalla coda
                    self.update_order_state(&mut st, ok);
                }
            }
            MissionKind::Move => {
                if let Some(i) = self.idx(&m.src) {
                    self.set_loc(&mut st, i, LocState::Free, None);
                }
                if let Some(i) = self.idx(&m.dst) {
                    self.set_loc(&mut st, i, LocState::Occupied, Some(m.lu.clone()));
                }
            }
        }
        if m.kind != MissionKind::Move {
            st.last_activity.insert(aisle, now);
        }
        m.state = MissionState::Done;
        m.finished = Some(now);
        st.done_log.push_back(DoneRec { ts: now, aisle, kind: m.kind });
        while st.done_log.len() > 20000 || st.done_log.front().is_some_and(|r| r.ts < now - KPI_WINDOW_MS) {
            st.done_log.pop_front();
        }
        self.persist_mission(&m);
        tracing::info!(wh = %self.def.name, "missione {id} completata ({:?} {})", m.kind, m.lu);
    }

    /// Corsie con traslo non disponibile (guasto o non in automatico).
    pub fn unavailable_aisles(&self, db: &TagDb) -> Vec<u16> {
        self.def
            .aisles
            .iter()
            .enumerate()
            .filter(|(_, a)| db.value(&format!("{}.Fault", a.crane)).as_bool().unwrap_or(false) || db.value(&format!("{}.Auto", a.crane)).as_bool() == Some(false))
            .map(|(i, _)| i as u16 + 1)
            .collect()
    }

    /// Demo: ordine casuale (articoli a giacenza, più frequenti i "veloci").
    fn demo_order(&self, db: &TagDb) {
        let skus: Vec<String> = {
            let st = self.st.lock().unwrap();
            let mut v: Vec<(u8, String)> = st.info.values().filter(|i| !i.sku.is_empty()).map(|i| (self.class_of(&st, &i.sku).unwrap_or(2), i.sku.clone())).collect();
            v.sort();
            v.dedup();
            v.into_iter().map(|x| x.1).collect()
        };
        if skus.is_empty() {
            return;
        }
        let mut rng = Rng(now_ms() as u64 | 1);
        let nl = 1 + rng.next() % 3;
        let lines = (0..nl)
            .map(|_| OrderLineReq { sku: skus[((rng.unit().powf(2.5)) * skus.len() as f64) as usize % skus.len()].clone(), qty: (20 + rng.next() % 60) as f64 })
            .collect();
        if let Err(e) = self.create_order(OrderReq { reference: String::new(), bay: None, priority: Some(if rng.next().is_multiple_of(10) { 8 } else { PRIO_DEFAULT }), lines }, &self.unavailable_aisles(db)) {
            tracing::debug!("ordine automatico: {e:#}");
        }
    }
}

/// Cella nello stato simulato della riorganizzazione.
#[derive(Debug, Clone, Copy)]
enum VCell {
    Free,
    Lu { class: Option<u8>, h: u16 },
    /// occupata da una missione in corso: non movimentabile
    Locked { class: Option<u8> },
    Blocked,
}

/// Task: dispatcher missioni, ordini, riorganizzazione e prelievi automatici demo.
pub async fn run(wh: Arc<Warehouse>, db: Arc<TagDb>) {
    let mut iv = tokio::time::interval(Duration::from_millis(200));
    let (mut last_auto, mut last_order, mut last_slow) = (now_ms(), now_ms(), now_ms());
    loop {
        iv.tick().await;
        wh.dispatch(&db).await;
        let now = now_ms();
        if now - last_slow > 5_000 {
            last_slow = now;
            wh.refresh_abc();
            wh.retry_orders(&wh.unavailable_aisles(&db));
            wh.reorg_tick(&db);
        }
        if wh.def.auto_outbound_s > 0 && now - last_auto > wh.def.auto_outbound_s as i64 * 1000 {
            last_auto = now;
            let queued_out = wh.missions().iter().filter(|m| m.kind == MissionKind::Out && m.state == MissionState::Queued).count();
            if queued_out < wh.def.aisles.len()
                && let Err(e) = wh.create_outbound(None, None, &wh.unavailable_aisles(&db))
            {
                tracing::debug!("prelievo automatico: {e:#}");
            }
        }
        if wh.def.auto_orders_s > 0 && now - last_order > wh.def.auto_orders_s as i64 * 1000 {
            last_order = now;
            let queued_out = wh.missions().iter().filter(|m| m.kind == MissionKind::Out && m.state == MissionState::Queued).count();
            if queued_out < wh.def.aisles.len() * 2 {
                wh.demo_order(&db);
            }
        }
    }
}
