//! Magazzino automatico (AS/RS): locazioni, unità di carico, missioni,
//! allocazione delle celle e dispatcher delle missioni verso i trasloelevatori.
//!
//! Geometria: corsie × lati × colonne × livelli. Colonna 0 = stazione P&D
//! (pick & deposit) in testa alla corsia. Ogni corsia ha un traslo collegato a
//! un'istanza UDT con il telegramma missione:
//!
//! | membro                       | scritto da | significato                           |
//! |------------------------------|------------|---------------------------------------|
//! | Mission (u32)                | WCS        | id missione                           |
//! | Type (u16)                   | WCS        | 1 deposito, 2 prelievo, 3 movimentazione |
//! | SrcSide/SrcCol/SrcLvl (u16)  | WCS        | origine (col 0 = P&D)                 |
//! | DstSide/DstCol/DstLvl (u16)  | WCS        | destinazione (col 0 = P&D)            |
//! | DstOut (u16)                 | WCS        | baia d'uscita (prelievi)              |
//! | Start (bool)                 | WCS        | avvio; azzerato dal WCS quando Busy=1 |
//! | Busy (bool)                  | PLC        | missione in esecuzione                |
//! | Done (u32)                   | PLC        | id dell'ultima missione completata    |
//! | Fault (bool)                 | PLC        | traslo in guasto (non riceve missioni)|

use crate::config::WarehouseDef;
use crate::store::Store;
use crate::tagdb::{TagDb, now_ms};
use crate::value::Value;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LocState {
    Free,
    Occupied,
    /// prenotata da una missione di deposito in corso
    Reserved,
    /// in uscita: occupata ma con missione di prelievo in corso
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MissionKind {
    In,
    Out,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mission {
    pub id: u32,
    pub kind: MissionKind,
    pub lu: String,
    pub aisle: u16,
    pub src: Pos,
    pub dst: Pos,
    /// ingresso di provenienza (depositi) o baia d'uscita (prelievi)
    pub bay: String,
    pub state: MissionState,
    pub created: i64,
    pub started: Option<i64>,
    pub finished: Option<i64>,
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
    pub done_hour: u32,
}

pub struct Warehouse {
    pub def: WarehouseDef,
    st: Mutex<State>,
    store: Store,
}

struct State {
    locs: Vec<Loc>,
    lus: HashMap<String, usize>,
    missions: VecDeque<Mission>,
    /// missione inviata/in corso per corsia
    active: HashMap<u16, u32>,
    next_id: u32,
    done_log: VecDeque<(i64, u16, MissionKind)>,
    out_rr: usize,
}

impl Warehouse {
    pub fn total(&self) -> usize {
        let d = &self.def;
        d.aisles.len() * d.sides as usize * d.columns as usize * d.levels as usize
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

    /// Carica le locazioni salvate; se il magazzino è nuovo applica il riempimento iniziale demo.
    pub async fn open(def: WarehouseDef, store: Store) -> Result<Arc<Self>> {
        let wh = Self {
            st: Mutex::new(State {
                locs: vec![],
                lus: HashMap::new(),
                missions: VecDeque::new(),
                active: HashMap::new(),
                next_id: 1,
                done_log: VecDeque::new(),
                out_rr: 0,
            }),
            def,
            store,
        };
        let total = wh.total();
        let mut locs = vec![Loc { state: LocState::Free, lu: None, ts: now_ms() }; total];
        let saved = wh.store.wh_load_locs(wh.def.name.clone()).await?;
        let fresh = saved.is_empty();
        for (i, state, lu, ts) in saved {
            if (i as usize) < total {
                locs[i as usize] = Loc { state: LocState::parse(&state), lu, ts };
            }
        }
        let missions = wh.store.wh_load_missions(wh.def.name.clone()).await?;
        let next_id = wh.store.wh_max_mission(wh.def.name.clone()).await? + 1;
        {
            let mut st = wh.st.lock().unwrap();
            st.locs = locs;
            st.next_id = next_id;
            for m in missions {
                // missioni interrotte: le reinviamo (lo stato della cella è già prenotato)
                let mut m = m;
                if matches!(m.state, MissionState::Sent | MissionState::Running) {
                    m.state = MissionState::Queued;
                }
                st.missions.push_back(m);
            }
            if fresh && wh.def.initial_fill_pct > 0 {
                let mut seed: u64 = 0x2545F4914F6CDD1D;
                let mut n = 0;
                for i in 0..total {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    if (seed % 100) < wh.def.initial_fill_pct as u64 {
                        n += 1;
                        let lu = format!("{}{:06}", wh.def.lu_prefix, n);
                        st.locs[i] = Loc { state: LocState::Occupied, lu: Some(lu.clone()), ts: now_ms() };
                    }
                }
            }
            let lus: HashMap<String, usize> =
                st.locs.iter().enumerate().filter_map(|(i, l)| l.lu.clone().map(|u| (u, i))).collect();
            st.lus = lus;
        }
        if fresh {
            let rows: Vec<_> = {
                let st = wh.st.lock().unwrap();
                st.locs.iter().enumerate().filter(|(_, l)| l.state != LocState::Free).map(|(i, l)| (i as u32, l.state.code().to_string(), l.lu.clone(), l.ts)).collect()
            };
            wh.store.wh_save_locs(wh.def.name.clone(), rows).await?;
            tracing::info!("magazzino {}: {} locazioni create", wh.def.name, total);
        }
        Ok(Arc::new(wh))
    }

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

    fn set_loc(&self, st: &mut State, i: usize, state: LocState, lu: Option<String>) {
        if let Some(old) = st.locs[i].lu.take()
            && st.lus.get(&old) == Some(&i) {
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

    /// Tempo di traslazione stimato (moto contemporaneo dei due assi).
    fn travel(&self, p: &Pos) -> f64 {
        (p.col as f64 * self.def.col_time_s).max(p.lvl as f64 * self.def.lvl_time_s)
    }

    /// Deposito: sceglie corsia e cella, prenota e accoda la missione. Ritorna la corsia.
    /// `blocked_aisles`: corsie con traslo in guasto/manuale (escluse).
    pub fn allocate_inbound(&self, lu: &str, bay: &str, blocked_aisles: &[u16], dry_run: bool) -> Result<(u16, Pos)> {
        let lu = lu.trim();
        if lu.is_empty() || lu == "NOREAD" {
            bail!("identificativo UdC mancante");
        }
        let mut st = self.st.lock().unwrap();
        if st.lus.contains_key(lu) || st.missions.iter().any(|m| m.lu == lu && matches!(m.state, MissionState::Queued | MissionState::Sent | MissionState::Running)) {
            bail!("UdC {lu} già presente in magazzino");
        }
        // corsia: meno missioni in coda, poi più celle libere
        let mut best: Option<(u16, usize, usize)> = None;
        for a in 1..=self.def.aisles.len() as u16 {
            if blocked_aisles.contains(&a) {
                continue;
            }
            let free = (0..st.locs.len()).filter(|&i| st.locs[i].state == LocState::Free && self.pos(i).aisle == a).count();
            if free == 0 {
                continue;
            }
            let q = st.missions.iter().filter(|m| m.aisle == a && !matches!(m.state, MissionState::Done | MissionState::Cancelled)).count();
            if best.is_none_or(|(_, bq, bf)| q < bq || (q == bq && free > bf)) {
                best = Some((a, q, free));
            }
        }
        let (aisle, _, _) = best.ok_or_else(|| anyhow!("magazzino pieno o corsie non disponibili"))?;
        // cella libera più vicina al P&D
        let i = (0..st.locs.len())
            .filter(|&i| st.locs[i].state == LocState::Free && self.pos(i).aisle == aisle)
            .min_by(|&x, &y| self.travel(&self.pos(x)).total_cmp(&self.travel(&self.pos(y))).then(x.cmp(&y)))
            .unwrap();
        let dst = self.pos(i);
        if dry_run {
            return Ok((aisle, dst));
        }
        self.set_loc(&mut st, i, LocState::Reserved, Some(lu.to_string()));
        let id = st.next_id;
        st.next_id += 1;
        let m = Mission {
            id,
            kind: MissionKind::In,
            lu: lu.into(),
            aisle,
            src: Self::pd(aisle),
            dst,
            bay: bay.into(),
            state: MissionState::Queued,
            created: now_ms(),
            started: None,
            finished: None,
        };
        self.persist_mission(&m);
        st.missions.push_back(m);
        tracing::info!(wh = %self.def.name, "deposito {lu} → {} (missione {id})", dst.code());
        Ok((aisle, dst))
    }

    /// Prelievo di una UdC (o di una a caso se `lu` è None) verso una baia d'uscita.
    pub fn create_outbound(&self, lu: Option<&str>, bay: Option<&str>, blocked_aisles: &[u16]) -> Result<Mission> {
        let mut st = self.st.lock().unwrap();
        let i = match lu {
            Some(u) => *st.lus.get(u.trim()).ok_or_else(|| anyhow!("UdC {u} non trovata"))?,
            None => {
                let cands: Vec<usize> = (0..st.locs.len())
                    .filter(|&i| st.locs[i].state == LocState::Occupied && !blocked_aisles.contains(&self.pos(i).aisle))
                    .collect();
                if cands.is_empty() {
                    bail!("nessuna UdC prelevabile");
                }
                cands[(now_ms() as usize / 7) % cands.len()]
            }
        };
        if st.locs[i].state != LocState::Occupied {
            bail!("locazione {} non prelevabile (stato {:?})", self.pos(i).code(), st.locs[i].state);
        }
        let bay = match bay {
            Some(b) if self.def.outputs.iter().any(|o| o == b) => b.to_string(),
            Some(b) => bail!("baia d'uscita sconosciuta: {b}"),
            None => {
                let b = self.def.outputs[st.out_rr % self.def.outputs.len().max(1)].clone();
                st.out_rr += 1;
                b
            }
        };
        let src = self.pos(i);
        let lu = st.locs[i].lu.clone().unwrap_or_default();
        self.set_loc(&mut st, i, LocState::Picking, Some(lu.clone()));
        let id = st.next_id;
        st.next_id += 1;
        let m = Mission {
            id,
            kind: MissionKind::Out,
            lu,
            aisle: src.aisle,
            src,
            dst: Self::pd(src.aisle),
            bay,
            state: MissionState::Queued,
            created: now_ms(),
            started: None,
            finished: None,
        };
        self.persist_mission(&m);
        st.missions.push_back(m.clone());
        Ok(m)
    }

    pub fn cancel(&self, id: u32) -> Result<()> {
        let mut st = self.st.lock().unwrap();
        let k = st.missions.iter().position(|m| m.id == id).ok_or_else(|| anyhow!("missione {id} non trovata"))?;
        if st.missions[k].state != MissionState::Queued {
            bail!("solo le missioni in coda possono essere annullate");
        }
        let m = st.missions[k].clone();
        match m.kind {
            MissionKind::In => {
                if let Some(i) = self.idx(&m.dst) {
                    self.set_loc(&mut st, i, LocState::Free, None);
                }
            }
            MissionKind::Out => {
                if let Some(i) = self.idx(&m.src) {
                    self.set_loc(&mut st, i, LocState::Occupied, Some(m.lu.clone()));
                }
            }
        }
        st.missions[k].state = MissionState::Cancelled;
        st.missions[k].finished = Some(now_ms());
        let m = st.missions.remove(k).unwrap();
        self.persist_mission(&m);
        Ok(())
    }

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

    pub fn location(&self, p: Pos) -> Option<Loc> {
        let st = self.st.lock().unwrap();
        self.idx(&p).map(|i| st.locs[i].clone())
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
            let active = st.active.get(&aisle).and_then(|id| st.missions.iter().find(|m| m.id == *id)).cloned();
            aisles.push(AisleSummary {
                name: a.name.clone(),
                crane: a.crane.clone(),
                sides,
                queued: st.missions.iter().filter(|m| m.aisle == aisle && m.state == MissionState::Queued).count() as u32,
                active,
                done_hour: st.done_log.iter().filter(|(t, a2, _)| *t >= hour_ago && *a2 == aisle).count() as u32,
            });
        }
        let total = self.total() as u32;
        serde_json::json!({
            "name": d.name, "description": d.description,
            "geometry": {"aisles": d.aisles.len(), "sides": d.sides, "columns": d.columns, "levels": d.levels, "block_cols": bs, "total": total},
            "totals": {"free": tot.free, "occupied": tot.occupied, "reserved": tot.reserved, "blocked": tot.blocked,
                       "fill_pct": if total > 0 { (tot.occupied + tot.reserved) as f64 * 100.0 / total as f64 } else { 0.0 }},
            "aisles": aisles,
            "missions": {
                "queued": st.missions.iter().filter(|m| m.state == MissionState::Queued).count(),
                "running": st.missions.iter().filter(|m| matches!(m.state, MissionState::Sent | MissionState::Running)).count(),
                "in_hour": st.done_log.iter().filter(|(t, _, k)| *t >= hour_ago && *k == MissionKind::In).count(),
                "out_hour": st.done_log.iter().filter(|(t, _, k)| *t >= hour_ago && *k == MissionKind::Out).count(),
            },
            "outputs": d.outputs, "inputs": d.inputs,
        })
    }

    pub fn missions(&self) -> Vec<Mission> {
        self.st.lock().unwrap().missions.iter().cloned().collect()
    }

    /// Un ciclo del dispatcher: dialogo con i trasli via tag.
    pub async fn dispatch(&self, db: &TagDb) {
        for (ai, a) in self.def.aisles.iter().enumerate() {
            let aisle = ai as u16 + 1;
            let t = |m: &str| format!("{}.{}", a.crane, m);
            let busy = db.value(&t("Busy")).as_bool().unwrap_or(false);
            let start = db.value(&t("Start")).as_bool().unwrap_or(false);
            let done = db.value(&t("Done")).as_i64().unwrap_or(0) as u32;
            let fault = db.value(&t("Fault")).as_bool().unwrap_or(false);
            // il PLC ha preso in carico: azzera lo Start
            if start && busy {
                let _ = db.write(&t("Start"), Value::Bool(false)).await;
                let mut st = self.st.lock().unwrap();
                if let Some(id) = st.active.get(&aisle).copied()
                    && let Some(m) = st.missions.iter_mut().find(|m| m.id == id && m.state == MissionState::Sent) {
                        m.state = MissionState::Running;
                        let m = m.clone();
                        self.persist_mission(&m);
                    }
                continue;
            }
            // completamento
            let active = self.st.lock().unwrap().active.get(&aisle).copied();
            if let Some(id) = active {
                if done == id && !busy && !start {
                    self.complete(aisle, id);
                }
                continue;
            }
            if busy || start || fault {
                continue;
            }
            // nuova missione per la corsia (prelievi e depositi in ordine di arrivo)
            let next = {
                let st = self.st.lock().unwrap();
                st.missions.iter().find(|m| m.aisle == aisle && m.state == MissionState::Queued).cloned()
            };
            let Some(m) = next else { continue };
            let typ = match m.kind {
                MissionKind::In => 1,
                MissionKind::Out => 2,
            };
            let out_bay = self.def.outputs.iter().position(|o| *o == m.bay).map(|p| p as i64 + 1).unwrap_or(0);
            let writes = [
                ("Mission", Value::Int(m.id as i64)),
                ("Type", Value::Int(typ)),
                ("SrcSide", Value::Int(m.src.side as i64)),
                ("SrcCol", Value::Int(m.src.col as i64)),
                ("SrcLvl", Value::Int(m.src.lvl as i64)),
                ("DstSide", Value::Int(m.dst.side as i64)),
                ("DstCol", Value::Int(m.dst.col as i64)),
                ("DstLvl", Value::Int(m.dst.lvl as i64)),
                ("DstOut", Value::Int(out_bay)),
            ];
            let mut ok = true;
            for (k, v) in writes {
                if let Err(e) = db.write(&t(k), v).await {
                    tracing::warn!(wh = %self.def.name, "traslo {}: scrittura {k}: {e:#}", a.crane);
                    ok = false;
                    break;
                }
            }
            if ok && db.write(&t("Start"), Value::Bool(true)).await.is_ok() {
                let mut st = self.st.lock().unwrap();
                st.active.insert(aisle, m.id);
                if let Some(x) = st.missions.iter_mut().find(|x| x.id == m.id) {
                    x.state = MissionState::Sent;
                    x.started = Some(now_ms());
                    let x = x.clone();
                    self.persist_mission(&x);
                }
            }
        }
    }

    fn complete(&self, aisle: u16, id: u32) {
        let mut st = self.st.lock().unwrap();
        st.active.remove(&aisle);
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
            }
        }
        m.state = MissionState::Done;
        m.finished = Some(now_ms());
        st.done_log.push_back((now_ms(), aisle, m.kind));
        while st.done_log.len() > 20000 || st.done_log.front().is_some_and(|(t, _, _)| *t < now_ms() - 3_600_000) {
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
            .filter(|(_, a)| {
                db.value(&format!("{}.Fault", a.crane)).as_bool().unwrap_or(false)
                    || db.value(&format!("{}.Auto", a.crane)).as_bool() == Some(false)
            })
            .map(|(i, _)| i as u16 + 1)
            .collect()
    }
}

/// Task: dispatcher missioni + prelievi automatici demo.
pub async fn run(wh: Arc<Warehouse>, db: Arc<TagDb>) {
    let mut iv = tokio::time::interval(Duration::from_millis(200));
    let mut last_auto = now_ms();
    loop {
        iv.tick().await;
        wh.dispatch(&db).await;
        if wh.def.auto_outbound_s > 0 && now_ms() - last_auto > wh.def.auto_outbound_s as i64 * 1000 {
            last_auto = now_ms();
            let queued_out = wh.missions().iter().filter(|m| m.kind == MissionKind::Out && m.state == MissionState::Queued).count();
            if queued_out < wh.def.aisles.len()
                && let Err(e) = wh.create_outbound(None, None, &wh.unavailable_aisles(&db)) {
                    tracing::debug!("prelievo automatico: {e:#}");
                }
        }
    }
}
