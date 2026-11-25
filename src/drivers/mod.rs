//! Driver di comunicazione PLC. Ogni driver gira in un task dedicato con:
//! - polling ciclico dei tag assegnati (letture a blocchi),
//! - coda di scrittura prioritaria (le scritture non aspettano il ciclo di poll),
//! - riconnessione automatica con backoff e qualità Bad sui tag in caso di errore.

pub mod modbus;
pub mod s7;
pub mod sim;
#[cfg(feature = "opcua")]
pub mod opcua;
#[cfg(feature = "eip")]
pub mod eip;

use crate::config::{DriverConfig, Protocol, TagDef};
use crate::tagdb::{Quality, TagDb, WriteCmd, now_ms};
use crate::value::Value;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[async_trait]
pub trait Driver: Send {
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) {}
    /// Legge un insieme di tag; il risultato è nello stesso ordine.
    async fn read(&mut self, tags: &[TagDef]) -> Result<Vec<Value>>;
    async fn write(&mut self, tag: &TagDef, value: &Value) -> Result<()>;
}

pub fn create(cfg: &DriverConfig, tags: &[TagDef]) -> Result<Box<dyn Driver>> {
    Ok(match cfg.protocol {
        Protocol::Sim => Box::new(sim::SimDriver::new(tags)),
        Protocol::Modbus => Box::new(modbus::ModbusDriver::new(cfg, tags)?),
        Protocol::S7 => Box::new(s7::S7Driver::new(cfg, tags)?),
        #[cfg(feature = "opcua")]
        Protocol::Opcua => Box::new(opcua::OpcUaDriver::new(cfg, tags)?),
        #[cfg(feature = "eip")]
        Protocol::Eip => Box::new(eip::EipDriver::new(cfg, tags)?),
        #[allow(unreachable_patterns)]
        _p => return Err(anyhow!("protocollo {p:?} non compilato: ricompila con --features {_p:?}".to_lowercase())),
    })
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DriverStatus {
    pub name: String,
    pub protocol: String,
    pub connected: bool,
    pub last_error: Option<String>,
    /// Durata dell'ultimo ciclo di lettura (ms).
    pub cycle_ms: f64,
    pub reads: u64,
    pub writes: u64,
    pub errors: u64,
    pub since: i64,
}

pub type StatusBoard = Arc<RwLock<HashMap<String, DriverStatus>>>;

/// Loop principale di un driver.
/// Periodo di lettura di ogni tag (ms): scan class del tag, altrimenti `poll_ms` del driver.
pub fn scan_rates(cfg: &DriverConfig, tags: &[TagDef], classes: &[crate::config::ScanClass]) -> Vec<u64> {
    tags.iter()
        .map(|t| {
            t.scan_class
                .as_ref()
                .and_then(|n| classes.iter().find(|c| &c.name == n))
                .map(|c| c.rate_ms)
                .unwrap_or(cfg.poll_ms)
                .max(10)
        })
        .collect()
}

pub async fn run(
    cfg: DriverConfig,
    tags: Vec<TagDef>,
    rates: Vec<u64>,
    db: Arc<TagDb>,
    mut rx: mpsc::Receiver<WriteCmd>,
    board: StatusBoard,
) {
    let name = cfg.name.clone();
    let set = |f: &dyn Fn(&mut DriverStatus)| {
        let mut b = board.write().unwrap();
        let s = b.entry(name.clone()).or_insert_with(|| DriverStatus {
            name: name.clone(),
            protocol: format!("{:?}", cfg.protocol).to_lowercase(),
            ..Default::default()
        });
        f(s);
    };
    set(&|_| {});

    let mut drv = match create(&cfg, &tags) {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(driver = %name, "creazione driver fallita: {e:#}");
            let msg = format!("{e:#}");
            set(&|s| s.last_error = Some(msg.clone()));
            db.set_driver_bad(&name);
            // Rifiuta tutte le scritture future.
            while let Some(cmd) = rx.recv().await {
                let _ = cmd.ack.send(Err(anyhow!("driver {name} non disponibile")));
            }
            return;
        }
    };

    let timeout = Duration::from_millis(cfg.timeout_ms);
    // Scan class: gruppi di tag per periodo di lettura.
    let mut groups: Vec<(u64, Vec<usize>)> = Vec::new();
    for (i, r) in rates.iter().enumerate() {
        match groups.iter_mut().find(|g| g.0 == *r) {
            Some(g) => g.1.push(i),
            None => groups.push((*r, vec![i])),
        }
    }
    let poll = Duration::from_millis(groups.iter().map(|g| g.0).min().unwrap_or(cfg.poll_ms).max(10));
    let mut backoff = Duration::from_millis(500);

    'outer: loop {
        // ---- connessione ----
        match tokio::time::timeout(timeout, drv.connect()).await {
            Ok(Ok(())) => {
                tracing::info!(driver = %name, "connesso");
                backoff = Duration::from_millis(500);
                set(&|s| {
                    s.connected = true;
                    s.last_error = None;
                    s.since = now_ms();
                });
            }
            res => {
                let msg = match res {
                    Ok(Err(e)) => format!("{e:#}"),
                    _ => "timeout connessione".into(),
                };
                tracing::warn!(driver = %name, "connessione fallita: {msg}");
                set(&|s| {
                    s.connected = false;
                    s.errors += 1;
                    s.last_error = Some(msg.clone());
                });
                db.set_driver_bad(&name);
                // Durante l'attesa rifiuta subito le scritture: il chiamante
                // (es. decision engine) deve saperlo senza bloccarsi.
                let deadline = tokio::time::sleep(backoff);
                tokio::pin!(deadline);
                loop {
                    tokio::select! {
                        _ = &mut deadline => break,
                        cmd = rx.recv() => match cmd {
                            Some(cmd) => { let _ = cmd.ack.send(Err(anyhow!("driver {name} disconnesso"))); }
                            None => return,
                        }
                    }
                }
                backoff = (backoff * 2).min(Duration::from_secs(10));
                continue;
            }
        }

        // ---- ciclo operativo ----
        let mut ticker = tokio::time::interval(poll);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut last_read: Vec<Option<Instant>> = vec![None; groups.len()];
        loop {
            tokio::select! {
                biased;
                cmd = rx.recv() => {
                    let Some(cmd) = cmd else { return };
                    let r = tokio::time::timeout(timeout, drv.write(&cmd.tag, &cmd.value)).await
                        .unwrap_or_else(|_| Err(anyhow!("timeout scrittura")));
                    let failed = r.is_err();
                    if let Err(e) = &r {
                        tracing::warn!(driver = %name, tag = %cmd.tag.name, "scrittura fallita: {e:#}");
                    }
                    let _ = cmd.ack.send(r);
                    set(&|s| if failed { s.errors += 1 } else { s.writes += 1 });
                    if failed { break; }
                }
                _ = ticker.tick() => {
                    if tags.is_empty() { continue; }
                    let t0 = Instant::now();
                    // gruppi in scadenza (tolleranza di mezzo periodo base)
                    let slack = poll / 2;
                    let due: Vec<usize> = (0..groups.len())
                        .filter(|&g| last_read[g].is_none_or(|t| t0 + slack >= t + Duration::from_millis(groups[g].0)))
                        .collect();
                    if due.is_empty() { continue; }
                    let idx: Vec<usize> = due.iter().flat_map(|&g| groups[g].1.iter().copied()).collect();
                    let batch: Vec<TagDef> = idx.iter().map(|&i| tags[i].clone()).collect();
                    match tokio::time::timeout(timeout, drv.read(&batch)).await {
                        Ok(Ok(values)) => {
                            for (t, v) in batch.iter().zip(values) {
                                db.update(&t.name, v, Quality::Good);
                            }
                            for &g in &due { last_read[g] = Some(t0); }
                            let ms = t0.elapsed().as_secs_f64() * 1000.0;
                            set(&|s| { s.reads += 1; s.cycle_ms = ms; });
                        }
                        res => {
                            let msg = match res { Ok(Err(e)) => format!("{e:#}"), _ => "timeout lettura".into() };
                            tracing::warn!(driver = %name, "lettura fallita: {msg}");
                            set(&|s| { s.errors += 1; s.last_error = Some(msg.clone()); });
                            break;
                        }
                    }
                }
            }
        }
        // errore: chiudi e riconnetti
        drv.disconnect().await;
        db.set_driver_bad(&name);
        set(&|s| s.connected = false);
        if rx.is_closed() {
            break 'outer;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Raggruppa intervalli [start, start+len) vicini in blocchi di lettura unici.
/// Ritorna per ogni blocco (start, len) e per ogni elemento l'indice del blocco.
pub fn group_ranges(items: &[(u32, u32)], max_gap: u32, max_len: u32) -> (Vec<(u32, u32)>, Vec<usize>) {
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| items[i].0);
    let mut blocks: Vec<(u32, u32)> = Vec::new();
    let mut assign = vec![0usize; items.len()];
    for i in order {
        let (s, l) = items[i];
        let e = s + l.max(1);
        if let Some(b) = blocks.last_mut() {
            let be = b.0 + b.1;
            if s <= be + max_gap && e.max(be) - b.0 <= max_len {
                b.1 = e.max(be) - b.0;
                assign[i] = blocks.len() - 1;
                continue;
            }
        }
        blocks.push((s, e - s));
        assign[i] = blocks.len() - 1;
    }
    (blocks, assign)
}

#[cfg(test)]
mod tests {
    #[test]
    fn grouping() {
        let (b, a) = super::group_ranges(&[(10, 2), (0, 1), (12, 2), (200, 4)], 8, 100);
        assert_eq!(b, vec![(0, 1), (10, 4), (200, 4)]);
        assert_eq!(a, vec![1, 0, 1, 2]);
        let (b, _) = super::group_ranges(&[(0, 1), (5, 1)], 8, 100);
        assert_eq!(b, vec![(0, 6)]);
    }
}
