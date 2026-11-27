//! PLC simulato dell'hub di smistamento (Modbus TCP), da usare con il layer fisico di PTWCS.
//!
//! Esegue la logica "vera" di un impianto di trasporto: i sensori (fotocellule, letture
//! scanner, velocità reali dei motori) arrivano dal layer fisico negli ingressi simulati,
//! le uscite (marcia nastri, posizione deviatori) le legge il layer fisico.
//! La topologia (linee, nastri, merge, sorter, scivoli, contatori) è in un JSON
//! generato da `tools/build_demo_hub.py` insieme a tag e viste.
//!
//! Mappa Modbus:
//!   coil 30 reset guasti                     coil 99 / hr 99  simulazione attiva / heartbeat
//!   coil 100+L  abilitazione linea L (WCS)   di 100+L  linea L in marcia
//!   coil 1000+N marcia nastro N (PLC)        di 1000+N guasto nastro N   hr 1000+N velocità reale [m/s×100] (sim)
//!   hr 1100+D   posizione deviatore D (PLC)
//!   coil 2000+P fotocellula P (sim)
//!   hr 2000+16·S scanner S (sim): +0 contatore letture, +1..10 barcode (20 char), +11 peso [g/10]
//!   telegramma WCS dello scanner a partire da `base`: ReqID, peso, barcode, …, RespID +12, destinazione +13
//!   ir I        contatori (fotocellule di conteggio, ricircoli)
//!
//! Logica:
//! - marcia nastro = linea abilitata, nessun guasto, consenso del merge (priorità alla linea
//!   principale: l'immissione attende se la fotocellula principale è occupata, con anti-blocco);
//! - controllo velocità: marcia comandata da 2,5 s e velocità reale nulla → guasto motore;
//! - scanner: a ogni lettura richiesta al WCS (protocollo request/reply a contatore) e
//!   inseguimento del collo in una FIFO per ogni deviatore;
//! - deviatori "route" (es. pre-smistamento Nord/Sud/scarti): posizione = destinazione;
//! - deviatori "chute" (sorter): devia se la destinazione è il suo scivolo e lo scivolo non è
//!   pieno, altrimenti il collo prosegue; dopo l'ultimo deviatore ricircola e viene riletto.
//!
//! Uso: plc_hub [config/hub/plc_hub.json] [bind]   (default 0.0.0.0:5021)

use serde::Deserialize;
use std::collections::VecDeque;
use std::future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tokio_modbus::prelude::*;
use tokio_modbus::server::tcp::{Server, accept_tcp_connection};

const N: usize = 4096;

#[derive(Default)]
struct Mem {
    coils: Vec<bool>,
    di: Vec<bool>,
    hr: Vec<u16>,
    ir: Vec<u16>,
}

#[derive(Clone)]
struct Plc(Arc<Mutex<Mem>>);

fn slice<T: Copy>(v: &[T], a: u16, n: u16) -> Result<Vec<T>, ExceptionCode> {
    let (a, n) = (a as usize, n as usize);
    v.get(a..a + n).map(<[T]>::to_vec).ok_or(ExceptionCode::IllegalDataAddress)
}

impl tokio_modbus::server::Service for Plc {
    type Request = Request<'static>;
    type Response = Response;
    type Exception = ExceptionCode;
    type Future = future::Ready<Result<Response, ExceptionCode>>;

    fn call(&self, req: Request<'static>) -> Self::Future {
        let mut m = self.0.lock().unwrap();
        let r = (|| -> Result<Response, ExceptionCode> {
            Ok(match req {
                Request::ReadCoils(a, n) => Response::ReadCoils(slice(&m.coils, a, n)?),
                Request::ReadDiscreteInputs(a, n) => Response::ReadDiscreteInputs(slice(&m.di, a, n)?),
                Request::ReadHoldingRegisters(a, n) => Response::ReadHoldingRegisters(slice(&m.hr, a, n)?),
                Request::ReadInputRegisters(a, n) => Response::ReadInputRegisters(slice(&m.ir, a, n)?),
                Request::WriteSingleCoil(a, v) => {
                    *m.coils.get_mut(a as usize).ok_or(ExceptionCode::IllegalDataAddress)? = v;
                    Response::WriteSingleCoil(a, v)
                }
                Request::WriteSingleRegister(a, v) => {
                    *m.hr.get_mut(a as usize).ok_or(ExceptionCode::IllegalDataAddress)? = v;
                    Response::WriteSingleRegister(a, v)
                }
                Request::WriteMultipleRegisters(a, vs) => {
                    let s = a as usize;
                    let dst = m.hr.get_mut(s..s + vs.len()).ok_or(ExceptionCode::IllegalDataAddress)?;
                    dst.copy_from_slice(&vs);
                    Response::WriteMultipleRegisters(a, vs.len() as u16)
                }
                Request::WriteMultipleCoils(a, vs) => {
                    let s = a as usize;
                    let dst = m.coils.get_mut(s..s + vs.len()).ok_or(ExceptionCode::IllegalDataAddress)?;
                    dst.copy_from_slice(&vs);
                    Response::WriteMultipleCoils(a, vs.len() as u16)
                }
                _ => return Err(ExceptionCode::IllegalFunction),
            })
        })();
        future::ready(r)
    }
}

// ------------------------------------------------------------------ configurazione

#[derive(Deserialize)]
struct Cfg {
    lines: Vec<LineCfg>,
    conveyors: Vec<ConvCfg>,
    #[serde(default)]
    scanners: Vec<ScanCfg>,
    #[serde(default)]
    sorters: Vec<SorterCfg>,
    #[serde(default)]
    counters: Vec<CounterCfg>,
}
#[derive(Deserialize)]
struct LineCfg {
    name: String,
    n: usize,
}
#[derive(Deserialize)]
struct ConvCfg {
    name: String,
    n: usize,
    line: usize,
    #[serde(default)]
    gate: Option<GateCfg>,
}
/// Merge: questo nastro immette su una linea principale; attende se la fotocellula
/// principale è occupata (al massimo `max_wait_s`, poi passa per 2 s).
#[derive(Deserialize)]
struct GateCfg {
    main_pe: usize,
    #[serde(default = "d_wait")]
    max_wait_s: f64,
}
fn d_wait() -> f64 {
    6.0
}
#[derive(Deserialize)]
struct ScanCfg {
    s: usize,
    base: usize,
    sorter: usize,
}
#[derive(Deserialize)]
struct SorterCfg {
    name: String,
    stages: Vec<StageCfg>,
    #[serde(default)]
    recirc_ir: Option<usize>,
}
#[derive(Deserialize)]
struct StageCfg {
    d: usize,
    pe: usize,
    /// "route": posizione = destinazione; "chute": 1 = devia nello scivolo, 0 = prosegue.
    kind: String,
    #[serde(default)]
    chute: u16,
    #[serde(default)]
    full_pe: Option<usize>,
    #[serde(default)]
    routes: Vec<u16>,
    #[serde(default)]
    default: u16,
}
#[derive(Deserialize)]
struct CounterCfg {
    pe: usize,
    ir: usize,
}

// ------------------------------------------------------------------ logica

/// Collo inseguito: ReqID, destinazione ricevuta dal WCS (None = non ancora / mai), istante della lettura.
type Entry = (u16, Option<u16>, Instant);

/// Una voce più vecchia di così non corrisponde più a un collo in arrivo (collo rimosso,
/// simulazione azzerata): viene scartata per non sfasare l'inseguimento.
const MAX_AGE: Duration = Duration::from_secs(30);

struct State {
    hb: u16,
    hb_at: Option<Instant>,
    sim_prev: bool,
    pe_prev: Vec<bool>,
    occ_since: Vec<Option<Instant>>,
    run_since: Vec<Option<Instant>>,
    gated_since: Vec<Option<Instant>>,
    burst_until: Vec<Option<Instant>>,
    last_scan: Vec<Option<u16>>,
    req: Vec<u16>,
    queues: Vec<Vec<VecDeque<Entry>>>,
}

fn cycle(cfg: &Cfg, m: &mut Mem, st: &mut State, now: Instant) {
    // simulazione attiva con heartbeat vivo
    if m.hr[99] != st.hb || st.hb_at.is_none() {
        st.hb = m.hr[99];
        st.hb_at = Some(now);
    }
    let sim_ok = m.coils[99] && st.hb_at.is_some_and(|t| now - t < Duration::from_secs(2));
    if sim_ok && !st.sim_prev {
        // la simulazione (ri)parte con un impianto nuovo: inseguimento da zero
        for q in st.queues.iter_mut().flatten() {
            q.clear();
        }
        println!("[hub] simulazione attiva: inseguimento azzerato");
    }
    st.sim_prev = sim_ok;
    if m.coils[30] {
        m.coils[30] = false;
        for c in &cfg.conveyors {
            m.di[1000 + c.n] = false;
        }
        for q in st.queues.iter_mut().flatten() {
            q.clear();
        }
        println!("[hub] reset guasti e inseguimento");
    }
    let pe = |m: &Mem, k: usize| m.coils[2000 + k];
    let mut rise = vec![false; st.pe_prev.len()];
    for (k, r) in rise.iter_mut().enumerate() {
        let v = pe(m, k);
        *r = v && !st.pe_prev[k];
        st.pe_prev[k] = v;
        st.occ_since[k] = if v { Some(st.occ_since[k].unwrap_or(now)) } else { None };
    }

    // nastri
    for c in &cfg.conveyors {
        let fault = m.di[1000 + c.n];
        let mut run = m.coils[100 + c.line] && !fault;
        if let Some(g) = &c.gate {
            if pe(m, g.main_pe) {
                let since = *st.gated_since[c.n].get_or_insert(now);
                if st.burst_until[c.n].is_some_and(|t| now < t) {
                    // anti-blocco: passa comunque per qualche istante
                } else if now - since > Duration::from_secs_f64(g.max_wait_s) {
                    st.burst_until[c.n] = Some(now + Duration::from_secs(2));
                    st.gated_since[c.n] = None;
                } else {
                    run = false;
                }
            } else {
                st.gated_since[c.n] = None;
            }
        }
        m.coils[1000 + c.n] = run;
        // controllo velocità (solo con il layer fisico attivo)
        if run && sim_ok {
            let t = *st.run_since[c.n].get_or_insert(now);
            if now - t > Duration::from_millis(2500) && m.hr[1000 + c.n] == 0 {
                m.di[1000 + c.n] = true;
                m.coils[1000 + c.n] = false;
                println!("[hub] {}: marcia comandata senza velocità reale → guasto motore", c.name);
            }
        } else {
            st.run_since[c.n] = None;
        }
    }
    for l in &cfg.lines {
        let faults = cfg.conveyors.iter().any(|c| c.line == l.n && m.di[1000 + c.n]);
        m.di[100 + l.n] = m.coils[100 + l.n] && !faults;
        let _ = &l.name;
    }

    // scanner → richiesta al WCS e inseguimento
    for sc in &cfg.scanners {
        let b = 2000 + 16 * sc.s;
        let cnt = m.hr[b];
        if st.last_scan[sc.s].is_none() {
            st.last_scan[sc.s] = Some(cnt);
        }
        if st.last_scan[sc.s] != Some(cnt) {
            st.last_scan[sc.s] = Some(cnt);
            let r = st.req[sc.s].checked_add(1).filter(|r| *r < 32768).unwrap_or(1);
            st.req[sc.s] = r;
            m.hr[sc.base + 1] = m.hr[b + 11];
            for i in 0..10 {
                m.hr[sc.base + 2 + i] = m.hr[b + 1 + i];
            }
            m.hr[sc.base] = r; // ReqID per ultimo
            if let Some(q) = st.queues.get_mut(sc.sorter).and_then(|s| s.first_mut()) {
                q.push_back((r, None, now));
            }
        }
        // risposta del WCS
        let resp = m.hr[sc.base + 12];
        let dest = m.hr[sc.base + 13];
        if let Some(e) = st.queues.get_mut(sc.sorter).into_iter().flatten().flatten().find(|e| e.0 == resp && e.1.is_none()) {
            e.1 = Some(dest);
        }
    }

    // deviatori
    for (si, so) in cfg.sorters.iter().enumerate() {
        for (k, stg) in so.stages.iter().enumerate() {
            if !rise[stg.pe] {
                continue;
            }
            let q = &mut st.queues[si][k];
            while q.front().is_some_and(|e| now - e.2 > MAX_AGE) {
                let e = q.pop_front().unwrap();
                println!("[hub] {}: voce {} scaduta, scartata", so.name, e.0);
            }
            let e = q.pop_front().unwrap_or((0, None, now));
            match stg.kind.as_str() {
                "route" => {
                    let pos = e.1.filter(|d| stg.routes.contains(d)).unwrap_or(stg.default);
                    m.hr[1100 + stg.d] = pos;
                }
                _ => {
                    // scivolo pieno: fotocellula d'ingresso occupata da più di 1,5 s (coda ferma)
                    let full = stg.full_pe.is_some_and(|p| st.occ_since[p].is_some_and(|t| now - t > Duration::from_millis(1500)));
                    if e.1 == Some(stg.chute) && !full {
                        m.hr[1100 + stg.d] = 1;
                    } else {
                        m.hr[1100 + stg.d] = 0;
                        if k + 1 < so.stages.len() {
                            st.queues[si][k + 1].push_back(e);
                        } else if let Some(ir) = so.recirc_ir {
                            m.ir[ir] = m.ir[ir].wrapping_add(1);
                            let _ = &so.name;
                        }
                    }
                }
            }
        }
    }
    // contatori
    for c in &cfg.counters {
        if rise[c.pe] {
            m.ir[c.ir] = m.ir[c.ir].wrapping_add(1);
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| "config/hub/plc_hub.json".into());
    let bind: SocketAddr = std::env::args().nth(2).unwrap_or_else(|| "0.0.0.0:5021".into()).parse()?;
    let cfg: Cfg = serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| anyhow::anyhow!("{path}: {e} (genera la demo con tools/build_demo_hub.py)"))?)?;
    let n_pe = cfg.sorters.iter().flat_map(|s| s.stages.iter().flat_map(|t| [Some(t.pe), t.full_pe])).flatten()
        .chain(cfg.counters.iter().map(|c| c.pe))
        .chain(cfg.conveyors.iter().filter_map(|c| c.gate.as_ref().map(|g| g.main_pe)))
        .max()
        .map_or(0, |m| m + 1);
    let n_conv = cfg.conveyors.iter().map(|c| c.n + 1).max().unwrap_or(0);
    let n_scan = cfg.scanners.iter().map(|s| s.s + 1).max().unwrap_or(0);
    let mut st = State {
        hb: 0,
        hb_at: None,
        sim_prev: false,
        pe_prev: vec![false; n_pe],
        occ_since: vec![None; n_pe],
        run_since: vec![None; n_conv],
        gated_since: vec![None; n_conv],
        burst_until: vec![None; n_conv],
        last_scan: vec![None; n_scan],
        req: vec![0; n_scan],
        queues: cfg.sorters.iter().map(|s| vec![VecDeque::new(); s.stages.len()]).collect(),
    };
    let mem = Mem { coils: vec![false; N], di: vec![false; N], hr: vec![0; N], ir: vec![0; N] };
    let plc = Plc(Arc::new(Mutex::new(mem)));
    {
        // all'avvio tutte le linee abilitate
        let mut m = plc.0.lock().unwrap();
        for l in &cfg.lines {
            m.coils[100 + l.n] = true;
        }
    }
    println!(
        "[hub] {} linee, {} nastri, {} scanner, {} sorter, {} contatori",
        cfg.lines.len(),
        cfg.conveyors.len(),
        cfg.scanners.len(),
        cfg.sorters.len(),
        cfg.counters.len()
    );
    let p2 = plc.clone();
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(20));
            let mut m = p2.0.lock().unwrap();
            cycle(&cfg, &mut m, &mut st, Instant::now());
        }
    });

    let listener = TcpListener::bind(bind).await?;
    println!("[hub] PLC Modbus TCP dell'hub in ascolto su {bind}");
    let server = Server::new(listener);
    let new_service = move |_addr| Ok(Some(plc.clone()));
    let on_connected = |stream, addr| {
        let ns = new_service.clone();
        async move { accept_tcp_connection(stream, addr, ns) }
    };
    server.serve(&on_connected, |e| eprintln!("[hub] errore: {e}")).await?;
    Ok(())
}
