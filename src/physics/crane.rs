//! Layer fisico – trasloelevatore con assi.
//!
//! Il componente "crane" è una corsia vista in elevazione: il riquadro del componente
//! rappresenta la corsia (X da sinistra a destra, Y dal basso in alto). Tre assi:
//! - **X** traslazione, **Y** sollevamento, **Z** forche (negativo = lato 1, positivo = lato 2);
//! - ogni asse è un azionamento in posizionamento (come un EPOS): segue la quota obiettivo
//!   scritta dal PLC con un profilo trapezoidale (velocità massima, accelerazione), entro i
//!   finecorsa; restituisce posizione reale, "in posizione" e "in movimento".
//!
//! Il carico si prende e si deposita con il gesto vero: forche fuori in una cella,
//! **sollevamento** (prelievo) o **abbassamento** (deposito) di almeno la corsa di presa,
//! forche dentro. La cella è quella davanti alle forche (colonna e livello dalla posizione).
//! Interblocchi controllati dalla fisica (errori tipici di un programma PLC):
//! movimento X/Y con le forche fuori, deposito in una cella occupata, forche fuori
//! con il carro fuori da una cella.

use super::full_key;
use super::RawComp;
use crate::value::Value;
use serde_json::{Value as J, json};
use std::collections::BTreeMap;

pub(super) type Check<'a> = &'a dyn Fn(&str, &str, &str, &mut Vec<String>) -> String;

/// Asse in posizionamento (unità: mm, mm/s).
#[derive(Debug, Clone)]
pub(super) struct Axis {
    name: &'static str,
    target: String,
    pos_tag: String,
    inpos_tag: String,
    vmax: f64,
    acc: f64,
    min: f64,
    max: f64,
    pub(super) pos: f64,
    v: f64,
    fault: bool,
}

impl Axis {
    fn step(&mut self, target: Option<f64>, enabled: bool, dt: f64) {
        let brake = |v: f64, a: f64| if v > 0.0 { (v - a * dt).max(0.0) } else { (v + a * dt).min(0.0) };
        match target {
            Some(t) if enabled && !self.fault => {
                let t = t.clamp(self.min, self.max);
                let d = t - self.pos;
                if d.abs() < 0.5 && self.v.abs() <= self.acc * dt * 2.0 {
                    self.pos = t;
                    self.v = 0.0;
                    return;
                }
                let dir = d.signum();
                let stop = self.v * self.v / (2.0 * self.acc);
                self.v = if self.v * dir < 0.0 || stop >= d.abs() {
                    brake(self.v, self.acc)
                } else {
                    (self.v + dir * self.acc * dt).clamp(-self.vmax, self.vmax)
                };
                let np = self.pos + self.v * dt;
                // non oltrepassare l'obiettivo per arrotondamento del passo
                if (t - np).signum() != dir && self.v.abs() < self.acc * dt * 4.0 + 1.0 {
                    self.pos = t;
                    self.v = 0.0;
                } else {
                    self.pos = np;
                }
            }
            _ => {
                self.v = brake(self.v, self.acc * 2.0);
                self.pos += self.v * dt;
            }
        }
        if self.pos < self.min || self.pos > self.max {
            // finecorsa: fermata immediata
            self.pos = self.pos.clamp(self.min, self.max);
            self.v = 0.0;
        }
    }
    fn in_pos(&self, target: Option<f64>) -> bool {
        target.is_some_and(|t| (t.clamp(self.min, self.max) - self.pos).abs() < 2.0) && self.v == 0.0
    }
}

#[derive(Debug, Clone)]
pub(super) struct Crane {
    pub(super) key: String,
    view: String,
    id: String,
    /// Riquadro del componente nella vista (px): x, y, w, h.
    rect: (f64, f64, f64, f64),
    pub(super) ax: [Axis; 3],
    enable: String,
    load_tag: String,
    fault_tag: String,
    moving_tag: String,
    reset_tag: String,
    last_reset: bool,
    reach: f64,
    lift: f64,
    x0: f64,
    y0: f64,
    pitch_x: f64,
    pitch_y: f64,
    cols: usize,
    levels: usize,
    track: bool,
    /// Occupazione delle celle (se tracciata): [lato 0/1][colonna][livello].
    cells: Vec<bool>,
    pub(super) pd_in: Option<usize>,
    pub(super) pd_out: Option<usize>,
    pub(super) loaded: bool,
    /// Carichi sul carro e capacità (1, o 2 per i trasli a doppia UdC).
    count: u8,
    capacity: u8,
    pub(super) load_code: String,
    /// Dimensioni e colore del carico preso da un nastro (per rimetterlo su un nastro).
    pub(super) load_dims: (f64, f64, String, String, f64),
    last_pos: [f64; 3],
    y_ext: Option<f64>,
    pub(super) fault: Option<String>,
    pick_fail: bool,
    picks: u64,
    drops: u64,
}

/// Trasferimento completato in questo passo (gestito dal motore per i P&D sui nastri).
pub(super) struct Transfer {
    pub(super) crane: usize,
    pub(super) pick: bool,
    pub(super) col: usize,
    pub(super) side: usize,
}

impl Crane {
    pub(super) fn build(raw: &[RawComp], actuator: Check, sensor: Check, conv_of: &dyn Fn(&str, &str) -> Option<usize>, errs: &mut Vec<String>) -> Vec<Crane> {
        let mut out = vec![];
        for r in raw.iter().filter(|r| r.def.kind == "crane") {
            let k = format!("{}/{}", r.view, r.id);
            let d = &r.def;
            let n0 = errs.len();
            let cols = d.cols.unwrap_or(20).max(1) as usize;
            let levels = d.levels.unwrap_or(5).max(1) as usize;
            let pitch_x = d.pitch_x_mm.unwrap_or(1200.0).max(1.0);
            let pitch_y = d.pitch_y_mm.unwrap_or(1500.0).max(1.0);
            let x0 = d.x0_mm.unwrap_or(0.0);
            let y0 = d.y0_mm.unwrap_or(100.0);
            let reach = d.z_reach_mm.unwrap_or(1300.0).max(10.0);
            let xmax = d.x_max_mm.unwrap_or(x0 + pitch_x * cols as f64 + 500.0);
            let ymax = d.y_max_mm.unwrap_or(y0 + pitch_y * (levels as f64 - 1.0) + 500.0);
            let axis = |name: &'static str, tgt: &str, pos: &str, inpos: &str, vmax: f64, acc: f64, min: f64, max: f64, errs: &mut Vec<String>| Axis {
                name,
                target: actuator(&k, &format!("quota obiettivo {name}"), tgt, errs),
                pos_tag: sensor(&k, &format!("posizione {name}"), pos, errs),
                inpos_tag: sensor(&k, &format!("in posizione {name}"), inpos, errs),
                vmax: vmax * 1000.0,
                acc: acc * 1000.0,
                min,
                max,
                pos: 0.0,
                v: 0.0,
                fault: false,
            };
            let ax = [
                axis("X", &d.x_target, &d.x_pos, &d.x_inpos, d.x_vmax.unwrap_or(3.0), d.x_acc.unwrap_or(0.5), 0.0, xmax, errs),
                axis("Y", &d.y_target, &d.y_pos, &d.y_inpos, d.y_vmax.unwrap_or(1.0), d.y_acc.unwrap_or(0.8), 0.0, ymax, errs),
                axis("Z", &d.z_target, &d.z_pos, &d.z_inpos, d.z_vmax.unwrap_or(0.6), d.z_acc.unwrap_or(1.0), -reach, reach, errs),
            ];
            for a in &ax {
                if a.target.is_empty() {
                    errs.push(format!("{k}: manca il tag della quota obiettivo dell'asse {}", a.name));
                }
            }
            let conv = |id: &str, what: &str, errs: &mut Vec<String>| -> Option<usize> {
                if id.trim().is_empty() {
                    return None;
                }
                let c = conv_of(&r.view, id.trim());
                if c.is_none() {
                    errs.push(format!("{k}: {what} '{}' non è un nastro", id.trim()));
                }
                c
            };
            let pd_in = conv(&d.pd_in, "P&D di ingresso", errs);
            let pd_out = conv(&d.pd_out, "P&D di uscita", errs);
            let mut crane = Crane {
                key: k.clone(),
                view: r.view.clone(),
                id: r.id.clone(),
                rect: (r.geo.cx - r.geo.w / 2.0, r.geo.cy - r.geo.h / 2.0, r.geo.w, r.geo.h),
                ax,
                enable: actuator(&k, "abilitazione", &d.enable, errs),
                load_tag: sensor(&k, "carico presente", &d.load_tag, errs),
                fault_tag: sensor(&k, "guasto", &d.fault_tag, errs),
                moving_tag: sensor(&k, "in movimento", &d.moving_tag, errs),
                reset_tag: actuator(&k, "reset guasti", &d.reset_tag, errs),
                last_reset: false,
                reach,
                lift: d.lift_mm.unwrap_or(100.0).max(10.0),
                x0,
                y0,
                pitch_x,
                pitch_y,
                cols,
                levels,
                track: d.track,
                cells: vec![false; 2 * (cols + 1) * levels],
                pd_in,
                pd_out,
                loaded: false,
                count: 0,
                capacity: d.capacity.unwrap_or(1).clamp(1, 4) as u8,
                load_code: String::new(),
                load_dims: (1.2, 0.8, "#a16207".into(), "Pallet".into(), 500.0),
                last_pos: [f64::NAN; 3],
                y_ext: None,
                fault: None,
                pick_fail: false,
                picks: 0,
                drops: 0,
            };
            // stato iniziale: carro al P&D, celle riempite al livello indicato (se tracciate)
            crane.ax[1].pos = y0;
            if crane.track {
                let fill = d.init_fill_pct.unwrap_or(0.0).clamp(0.0, 100.0) / 100.0;
                let mut seed: u64 = 0x9e37_79b9_7f4a_7c15 ^ k.len() as u64;
                for i in 0..crane.cells.len() {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    let col = (i / levels) % (cols + 1);
                    crane.cells[i] = col > 0 && ((seed % 10_000) as f64) < fill * 10_000.0;
                }
            }
            if errs.len() == n0 {
                out.push(crane);
            }
        }
        out
    }

    fn cell_idx(&self, side: usize, col: usize, lvl: usize) -> usize {
        (side * (self.cols + 1) + col) * self.levels + (lvl - 1)
    }

    /// Cella davanti alle forche: (lato 0/1, colonna 0..=cols, livello 1..=levels) se il carro è allineato.
    fn cell_here(&self, y: f64) -> Option<(usize, usize)> {
        let cf = (self.ax[0].pos - self.x0) / self.pitch_x;
        let lf = (y - self.y0) / self.pitch_y;
        let (col, lvl) = (cf.round(), lf.round());
        // tolleranza: ±40 mm in X, entro la corsa di presa in Y
        if (cf - col).abs() * self.pitch_x > 40.0 || (lf - lvl).abs() * self.pitch_y > self.lift + 40.0 {
            return None;
        }
        if col < 0.0 || col as usize > self.cols || lvl < 0.0 || lvl as usize >= self.levels {
            return None;
        }
        Some((col as usize, lvl as usize + 1))
    }

    fn set_fault(&mut self, why: String) {
        if self.fault.is_none() {
            self.fault = Some(why);
        }
        for a in &mut self.ax {
            a.v = 0.0;
        }
    }

    /// Un passo: assi, interblocchi, prelievi e depositi. Ritorna i trasferimenti avvenuti ai P&D.
    pub(super) fn step(&mut self, idx: usize, dt: f64, read: &dyn Fn(&str) -> Value, out: &mut Vec<(String, Value)>) -> Option<Transfer> {
        let b = |t: &str| !t.is_empty() && read(t).as_bool().unwrap_or(false);
        let reset = b(&self.reset_tag);
        if reset && !self.last_reset {
            self.fault = None;
            for a in &mut self.ax {
                a.fault = false;
            }
        }
        self.last_reset = reset;
        let enabled = (self.enable.is_empty() || b(&self.enable)) && self.fault.is_none();
        let targets: Vec<Option<f64>> = self.ax.iter().map(|a| read(&a.target).as_f64()).collect();
        let (x_prev, y_prev) = (self.ax[0].pos, self.ax[1].pos);
        for (a, t) in self.ax.iter_mut().zip(&targets) {
            a.step(*t, enabled, dt);
        }
        let z = self.ax[2].pos;
        let mut transfer = None;
        // interblocco: traslazione o sollevamento oltre la corsa di presa con le forche fuori
        if z.abs() > 50.0 {
            if (self.ax[0].pos - x_prev).abs() > 0.01 {
                self.set_fault("traslazione con le forche fuori".into());
            }
            let y_ext = *self.y_ext.get_or_insert(y_prev);
            if (self.ax[1].pos - y_ext).abs() > self.lift + 60.0 {
                self.set_fault("sollevamento con le forche fuori oltre la corsa di presa".into());
            }
            if self.fault.is_none() && self.cell_here(y_ext).is_none() {
                self.set_fault("forche fuori con il carro non allineato a una cella".into());
            }
        }
        // prelievo / deposito: forche tutte fuori, poi su (prelievo) o giù (deposito) di almeno la corsa
        if self.fault.is_none() && z.abs() >= self.reach * 0.97 {
            let y_ext = *self.y_ext.get_or_insert(self.ax[1].pos);
            let dy = self.ax[1].pos - y_ext;
            let side = if z < 0.0 { 0 } else { 1 };
            if let Some((col, lvl)) = self.cell_here(y_ext) {
                if self.count < self.capacity && dy >= self.lift * 0.8 {
                    let pd = col == 0 && ((side == 0 && self.pd_in.is_some()) || (side == 1 && self.pd_out.is_some()));
                    if pd {
                        transfer = Some(Transfer { crane: idx, pick: true, col, side });
                    } else {
                        let full = !self.track || self.cells[self.cell_idx(side, col, lvl)];
                        if full && !self.pick_fail {
                            self.count += 1;
                            self.loaded = true;
                            self.load_code = format!("C{col:02}L{lvl:02}{}", if side == 0 { "S" } else { "D" });
                            if self.track {
                                let i = self.cell_idx(side, col, lvl);
                                self.cells[i] = false;
                            }
                            self.picks += 1;
                        }
                        self.pick_fail = false;
                    }
                    self.y_ext = Some(self.ax[1].pos); // un solo prelievo per estensione
                } else if self.count > 0 && dy <= -self.lift * 0.8 {
                    let pd = col == 0 && ((side == 0 && self.pd_in.is_some()) || (side == 1 && self.pd_out.is_some()));
                    if pd {
                        transfer = Some(Transfer { crane: idx, pick: false, col, side });
                    } else if self.track && self.cells[self.cell_idx(side, col, lvl)] {
                        self.set_fault(format!("doppio deposito: cella lato {} col {col} liv {lvl} occupata", side + 1));
                    } else {
                        if self.track {
                            let i = self.cell_idx(side, col, lvl);
                            self.cells[i] = true;
                        }
                        self.count -= 1;
                        self.loaded = self.count > 0;
                        if !self.loaded {
                            self.load_code.clear();
                        }
                        self.drops += 1;
                    }
                    self.y_ext = Some(self.ax[1].pos);
                }
            }
        }
        if z.abs() <= 50.0 {
            self.y_ext = None;
        }
        // sensori
        let mut aw = |tag: &str, v: Value| {
            if !tag.is_empty() {
                out.push((tag.to_string(), v));
            }
        };
        for (k, (a, t)) in self.ax.iter().zip(&targets).enumerate() {
            // posizione: banda morta di 5 mm in movimento, valore esatto da fermo
            if a.v == 0.0 || (a.pos - self.last_pos[k]).abs() >= 5.0 || self.last_pos[k].is_nan() {
                self.last_pos[k] = a.pos;
                aw(&a.pos_tag, Value::Float(a.pos.round()));
            }
            aw(&a.inpos_tag, Value::Bool(a.in_pos(*t)));
        }
        aw(&self.load_tag, Value::Bool(self.loaded));
        aw(&self.fault_tag, Value::Bool(self.fault.is_some()));
        aw(&self.moving_tag, Value::Bool(self.ax.iter().any(|a| a.v != 0.0)));
        transfer
    }

    /// Esito di un trasferimento al P&D deciso dal motore (unità presa o depositata sul nastro).
    pub(super) fn complete(&mut self, pick: bool, ok: bool, code: String, why: &str) {
        if !ok {
            if !pick {
                self.set_fault(why.into());
            }
            return;
        }
        if pick {
            self.count += 1;
            self.loaded = true;
            self.load_code = code;
            self.picks += 1;
        } else {
            self.count = self.count.saturating_sub(1);
            self.loaded = self.count > 0;
            if !self.loaded {
                self.load_code.clear();
            }
            self.drops += 1;
        }
    }

    pub(super) fn frame(&self, views: &mut BTreeMap<String, Vec<J>>) {
        let (rx, ry, rw, rh) = self.rect;
        let xmax = self.ax[0].max.max(1.0);
        let ymax = self.ax[1].max.max(1.0);
        let px = |x: f64| rx + x / xmax * rw;
        let py = |y: f64| ry + rh - y / ymax * rh;
        let occ: Vec<J> = if self.track {
            (0..self.cells.len()).filter(|i| self.cells[*i]).map(|i| json!(i)).collect()
        } else {
            vec![]
        };
        views.entry(self.view.clone()).or_default().push(json!({
            "id": self.id, "x": (px(self.ax[0].pos) * 10.0).round() / 10.0, "y": (py(self.ax[1].pos) * 10.0).round() / 10.0,
            "z": (self.ax[2].pos / self.reach * 100.0).round() / 100.0, "loaded": self.loaded, "count": self.count, "fault": self.fault.is_some(),
            "rect": [rx, ry, rw, rh], "cw": self.pitch_x / xmax * rw, "ch": self.pitch_y / ymax * rh,
            "x0": px(self.x0), "y0": py(self.y0), "cols": self.cols, "levels": self.levels, "track": self.track, "occ": occ,
        }));
    }

    pub(super) fn status(&self, comps: &mut Vec<J>) {
        let ax: Vec<J> = self.ax.iter().map(|a| json!({"name": a.name, "pos": a.pos.round(), "v": (a.v).round(), "fault": a.fault, "min": a.min, "max": a.max})).collect();
        let cell = self.cell_here(self.ax[1].pos);
        comps.push(json!({"key": self.key, "kind": "crane", "axes": ax, "loaded": self.loaded, "count": self.count, "capacity": self.capacity, "load": self.load_code, "fault": self.fault,
            "cell": cell.map(|(c, l)| json!({"col": c, "lvl": l})), "picks": self.picks, "drops": self.drops, "track": self.track,
            "occupied": self.cells.iter().filter(|c| **c).count(), "cells": 2 * self.cols * self.levels}));
    }

    pub(super) fn inject(&mut self, req: &J) -> anyhow::Result<String> {
        let flag = |k: &str| req.get(k).and_then(J::as_bool).unwrap_or(false);
        let mut msg = vec![];
        if let Some(a) = req.get("axis").and_then(J::as_str) {
            let ax = self.ax.iter_mut().find(|x| x.name.eq_ignore_ascii_case(a)).ok_or_else(|| anyhow::anyhow!("asse '{a}' sconosciuto (X, Y, Z)"))?;
            ax.fault = flag("fault");
            ax.v = 0.0;
            msg.push(format!("asse {}: {}", ax.name, if ax.fault { "guasto azionamento" } else { "ripristinato" }));
        }
        if req.get("pick_fail").is_some() {
            self.pick_fail = flag("pick_fail");
            msg.push(if self.pick_fail { "il prossimo prelievo trova la cella vuota".into() } else { "prelievo normale".into() });
        }
        if flag("reset") {
            self.fault = None;
            for a in &mut self.ax {
                a.fault = false;
            }
            msg.push("guasti azzerati".into());
        }
        if flag("unload") {
            self.count = 0;
            self.loaded = false;
            self.load_code.clear();
            msg.push("carico rimosso dal carro".into());
        }
        anyhow::ensure!(!msg.is_empty(), "nessun intervento indicato");
        Ok(format!("{}: {}", self.key, msg.join(", ")))
    }

    pub(super) fn carry_from(&mut self, old: &Crane) {
        for (a, o) in self.ax.iter_mut().zip(&old.ax) {
            (a.pos, a.fault) = (o.pos.clamp(a.min, a.max), o.fault);
        }
        (self.loaded, self.load_code, self.fault, self.picks, self.drops) = (old.loaded, old.load_code.clone(), old.fault.clone(), old.picks, old.drops);
        self.count = old.count.min(self.capacity);
        if self.cells.len() == old.cells.len() {
            self.cells = old.cells.clone();
        }
    }

    pub(super) fn reset(&mut self) {
        for a in &mut self.ax {
            (a.pos, a.v, a.fault) = (0.0, 0.0, false);
        }
        self.ax[1].pos = self.y0;
        (self.loaded, self.fault, self.y_ext, self.count) = (false, None, None, 0);
        self.load_code.clear();
    }

    pub(super) fn key_matches(&self, key: &str) -> bool {
        self.key == key || full_key(&self.view, key) == self.key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::PhysDef;
    use std::cell::RefCell;
    use std::collections::HashMap;

    fn crane(track: bool) -> Crane {
        let ok: Check = &|_k, _w, t, _e| t.trim().to_string();
        let def: PhysDef = serde_json::from_value(json!({
            "kind": "crane", "cols": 10, "levels": 4, "pitch_x_mm": 1000, "pitch_y_mm": 1500, "y0_mm": 100,
            "x_target": "X.T", "y_target": "Y.T", "z_target": "Z.T", "x_pos": "X.P", "x_inpos": "X.I", "z_inpos": "Z.I",
            "load_tag": "LOAD", "fault_tag": "FAULT", "track": track, "init_fill_pct": 0,
            "x_vmax": 2.0, "x_acc": 0.5, "y_vmax": 1.0, "y_acc": 1.0, "z_vmax": 0.8, "z_acc": 2.0, "z_reach_mm": 1200, "lift_mm": 100
        })).unwrap();
        let raw = vec![RawComp { view: "v".into(), id: "C".into(), geo: Default::default(), def }];
        let mut errs = vec![];
        let v = Crane::build(&raw, ok, ok, &|_, _| None, &mut errs);
        assert!(errs.is_empty(), "{errs:?}");
        v.into_iter().next().unwrap()
    }

    fn go(c: &mut Crane, tags: &RefCell<HashMap<&str, Value>>, x: f64, y: f64, z: f64, max_s: f64) -> f64 {
        {
            let mut t = tags.borrow_mut();
            t.insert("X.T", Value::Float(x));
            t.insert("Y.T", Value::Float(y));
            t.insert("Z.T", Value::Float(z));
        }
        let mut s = 0.0;
        let mut out = vec![];
        while s < max_s {
            let tg = tags.borrow();
            c.step(0, 0.01, &|n| tg.get(n).cloned().unwrap_or(Value::Null), &mut out);
            s += 0.01;
            if c.ax.iter().all(|a| a.v == 0.0) && (c.ax[0].pos - x).abs() < 0.6 && (c.ax[1].pos - y).abs() < 0.6 && (c.ax[2].pos - z).abs() < 0.6 {
                break;
            }
        }
        s
    }

    #[test]
    fn trapezoidal_move_and_limits() {
        let mut c = crane(false);
        let tags = RefCell::new(HashMap::new());
        // 8 m con vmax 2 m/s e acc 0,5 m/s²: 4 s di rampa + 4 m a regime (2 s) + 4 s di frenata = 8 s… circa 8 s
        let t = go(&mut c, &tags, 8000.0, 100.0, 0.0, 20.0);
        assert!((t - 8.0).abs() < 0.2, "tempo di traslazione {t}");
        assert!((c.ax[0].pos - 8000.0).abs() < 0.6);
        // obiettivo oltre il finecorsa: si ferma al limite
        go(&mut c, &tags, 99_999.0, 100.0, 0.0, 30.0);
        assert!((c.ax[0].pos - c.ax[0].max).abs() < 0.6);
    }

    #[test]
    fn pick_drop_with_lift_and_interlocks() {
        let mut c = crane(true);
        let i = c.cell_idx(0, 3, 2);
        c.cells[i] = true; // cella lato 1, colonna 3, livello 2 occupata
        let tags = RefCell::new(HashMap::new());
        let (x, y) = (3000.0, 100.0 + 1500.0);
        // prelievo: in basso, forche fuori (lato 1 = Z negativo), su, forche dentro
        go(&mut c, &tags, x, y - 50.0, 0.0, 30.0);
        go(&mut c, &tags, x, y - 50.0, -1200.0, 10.0);
        go(&mut c, &tags, x, y + 50.0, -1200.0, 10.0);
        go(&mut c, &tags, x, y + 50.0, 0.0, 10.0);
        assert!(c.loaded && c.fault.is_none(), "preso: {:?}", c.fault);
        assert!(!c.cells[i], "cella svuotata");
        // deposito nella cella lato 2, colonna 5, livello 1: in alto, forche fuori, giù, dentro
        let (x2, y2) = (5000.0, 100.0);
        go(&mut c, &tags, x2, y2 + 50.0, 0.0, 30.0);
        go(&mut c, &tags, x2, y2 + 50.0, 1200.0, 10.0);
        go(&mut c, &tags, x2, y2 - 50.0, 1200.0, 10.0);
        go(&mut c, &tags, x2, y2 - 50.0, 0.0, 10.0);
        assert!(!c.loaded && c.cells[c.cell_idx(1, 5, 1)], "depositato");
        // prelievo da cella vuota: nessun carico (il PLC deve accorgersene dal sensore)
        go(&mut c, &tags, 7000.0, 100.0 - 50.0, 0.0, 30.0);
        go(&mut c, &tags, 7000.0, 50.0, -1200.0, 10.0);
        go(&mut c, &tags, 7000.0, 150.0, -1200.0, 10.0);
        go(&mut c, &tags, 7000.0, 150.0, 0.0, 10.0);
        assert!(!c.loaded, "cella vuota: niente carico");
        // interblocco: traslazione con le forche fuori → guasto, assi fermi
        go(&mut c, &tags, 7000.0, 150.0, -1200.0, 10.0);
        go(&mut c, &tags, 9000.0, 150.0, -1200.0, 2.0);
        assert!(c.fault.as_deref().is_some_and(|f| f.contains("forche fuori")), "{:?}", c.fault);
        let x_fault = c.ax[0].pos;
        go(&mut c, &tags, 9000.0, 150.0, -1200.0, 2.0);
        assert!((c.ax[0].pos - x_fault).abs() < 1.0, "fermo in guasto");
        // reset e doppio deposito
        c.inject(&json!({"reset": true})).unwrap();
        go(&mut c, &tags, x_fault, 150.0, 0.0, 10.0); // prima le forche dentro, poi la traslazione
        go(&mut c, &tags, 7000.0, 150.0, 0.0, 10.0);
        assert!(c.fault.is_none(), "{:?}", c.fault);
        c.loaded = true;
        c.count = 1;
        let j = c.cell_idx(1, 7, 1);
        c.cells[j] = true;
        go(&mut c, &tags, 7000.0, 150.0, 1200.0, 10.0);
        go(&mut c, &tags, 7000.0, 50.0, 1200.0, 10.0);
        assert!(c.fault.as_deref().is_some_and(|f| f.contains("doppio deposito")), "{:?}", c.fault);
    }
}
