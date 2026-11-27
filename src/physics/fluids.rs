//! Layer fisico – fluidi: serbatoi, alimentazioni, scarichi, giunzioni, valvole, pompe,
//! tubazioni, riscaldatori e motori (agitatori).
//!
//! Modello a parametri concentrati:
//! - **nodi**: serbatoi (pressione sul fondo = ρ·g·(quota + livello)), alimentazioni a
//!   pressione fissa, scarichi (0 bar), giunzioni senza volume (pressione incognita);
//! - **rami**: tubazioni con valvole e al più una pompa in serie. Kv equivalente
//!   1/Kv² = 1/Kv_tubo² + Σ 1/(Kv_i·apertura_i)², portata Q = Kv·√(ΔP/SG) (m³/h, bar);
//!   la pompa aggiunge la prevalenza H(Q) = H0·s²·(1 − (Q/(Qmax·s))²), con s = velocità;
//!   una pompa ferma blocca il ramo, una pompa non fa passare flusso all'indietro;
//! - **giunzioni**: pressione tale che Σ portate = 0 (bisezione, più passate se collegate);
//! - **serbatoi**: bilancio di massa (con troppo pieno e perdite) e di energia (miscelazione,
//!   riscaldatori con termostato locale, dispersione verso l'ambiente).

use super::{RawComp, full_key};
use crate::value::Value;
use serde_json::{Value as J, json};
use std::collections::{BTreeMap, HashMap};

const G: f64 = 9.81;

/// Controllo dei tag: (chiave componente, descrizione, tag, errori) → tag normalizzato.
pub(super) type Check<'a> = &'a dyn Fn(&str, &str, &str, &mut Vec<String>) -> String;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Node {
    Tank(usize),
    Supply(usize),
    Drain,
    Junction(usize),
}

#[derive(Debug, Clone)]
struct Tank {
    key: String,
    vol: f64,
    area: f64,
    elev: f64,
    level_tag: String,
    level_max: f64,
    temp_tag: String,
    pressure_tag: String,
    init_v: f64,
    init_t: f64,
    loss: f64,
    v: f64,
    t: f64,
    leak: f64,
    spilled: f64,
}

#[derive(Debug, Clone)]
struct Supply {
    key: String,
    p: f64,
    t: f64,
}

#[derive(Debug, Clone)]
struct Junction {
    key: String,
    p: f64,
}

#[derive(Debug, Clone)]
struct Valve {
    key: String,
    cmd: String,
    analog: bool,
    invert: bool,
    kv: f64,
    stroke: f64,
    fb_open: String,
    fb_closed: String,
    fb_pos: String,
    pos: f64,
    stuck: bool,
}

#[derive(Debug, Clone)]
struct Pump {
    key: String,
    run: String,
    sp: String,
    sp_max: f64,
    head: f64,
    qmax: f64,
    fb_run: String,
    s: f64,
    tripped: bool,
    dry: bool,
}

#[derive(Debug, Clone)]
struct Pipe {
    key: String,
    view: String,
    id: String,
    from: Node,
    to: Node,
    valves: Vec<usize>,
    pump: Option<usize>,
    kv: f64,
    flow_tag: String,
    flow_scale: f64,
    q: f64,
}

#[derive(Debug, Clone)]
struct Heater {
    key: String,
    tank: usize,
    cmd: String,
    setpoint: String,
    hyst: f64,
    kw: f64,
    on: bool,
    failed: bool,
}

#[derive(Debug, Clone)]
struct Motor {
    key: String,
    run: String,
    sp: String,
    sp_max: f64,
    rpm: f64,
    ramp: f64,
    fb: String,
    v: f64,
    failed: bool,
}

/// Rete dei fluidi compilata dalle viste.
#[derive(Debug, Default)]
pub(super) struct Fluids {
    tanks: Vec<Tank>,
    supplies: Vec<Supply>,
    juncs: Vec<Junction>,
    valves: Vec<Valve>,
    pumps: Vec<Pump>,
    pipes: Vec<Pipe>,
    heaters: Vec<Heater>,
    motors: Vec<Motor>,
    rho: f64,
    cp: f64,
    amb: f64,
    /// Ultimo valore analogico scritto per tag (banda morta).
    last: HashMap<String, f64>,
}

pub(super) const KINDS: &[&str] = &["tank", "supply", "drain", "junction", "valve", "pump", "pipe", "heater", "motor"];

impl Fluids {
    pub(super) fn is_empty(&self) -> bool {
        self.tanks.is_empty() && self.pipes.is_empty() && self.motors.is_empty()
    }

    pub(super) fn build(raw: &[RawComp], rho: f64, cp: f64, amb: f64, actuator: Check, sensor: Check, errs: &mut Vec<String>) -> Fluids {
        let mut f = Fluids { rho, cp, amb, ..Default::default() };
        let key_of = |r: &RawComp| format!("{}/{}", r.view, r.id);
        let mut nodes: HashMap<String, Node> = HashMap::new();
        let mut drains = 0;
        // nodi
        for r in raw {
            let (k, d) = (key_of(r), &r.def);
            let n0 = errs.len();
            match d.kind.as_str() {
                "tank" => {
                    let vol = d.volume_m3.filter(|v| *v > 0.0).unwrap_or(1.0);
                    let height = d.height_m.filter(|v| *v > 0.0).unwrap_or(1.5);
                    let t = Tank {
                        key: k.clone(),
                        vol,
                        area: vol / height,
                        elev: d.elevation_m.unwrap_or(0.0),
                        level_tag: sensor(&k, "livello", &d.level_tag, errs),
                        level_max: d.level_max.filter(|v| *v > 0.0).unwrap_or(100.0),
                        temp_tag: sensor(&k, "temperatura", &d.temp_tag, errs),
                        pressure_tag: sensor(&k, "pressione", &d.pressure_tag, errs),
                        init_v: vol * d.init_level.unwrap_or(0.0).clamp(0.0, 100.0) / 100.0,
                        init_t: d.init_temp.unwrap_or(amb),
                        loss: d.loss_w_k.unwrap_or(30.0).max(0.0),
                        v: 0.0,
                        t: 0.0,
                        leak: 0.0,
                        spilled: 0.0,
                    };
                    if errs.len() == n0 {
                        nodes.insert(k, Node::Tank(f.tanks.len()));
                        f.tanks.push(Tank { v: t.init_v, t: t.init_t, ..t });
                    }
                }
                "supply" => {
                    nodes.insert(k.clone(), Node::Supply(f.supplies.len()));
                    f.supplies.push(Supply { key: k, p: d.pressure_bar.unwrap_or(3.0), t: d.temp_c.unwrap_or(15.0) });
                }
                "drain" => {
                    nodes.insert(k, Node::Drain);
                    drains += 1;
                }
                "junction" => {
                    nodes.insert(k.clone(), Node::Junction(f.juncs.len()));
                    f.juncs.push(Junction { key: k, p: 0.0 });
                }
                _ => {}
            }
        }
        let _ = drains;
        // elementi in linea e utenze
        let mut valve_idx: HashMap<String, usize> = HashMap::new();
        let mut pump_idx: HashMap<String, usize> = HashMap::new();
        for r in raw {
            let (k, d) = (key_of(r), &r.def);
            let n0 = errs.len();
            match d.kind.as_str() {
                "valve" => {
                    let cmd = actuator(&k, if d.analog { "apertura %" } else { "comando apertura" }, if d.analog { &d.position } else { &d.cmd }, errs);
                    if cmd.is_empty() {
                        errs.push(format!("{k}: valvola senza tag di comando ({})", if d.analog { "apertura" } else { "comando" }));
                    }
                    let v = Valve {
                        key: k.clone(),
                        cmd,
                        analog: d.analog,
                        invert: d.invert,
                        kv: d.kv.filter(|v| *v > 0.0).unwrap_or(10.0),
                        stroke: d.time_s.unwrap_or(2.0).max(0.0),
                        fb_open: sensor(&k, "finecorsa aperta", &d.fb_open, errs),
                        fb_closed: sensor(&k, "finecorsa chiusa", &d.fb_closed, errs),
                        fb_pos: sensor(&k, "posizione", &d.fb_pos, errs),
                        pos: 0.0,
                        stuck: false,
                    };
                    if errs.len() == n0 {
                        valve_idx.insert(k, f.valves.len());
                        f.valves.push(v);
                    }
                }
                "pump" => {
                    let p = Pump {
                        key: k.clone(),
                        run: actuator(&k, "marcia", &d.run, errs),
                        sp: actuator(&k, "setpoint velocità", &d.speed_sp, errs),
                        sp_max: d.sp_max.filter(|v| *v > 0.0).unwrap_or(100.0),
                        head: d.head_m.filter(|v| *v > 0.0).unwrap_or(20.0),
                        qmax: d.q_max.filter(|v| *v > 0.0).unwrap_or(10.0),
                        fb_run: sensor(&k, "in marcia", &d.fb_run, errs),
                        s: 0.0,
                        tripped: false,
                        dry: false,
                    };
                    if p.run.is_empty() {
                        errs.push(format!("{k}: pompa senza tag di marcia"));
                    }
                    if errs.len() == n0 {
                        pump_idx.insert(k, f.pumps.len());
                        f.pumps.push(p);
                    }
                }
                "motor" => {
                    let m = Motor {
                        key: k.clone(),
                        run: actuator(&k, "marcia", &d.run, errs),
                        sp: actuator(&k, "setpoint velocità", &d.speed_sp, errs),
                        sp_max: d.sp_max.filter(|v| *v > 0.0).unwrap_or(100.0),
                        rpm: d.rpm.filter(|v| *v > 0.0).unwrap_or(100.0),
                        ramp: d.time_s.unwrap_or(2.0).max(0.0),
                        fb: sensor(&k, "velocità reale", &d.speed_fb, errs),
                        v: 0.0,
                        failed: false,
                    };
                    if m.run.is_empty() {
                        errs.push(format!("{k}: motore senza tag di marcia"));
                    }
                    if errs.len() == n0 {
                        f.motors.push(m);
                    }
                }
                _ => {}
            }
        }
        for r in raw {
            let (k, d) = (key_of(r), &r.def);
            let n0 = errs.len();
            match d.kind.as_str() {
                "pipe" => {
                    let node = |id: &str, what: &str, errs: &mut Vec<String>| -> Option<Node> {
                        if id.trim().is_empty() {
                            errs.push(format!("{k}: tubazione senza '{what}'"));
                            return None;
                        }
                        let nk = full_key(&r.view, id.trim());
                        let n = nodes.get(&nk).copied();
                        if n.is_none() {
                            errs.push(format!("{k}: '{what}' = '{}' non è un serbatoio, un'alimentazione, uno scarico o una giunzione", id.trim()));
                        }
                        n
                    };
                    let from = node(&d.from, "da", errs);
                    let to = node(&d.to, "a", errs);
                    let mut valves = vec![];
                    let mut pump = None;
                    for e in d.elements.split(',').map(str::trim).filter(|e| !e.is_empty()) {
                        let ek = full_key(&r.view, e);
                        if let Some(i) = valve_idx.get(&ek) {
                            valves.push(*i);
                        } else if let Some(i) = pump_idx.get(&ek) {
                            if pump.replace(*i).is_some() {
                                errs.push(format!("{k}: al massimo una pompa per tubazione (dividi con una giunzione)"));
                            }
                        } else {
                            errs.push(format!("{k}: elemento '{e}' non è una valvola o una pompa"));
                        }
                    }
                    let flow_tag = sensor(&k, "portata", &d.flow_tag, errs);
                    if let (Some(from), Some(to), true) = (from, to, errs.len() == n0) {
                        if from == to {
                            errs.push(format!("{k}: 'da' e 'a' coincidono"));
                        } else {
                            f.pipes.push(Pipe {
                                key: k,
                                view: r.view.clone(),
                                id: r.id.clone(),
                                from,
                                to,
                                valves,
                                pump,
                                kv: d.kv.filter(|v| *v > 0.0).unwrap_or(100.0),
                                flow_tag,
                                flow_scale: d.flow_scale.unwrap_or(1.0),
                                q: 0.0,
                            });
                        }
                    }
                }
                "heater" => {
                    let tank = match nodes.get(&full_key(&r.view, d.on.trim())) {
                        Some(Node::Tank(i)) => Some(*i),
                        _ => {
                            errs.push(format!("{k}: riscaldatore senza serbatoio ('Nel serbatoio')"));
                            None
                        }
                    };
                    let cmd = actuator(&k, "comando", &d.cmd, errs);
                    if cmd.is_empty() {
                        errs.push(format!("{k}: riscaldatore senza tag di comando"));
                    }
                    let setpoint = actuator(&k, "setpoint termostato", &d.setpoint_tag, errs);
                    if let (Some(tank), true) = (tank, errs.len() == n0) {
                        f.heaters.push(Heater { key: k, tank, cmd, setpoint, hyst: d.hysteresis.unwrap_or(1.0).max(0.0), kw: d.power_kw.unwrap_or(10.0).max(0.0), on: false, failed: false });
                    }
                }
                _ => {}
            }
        }
        f
    }

    /// Mantiene lo stato (livelli, temperature, aperture, velocità) di un modello precedente.
    pub(super) fn carry_from(&mut self, old: &Fluids) {
        for t in &mut self.tanks {
            if let Some(o) = old.tanks.iter().find(|o| o.key == t.key) {
                (t.v, t.t, t.leak) = (o.v.min(t.vol), o.t, o.leak);
            }
        }
        for v in &mut self.valves {
            if let Some(o) = old.valves.iter().find(|o| o.key == v.key) {
                (v.pos, v.stuck) = (o.pos, o.stuck);
            }
        }
        for p in &mut self.pumps {
            if let Some(o) = old.pumps.iter().find(|o| o.key == p.key) {
                (p.s, p.tripped) = (o.s, o.tripped);
            }
        }
        for m in &mut self.motors {
            if let Some(o) = old.motors.iter().find(|o| o.key == m.key) {
                (m.v, m.failed) = (o.v, o.failed);
            }
        }
        for h in &mut self.heaters {
            if let Some(o) = old.heaters.iter().find(|o| o.key == h.key) {
                (h.on, h.failed) = (o.on, o.failed);
            }
        }
        self.last = old.last.clone();
    }

    /// Torna allo stato iniziale (livelli e temperature iniziali, tutto fermo).
    pub(super) fn reset(&mut self) {
        for t in &mut self.tanks {
            (t.v, t.t, t.leak, t.spilled) = (t.init_v, t.init_t, 0.0, 0.0);
        }
        for v in &mut self.valves {
            v.pos = 0.0;
        }
        for p in &mut self.pumps {
            (p.s, p.tripped) = (0.0, false);
        }
        for m in &mut self.motors {
            m.v = 0.0;
        }
        for p in &mut self.pipes {
            p.q = 0.0;
        }
        self.last.clear();
    }

    fn node_p(&self, n: Node) -> f64 {
        match n {
            Node::Tank(i) => {
                let t = &self.tanks[i];
                self.rho * G * (t.elev + t.v / t.area) / 1e5
            }
            Node::Supply(i) => self.supplies[i].p,
            Node::Drain => 0.0,
            Node::Junction(i) => self.juncs[i].p,
        }
    }

    /// Portata del ramo (m³/h, positiva da → a) date le pressioni di monte e valle.
    fn pipe_q(&self, p: &Pipe, pf: f64, pt: f64) -> f64 {
        let mut inv = 1.0 / (p.kv * p.kv);
        for &v in &p.valves {
            let o = self.valves[v].pos;
            if o < 1e-3 {
                return 0.0;
            }
            let k = self.valves[v].kv * o;
            inv += 1.0 / (k * k);
        }
        let kv = 1.0 / inv.sqrt();
        let sg = self.rho / 1000.0;
        let dps = pf - pt;
        match p.pump.map(|i| &self.pumps[i]) {
            Some(pu) => {
                let s = pu.s;
                if s < 0.02 || pu.tripped {
                    return 0.0; // pompa ferma: ramo bloccato
                }
                let h0 = self.rho * G * pu.head * s * s / 1e5;
                let qm = pu.qmax * s;
                let f = |q: f64| kv * ((dps + h0 * (1.0 - (q / qm).powi(2))).max(0.0) / sg).sqrt() - q;
                if f(0.0) <= 0.0 {
                    return 0.0;
                }
                if f(qm) >= 0.0 {
                    return qm;
                }
                let (mut a, mut b) = (0.0, qm);
                for _ in 0..30 {
                    let m = (a + b) / 2.0;
                    if f(m) > 0.0 { a = m } else { b = m }
                }
                (a + b) / 2.0
            }
            None => dps.signum() * kv * (dps.abs() / sg).sqrt(),
        }
    }

    /// Pressione delle giunzioni: Σ portate entranti = Σ uscenti.
    fn solve_junctions(&mut self) {
        if self.juncs.is_empty() {
            return;
        }
        let fixed: Vec<f64> = self.tanks.iter().enumerate().map(|(i, _)| self.node_p(Node::Tank(i))).chain(self.supplies.iter().map(|s| s.p)).collect();
        let head = self.pumps.iter().map(|p| self.rho * G * p.head / 1e5).fold(0.0, f64::max);
        let lo = fixed.iter().copied().fold(0.0, f64::min) - 0.5;
        let hi = fixed.iter().copied().fold(0.0, f64::max) + head + 0.5;
        for _ in 0..4 {
            for j in 0..self.juncs.len() {
                let me = Node::Junction(j);
                let net = |s: &Fluids, pj: f64| -> f64 {
                    s.pipes.iter().map(|p| {
                        if p.to == me {
                            s.pipe_q(p, s.node_p(p.from), pj)
                        } else if p.from == me {
                            -s.pipe_q(p, pj, s.node_p(p.to))
                        } else {
                            0.0
                        }
                    }).sum()
                };
                if net(self, lo).abs() < 1e-9 && net(self, hi).abs() < 1e-9 {
                    // nessun ramo può scorrere (valvole chiuse, pompe ferme): pressione media dei vicini
                    let nb: Vec<f64> = self.pipes.iter().filter_map(|p| if p.to == me { Some(p.from) } else if p.from == me { Some(p.to) } else { None })
                        .filter(|n| !matches!(n, Node::Junction(_))).map(|n| self.node_p(n)).collect();
                    self.juncs[j].p = if nb.is_empty() { 0.0 } else { nb.iter().sum::<f64>() / nb.len() as f64 };
                    continue;
                }
                let (mut a, mut b) = (lo, hi);
                for _ in 0..40 {
                    let m = (a + b) / 2.0;
                    if net(self, m) > 0.0 { a = m } else { b = m }
                }
                self.juncs[j].p = (a + b) / 2.0;
            }
        }
    }

    /// Un passo di `dt` secondi: attuatori, rete, serbatoi, sensori.
    pub(super) fn step(&mut self, dt: f64, read: &dyn Fn(&str) -> Value) -> Vec<(String, Value)> {
        let mut out = vec![];
        let b = |t: &str| !t.is_empty() && read(t).as_bool().unwrap_or(false);
        let num = |t: &str| read(t).as_f64().unwrap_or(0.0);
        // --- attuatori
        for v in &mut self.valves {
            if v.stuck {
                continue;
            }
            let target = if v.analog { (num(&v.cmd) / 100.0).clamp(0.0, 1.0) } else if b(&v.cmd) != v.invert { 1.0 } else { 0.0 };
            let rate = if v.stroke <= 0.0 { 1e9 } else { dt / v.stroke };
            v.pos = if v.pos < target { (v.pos + rate).min(target) } else { (v.pos - rate).max(target) };
        }
        for p in &mut self.pumps {
            let k = if p.sp.is_empty() { 1.0 } else { (num(&p.sp) / p.sp_max).clamp(0.0, 1.0) };
            let target = if b(&p.run) && !p.tripped { k } else { 0.0 };
            p.s = if p.s < target { (p.s + dt).min(target) } else { (p.s - dt * 2.0).max(target) };
        }
        for m in &mut self.motors {
            let k = if m.sp.is_empty() { 1.0 } else { (num(&m.sp) / m.sp_max).clamp(0.0, 1.0) };
            let target = if b(&m.run) && !m.failed { k * m.rpm } else { 0.0 };
            let rate = if m.ramp <= 0.0 { 1e12 } else { m.rpm * dt / m.ramp };
            m.v = if m.v < target { (m.v + rate).min(target) } else { (m.v - rate).max(target) };
        }
        for h in &mut self.heaters {
            let t = self.tanks[h.tank].t;
            let cmd = b(&h.cmd) && !h.failed;
            h.on = if !cmd {
                false
            } else if h.setpoint.is_empty() {
                true
            } else {
                let sp = num(&h.setpoint);
                if t < sp - h.hyst / 2.0 { true } else if t > sp + h.hyst / 2.0 { false } else { h.on }
            };
        }
        // --- rete
        self.solve_junctions();
        let qs: Vec<f64> = self.pipes.iter().map(|p| self.pipe_q(p, self.node_p(p.from), self.node_p(p.to))).collect();
        for (p, q) in self.pipes.iter_mut().zip(qs) {
            p.q = q;
        }
        // un serbatoio non può dare più di quanto contiene
        for (ti, t) in self.tanks.iter().enumerate() {
            let me = Node::Tank(ti);
            let outq: f64 = self.pipes.iter().map(|p| if p.from == me { p.q.max(0.0) } else if p.to == me { (-p.q).max(0.0) } else { 0.0 }).sum();
            let avail = t.v * 3600.0 / dt;
            if outq > avail && outq > 0.0 {
                let k = avail / outq;
                for p in self.pipes.iter_mut() {
                    if (p.from == me && p.q > 0.0) || (p.to == me && p.q < 0.0) {
                        p.q *= k;
                    }
                }
            }
        }
        for p in &mut self.pumps {
            p.dry = false;
        }
        for pi in 0..self.pipes.len() {
            let (from, pump, q) = (self.pipes[pi].from, self.pipes[pi].pump, self.pipes[pi].q);
            if let (Some(u), Node::Tank(t)) = (pump, from)
                && self.tanks[t].v < 1e-4
                && self.pumps[u].s > 0.05
            {
                self.pumps[u].dry = true;
            }
            let _ = q;
        }
        // temperatura delle giunzioni (media pesata degli ingressi)
        let src_t = |s: &Fluids, n: Node| -> f64 {
            match n {
                Node::Tank(i) => s.tanks[i].t,
                Node::Supply(i) => s.supplies[i].t,
                _ => s.amb,
            }
        };
        let mut jt = vec![self.amb; self.juncs.len()];
        for (j, jtv) in jt.iter_mut().enumerate() {
            let me = Node::Junction(j);
            let (mut qs, mut qt) = (0.0, 0.0);
            for p in &self.pipes {
                let (q, src) = if p.to == me && p.q > 0.0 { (p.q, p.from) } else if p.from == me && p.q < 0.0 { (-p.q, p.to) } else { continue };
                qs += q;
                qt += q * src_t(self, src);
            }
            if qs > 1e-9 {
                *jtv = qt / qs;
            }
        }
        // --- serbatoi: massa ed energia
        let (rho, cp, amb) = (self.rho, self.cp, self.amb);
        let mut ins: Vec<Vec<(f64, f64)>> = vec![vec![]; self.tanks.len()];
        let mut outs = vec![0.0; self.tanks.len()];
        for p in &self.pipes {
            let vol = p.q.abs() * dt / 3600.0;
            if vol <= 0.0 {
                continue;
            }
            let (src, dst) = if p.q > 0.0 { (p.from, p.to) } else { (p.to, p.from) };
            let t_src = match src {
                Node::Junction(j) => jt[j],
                n => src_t(self, n),
            };
            if let Node::Tank(i) = dst {
                ins[i].push((vol, t_src));
            }
            if let Node::Tank(i) = src {
                outs[i] += vol;
            }
        }
        for (i, t) in self.tanks.iter_mut().enumerate() {
            let leak = (t.leak * dt / 3600.0).min((t.v - outs[i]).max(0.0));
            let rem = (t.v - outs[i] - leak).max(0.0);
            let vin: f64 = ins[i].iter().map(|x| x.0).sum();
            let tin: f64 = ins[i].iter().map(|x| x.0 * x.1).sum();
            let mut v = rem + vin;
            if v > 1e-9 {
                t.t = (t.t * rem + tin) / v;
            }
            if v > t.vol {
                t.spilled += v - t.vol;
                v = t.vol;
            }
            t.v = v;
        }
        for h in &self.heaters {
            let t = &mut self.tanks[h.tank];
            if h.on && t.v > t.vol * 0.02 {
                t.t += h.kw * 1000.0 * dt / (rho * cp * t.v);
            }
        }
        for t in &mut self.tanks {
            let c = rho * cp * t.v.max(t.vol * 0.01);
            t.t = amb + (t.t - amb) * (-t.loss * dt / c).exp();
        }
        // --- sensori
        let mut aw = |out: &mut Vec<(String, Value)>, tag: &str, v: f64, db: f64| {
            if tag.is_empty() {
                return;
            }
            if self.last.get(tag).is_none_or(|l| (l - v).abs() >= db) {
                self.last.insert(tag.to_string(), v);
                out.push((tag.to_string(), Value::Float((v * 1000.0).round() / 1000.0)));
            }
        };
        for t in &self.tanks {
            aw(&mut out, &t.level_tag, t.v / t.vol * t.level_max, t.level_max * 0.001);
            aw(&mut out, &t.temp_tag, t.t, 0.02);
            aw(&mut out, &t.pressure_tag, rho * G * (t.v / t.area) / 1e5, 0.0005);
        }
        for p in &self.pipes {
            aw(&mut out, &p.flow_tag, p.q * p.flow_scale, 0.01 * p.flow_scale.abs().max(1e-9));
        }
        for v in &self.valves {
            aw(&mut out, &v.fb_pos, v.pos * 100.0, 0.5);
            for (tag, on) in [(&v.fb_open, v.pos >= 0.98), (&v.fb_closed, v.pos <= 0.02)] {
                if !tag.is_empty() {
                    out.push((tag.clone(), Value::Bool(on)));
                }
            }
        }
        for p in &self.pumps {
            if !p.fb_run.is_empty() {
                out.push((p.fb_run.clone(), Value::Bool(p.s > 0.05 && !p.tripped)));
            }
        }
        for m in &self.motors {
            aw(&mut out, &m.fb, m.v, (m.rpm * 0.002).max(1e-6));
        }
        out
    }

    /// Ramificazioni con flusso, per vista: [id componente, portata m³/h].
    pub(super) fn flows(&self, views: &mut BTreeMap<String, Vec<J>>) {
        for p in self.pipes.iter().filter(|p| p.q.abs() > 0.01) {
            views.entry(p.view.clone()).or_default().push(json!([p.id, (p.q * 100.0).round() / 100.0]));
        }
    }

    pub(super) fn status(&self, comps: &mut Vec<J>) {
        let r2 = |x: f64| (x * 100.0).round() / 100.0;
        for t in &self.tanks {
            comps.push(json!({"key": t.key, "kind": "tank", "level": r2(t.v / t.vol * 100.0), "volume": r2(t.v), "vol": t.vol, "temp": r2(t.t),
                "pressure": (self.rho * G * (t.v / t.area) / 1e5 * 1000.0).round() / 1000.0, "leak": t.leak, "spilled": r2(t.spilled)}));
        }
        for s in &self.supplies {
            comps.push(json!({"key": s.key, "kind": "supply", "p": s.p, "t": s.t}));
        }
        for j in &self.juncs {
            comps.push(json!({"key": j.key, "kind": "junction", "p": (j.p * 1000.0).round() / 1000.0}));
        }
        for v in &self.valves {
            comps.push(json!({"key": v.key, "kind": "valve", "pos": r2(v.pos * 100.0), "kv": v.kv, "stuck": v.stuck, "cmd": v.cmd}));
        }
        for p in &self.pumps {
            comps.push(json!({"key": p.key, "kind": "pump", "speed": r2(p.s * 100.0), "tripped": p.tripped, "dry": p.dry, "qmax": p.qmax, "head": p.head}));
        }
        for p in &self.pipes {
            comps.push(json!({"key": p.key, "kind": "pipe", "q": r2(p.q)}));
        }
        for h in &self.heaters {
            comps.push(json!({"key": h.key, "kind": "heater", "on": h.on, "kw": h.kw, "failed": h.failed, "tank": self.tanks[h.tank].key}));
        }
        for m in &self.motors {
            comps.push(json!({"key": m.key, "kind": "motor", "speed": r2(m.v), "rpm": m.rpm, "failed": m.failed}));
        }
    }

    /// Guasti e interventi sui componenti di processo. None = azione non dei fluidi.
    pub(super) fn inject(&mut self, action: &str, key: &str, req: &J) -> Option<anyhow::Result<String>> {
        let flag = |k: &str| req.get(k).and_then(J::as_bool).unwrap_or(false);
        let nf = || anyhow::anyhow!("componente '{key}' non trovato");
        Some(match action {
            "valve" => self.valves.iter_mut().find(|v| v.key == key).ok_or_else(nf).map(|v| {
                v.stuck = flag("stuck");
                format!("{key}: {}", if v.stuck { format!("bloccata al {:.0} %", v.pos * 100.0) } else { "sbloccata".into() })
            }),
            "pump" => self.pumps.iter_mut().find(|p| p.key == key).ok_or_else(nf).map(|p| {
                p.tripped = flag("tripped");
                format!("{key}: {}", if p.tripped { "scatto termico (ferma)" } else { "riarmata" })
            }),
            "heater" => self.heaters.iter_mut().find(|h| h.key == key).ok_or_else(nf).map(|h| {
                h.failed = flag("failed");
                format!("{key}: {}", if h.failed { "guasto (non scalda)" } else { "ripristinato" })
            }),
            "motor" => self.motors.iter_mut().find(|m| m.key == key).ok_or_else(nf).map(|m| {
                m.failed = flag("failed");
                format!("{key}: {}", if m.failed { "motore in avaria" } else { "motore ripristinato" })
            }),
            "tank" => self.tanks.iter_mut().find(|t| t.key == key).ok_or_else(nf).map(|t| {
                let mut msg = vec![];
                if let Some(l) = req.get("leak").and_then(J::as_f64) {
                    t.leak = l.max(0.0);
                    msg.push(if t.leak > 0.0 { format!("perdita {} m³/h", t.leak) } else { "perdita chiusa".into() });
                }
                if let Some(l) = req.get("level").and_then(J::as_f64) {
                    t.v = t.vol * l.clamp(0.0, 100.0) / 100.0;
                    msg.push(format!("livello impostato al {l} %"));
                }
                if let Some(x) = req.get("temp").and_then(J::as_f64) {
                    t.t = x;
                    msg.push(format!("temperatura impostata a {x} °C"));
                }
                format!("{key}: {}", if msg.is_empty() { "nessuna modifica".into() } else { msg.join(", ") })
            }),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::PhysDef;
    use std::cell::RefCell;

    fn raw(view: &str, id: &str, def: J) -> RawComp {
        RawComp { view: view.into(), id: id.into(), geo: Default::default(), def: serde_json::from_value::<PhysDef>(def).unwrap() }
    }

    fn net() -> Fluids {
        let ok: Check = &|_k, _w, t, _e| t.trim().to_string();
        let comps = vec![
            raw("p", "W", json!({"kind": "supply", "pressure_bar": 3.0, "temp_c": 15.0})),
            raw("p", "T1", json!({"kind": "tank", "volume_m3": 2.0, "height_m": 2.0, "level_tag": "T1.L", "temp_tag": "T1.T", "init_level": 0, "init_temp": 20})),
            raw("p", "T2", json!({"kind": "tank", "volume_m3": 4.0, "height_m": 2.0, "elevation_m": 0.0, "level_tag": "T2.L", "init_level": 0})),
            raw("p", "D", json!({"kind": "drain"})),
            raw("p", "V1", json!({"kind": "valve", "cmd": "V1", "kv": 6.0, "time_s": 0.5})),
            raw("p", "V2", json!({"kind": "valve", "cmd": "V2", "kv": 8.0, "time_s": 0.5})),
            raw("p", "P1", json!({"kind": "pump", "run": "P1", "head_m": 20.0, "q_max": 6.0, "fb_run": "P1.Fb"})),
            raw("p", "H1", json!({"kind": "heater", "on": "T1", "cmd": "H1", "power_kw": 300.0, "setpoint_tag": "SP", "hysteresis": 1.0})),
            raw("p", "L1", json!({"kind": "pipe", "from": "W", "to": "T1", "elements": "V1", "flow_tag": "F1"})),
            raw("p", "L2", json!({"kind": "pipe", "from": "T1", "to": "T2", "elements": "V2"})),
            raw("p", "L3", json!({"kind": "pipe", "from": "T2", "to": "D", "elements": "P1"})),
        ];
        let mut errs = vec![];
        let f = Fluids::build(&comps, 1000.0, 4186.0, 20.0, ok, ok, &mut errs);
        assert!(errs.is_empty(), "{errs:?}");
        f
    }

    fn run(f: &mut Fluids, secs: f64, tags: &RefCell<HashMap<&str, Value>>) {
        let mut t = 0.0;
        while t < secs {
            let tg = tags.borrow();
            let read = |n: &str| tg.get(n).cloned().unwrap_or(Value::Bool(false));
            f.step(0.02, &read);
            t += 0.02;
        }
    }

    #[test]
    fn fill_heat_transfer_and_pump_out() {
        let mut f = net();
        let tags = RefCell::new(HashMap::from([("SP", Value::Float(50.0))]));
        // riempimento da rete a 3 bar: Kv 6 → ~10 m³/h a ΔP ≈ 3 bar, temperatura dell'acqua di rete
        tags.borrow_mut().insert("V1", Value::Bool(true));
        run(&mut f, 60.0, &tags);
        let q = f.pipes[0].q;
        assert!(q > 8.0 && q < 11.0, "portata {q}");
        let l1 = f.tanks[0].v;
        assert!(l1 > 0.1 && l1 < 0.2, "volume {l1}");
        assert!((f.tanks[0].t - 15.0).abs() < 0.5, "miscelazione {}", f.tanks[0].t);
        // continua fino a ~1,2 m³, poi chiude
        run(&mut f, 400.0, &tags);
        tags.borrow_mut().insert("V1", Value::Bool(false));
        run(&mut f, 2.0, &tags);
        let v = f.tanks[0].v;
        assert!(v > 1.0 && v <= 2.0, "volume {v}");
        // riscaldamento con termostato a 50 °C: sale e si ferma attorno al setpoint
        tags.borrow_mut().insert("H1", Value::Bool(true));
        run(&mut f, 900.0, &tags);
        let t = f.tanks[0].t;
        assert!((t - 50.0).abs() < 1.5, "termostato {t}");
        // travaso per gravità T1 → T2 fino a livelli (pressioni) uguali
        tags.borrow_mut().insert("V2", Value::Bool(true));
        run(&mut f, 3000.0, &tags);
        let (h1, h2) = (f.tanks[0].v / f.tanks[0].area, f.tanks[1].v / f.tanks[1].area);
        assert!((h1 - h2).abs() < 0.02, "vasi comunicanti {h1} {h2}");
        assert!((f.tanks[0].v + f.tanks[1].v - v).abs() < 1e-6, "massa conservata");
        // la pompa svuota T2 verso lo scarico; a serbatoio vuoto gira a secco
        tags.borrow_mut().insert("V2", Value::Bool(false));
        tags.borrow_mut().insert("P1", Value::Bool(true));
        run(&mut f, 600.0, &tags);
        assert!(f.tanks[1].v < 1e-6, "T2 vuoto {}", f.tanks[1].v);
        assert!(f.pumps[0].dry, "marcia a secco");
        // scatto termico: la pompa si ferma
        f.inject("pump", "p/P1", &json!({"tripped": true})).unwrap().unwrap();
        run(&mut f, 3.0, &tags);
        assert_eq!(f.pumps[0].s, 0.0);
    }

    #[test]
    fn junction_shares_pump_between_two_branches() {
        let ok: Check = &|_k, _w, t, _e| t.trim().to_string();
        let comps = vec![
            raw("p", "S", json!({"kind": "tank", "volume_m3": 10.0, "height_m": 2.0, "init_level": 80})),
            raw("p", "A", json!({"kind": "tank", "volume_m3": 2.0, "height_m": 2.0, "elevation_m": 3.0})),
            raw("p", "B", json!({"kind": "tank", "volume_m3": 2.0, "height_m": 2.0, "elevation_m": 3.0})),
            raw("p", "J", json!({"kind": "junction"})),
            raw("p", "P", json!({"kind": "pump", "run": "P", "head_m": 30.0, "q_max": 12.0})),
            raw("p", "VA", json!({"kind": "valve", "cmd": "VA", "kv": 5.0, "time_s": 0})),
            raw("p", "VB", json!({"kind": "valve", "cmd": "VB", "kv": 5.0, "time_s": 0})),
            raw("p", "L0", json!({"kind": "pipe", "from": "S", "to": "J", "elements": "P"})),
            raw("p", "LA", json!({"kind": "pipe", "from": "J", "to": "A", "elements": "VA"})),
            raw("p", "LB", json!({"kind": "pipe", "from": "J", "to": "B", "elements": "VB"})),
        ];
        let mut errs = vec![];
        let mut f = Fluids::build(&comps, 1000.0, 4186.0, 20.0, ok, ok, &mut errs);
        assert!(errs.is_empty(), "{errs:?}");
        let tags = RefCell::new(HashMap::from([("P", Value::Bool(true)), ("VA", Value::Bool(true))]));
        run(&mut f, 5.0, &tags);
        let qa = f.pipes[1].q;
        assert!(qa > 1.0, "un ramo: {qa}");
        assert!((f.pipes[0].q - qa).abs() < 0.05, "bilancio alla giunzione {} {}", f.pipes[0].q, qa);
        tags.borrow_mut().insert("VB", Value::Bool(true));
        run(&mut f, 5.0, &tags);
        let (q0, qa2, qb) = (f.pipes[0].q, f.pipes[1].q, f.pipes[2].q);
        assert!((q0 - qa2 - qb).abs() < 0.05, "bilancio {q0} {qa2} {qb}");
        assert!(qa2 < qa && (qa2 - qb).abs() < 0.05, "rami uguali, ciascuno meno di prima: {qa2} {qb} (prima {qa})");
        assert!(q0 > qa, "la pompa dà di più con due rami aperti");
    }
}
