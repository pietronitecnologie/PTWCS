//! Classi di smistamento e integrazione WMS.
//!
//! - Una classe raggruppa barcode (elenco esplicito e/o pattern con `*` e `?`)
//!   e ha una destinazione. Le classi `source: local` sono definite nel WCS;
//!   quelle `source: wms` ricevono l'elenco dei barcode dal WMS (sync periodica).
//! - Il WMS può anche essere interrogato collo per collo ("dove va questo
//!   barcode?") con timeout breve e cache: la risposta è usata dalle regole
//!   tramite l'uscita `wms:<campo>`.

use crate::config::{ClassSource, SortClass, WmsConfig};
use crate::tagdb::now_ms;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value as J;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

/// Corrispondenza con `*` (qualsiasi sequenza) e `?` (un carattere).
pub fn glob(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0usize, 0usize, None::<usize>, 0usize);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[derive(Debug, Clone)]
struct Class {
    def: SortClass,
    set: HashSet<String>,
}

impl Class {
    fn matches(&self, bc: &str) -> bool {
        self.set.contains(bc) || self.def.patterns.iter().any(|p| glob(p, bc))
    }
}

/// Risposta del WMS per un collo.
#[derive(Debug, Clone, Serialize)]
pub struct WmsAnswer {
    pub destination: Option<String>,
    pub class: Option<String>,
    pub ts: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct WmsStatus {
    pub enabled: bool,
    pub base_url: String,
    pub last_sync: Option<i64>,
    pub last_error: Option<String>,
    pub requests: u64,
    pub errors: u64,
    pub cache_size: usize,
    pub synced_classes: Vec<String>,
}

/// Classe ricevuta dal WMS nella sincronizzazione.
#[derive(Debug, Deserialize)]
struct WmsClass {
    name: String,
    #[serde(default)]
    destination: Option<J>,
    #[serde(default)]
    barcodes: Vec<String>,
    #[serde(default)]
    patterns: Vec<String>,
}

pub struct Sorting {
    classes: RwLock<Vec<Class>>,
    wms: RwLock<Option<WmsConfig>>,
    http: reqwest::Client,
    cache: RwLock<HashMap<String, WmsAnswer>>,
    status: RwLock<WmsStatus>,
}

fn json_str(v: &J) -> Option<String> {
    match v {
        J::Null => None,
        J::String(s) if s.is_empty() => None,
        J::String(s) => Some(s.clone()),
        v => Some(v.to_string()),
    }
}

/// Legge un campo con percorso a punti (`data.destination`).
fn field<'a>(v: &'a J, path: &str) -> Option<&'a J> {
    path.split('.').try_fold(v, |acc, k| acc.get(k))
}

impl Sorting {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            classes: Default::default(),
            wms: Default::default(),
            http: reqwest::Client::builder().build().expect("client HTTP"),
            cache: Default::default(),
            status: Default::default(),
        })
    }

    /// Applica la configurazione (all'avvio e a ogni salvataggio del progetto).
    /// L'appartenenza delle classi WMS già sincronizzata viene conservata.
    pub fn configure(&self, defs: &[SortClass], wms: Option<WmsConfig>) {
        let old: HashMap<String, HashSet<String>> = self
            .classes
            .read()
            .unwrap()
            .iter()
            .filter(|c| c.def.source == ClassSource::Wms)
            .map(|c| (c.def.name.clone(), c.set.clone()))
            .collect();
        let classes = defs
            .iter()
            .map(|d| {
                let mut set: HashSet<String> = d.barcodes.iter().cloned().collect();
                if d.source == ClassSource::Wms {
                    set.extend(old.get(&d.name).cloned().unwrap_or_default());
                }
                Class { def: d.clone(), set }
            })
            .collect();
        *self.classes.write().unwrap() = classes;
        let changed = {
            let cur = self.wms.read().unwrap();
            serde_json::to_string(&*cur).ok() != serde_json::to_string(&wms).ok()
        };
        if changed {
            self.cache.write().unwrap().clear();
        }
        let mut st = self.status.write().unwrap();
        st.enabled = wms.as_ref().is_some_and(|w| w.enabled);
        st.base_url = wms.as_ref().map(|w| w.base_url.clone()).unwrap_or_default();
        *self.wms.write().unwrap() = wms;
    }

    pub fn status(&self) -> WmsStatus {
        let mut s = self.status.read().unwrap().clone();
        s.cache_size = self.cache.read().unwrap().len();
        s
    }

    /// Prima classe che contiene il barcode: (nome, destinazione).
    pub fn classify(&self, barcode: &str) -> Option<(String, String)> {
        let bc = barcode.trim();
        if bc.is_empty() {
            return None;
        }
        self.classes
            .read()
            .unwrap()
            .iter()
            .find(|c| c.matches(bc))
            .map(|c| (c.def.name.clone(), c.def.destination.clone()))
    }

    pub fn in_class(&self, barcode: &str, class: &str) -> bool {
        let bc = barcode.trim();
        self.classes.read().unwrap().iter().any(|c| c.def.name == class && c.matches(bc))
    }

    /// Classi di appartenenza (tutte) di un barcode.
    pub fn classes_of(&self, barcode: &str) -> Vec<String> {
        let bc = barcode.trim();
        self.classes.read().unwrap().iter().filter(|c| c.matches(bc)).map(|c| c.def.name.clone()).collect()
    }

    pub fn wms_enabled(&self) -> bool {
        self.wms.read().unwrap().as_ref().is_some_and(|w| w.enabled)
    }

    /// Risposta WMS in cache (usata in modo sincrono dalle regole).
    pub fn wms_cached(&self, barcode: &str) -> Option<WmsAnswer> {
        let ttl = self.wms.read().unwrap().as_ref().map(|w| w.cache_ttl_s).unwrap_or(0) as i64 * 1000;
        self.cache.read().unwrap().get(barcode.trim()).filter(|a| now_ms() - a.ts <= ttl).cloned()
    }

    fn headers(w: &WmsConfig) -> reqwest::header::HeaderMap {
        let mut h = reqwest::header::HeaderMap::new();
        for (k, v) in &w.headers {
            // ${VAR} -> variabile d'ambiente (token fuori dal file di progetto)
            let val = if let Some(var) = v.strip_prefix("${").and_then(|x| x.strip_suffix('}')) {
                std::env::var(var).unwrap_or_default()
            } else {
                v.clone()
            };
            if let (Ok(k), Ok(v)) = (
                reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                reqwest::header::HeaderValue::from_str(&val),
            ) {
                h.insert(k, v);
            }
        }
        h
    }

    /// Chiede al WMS la destinazione di un collo (con cache). Un barcode
    /// sconosciuto al WMS (404) è memorizzato come risposta vuota.
    pub async fn wms_lookup(&self, barcode: &str) -> Result<WmsAnswer> {
        let bc = barcode.trim().to_string();
        if let Some(a) = self.wms_cached(&bc) {
            return Ok(a);
        }
        let w = self.wms.read().unwrap().clone().filter(|w| w.enabled).ok_or_else(|| anyhow!("WMS non configurato"))?;
        let url = format!(
            "{}{}",
            w.base_url.trim_end_matches('/'),
            w.destination_path.replace("{barcode}", &urlencode(&bc))
        );
        self.status.write().unwrap().requests += 1;
        let res = async {
            let r = self
                .http
                .get(&url)
                .headers(Self::headers(&w))
                .timeout(Duration::from_millis(w.timeout_ms))
                .send()
                .await?;
            if r.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(WmsAnswer { destination: None, class: None, ts: now_ms() });
            }
            let r = r.error_for_status()?;
            let j: J = r.json().await?;
            Ok::<_, anyhow::Error>(WmsAnswer {
                destination: field(&j, &w.destination_field).and_then(json_str),
                class: field(&j, &w.class_field).and_then(json_str),
                ts: now_ms(),
            })
        }
        .await;
        match res {
            Ok(a) => {
                self.cache.write().unwrap().insert(bc, a.clone());
                self.status.write().unwrap().last_error = None;
                Ok(a)
            }
            Err(e) => {
                let mut st = self.status.write().unwrap();
                st.errors += 1;
                st.last_error = Some(format!("{e:#}"));
                Err(e).with_context(|| format!("richiesta WMS {url}"))
            }
        }
    }

    /// Sincronizza l'appartenenza delle classi `source: wms`.
    pub async fn sync_classes(&self) -> Result<usize> {
        let Some(w) = self.wms.read().unwrap().clone().filter(|w| w.enabled && !w.classes_path.is_empty()) else {
            return Ok(0);
        };
        let url = format!("{}{}", w.base_url.trim_end_matches('/'), w.classes_path);
        let res: Result<Vec<WmsClass>> = async {
            Ok(self
                .http
                .get(&url)
                .headers(Self::headers(&w))
                .timeout(Duration::from_millis(w.timeout_ms.max(2000)))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?)
        }
        .await;
        let list = match res {
            Ok(l) => l,
            Err(e) => {
                self.status.write().unwrap().last_error = Some(format!("sync classi: {e:#}"));
                return Err(e);
            }
        };
        let mut n = 0;
        let mut names = vec![];
        {
            let mut classes = self.classes.write().unwrap();
            for wc in list {
                if let Some(c) = classes.iter_mut().find(|c| c.def.name == wc.name && c.def.source == ClassSource::Wms) {
                    c.set = wc.barcodes.into_iter().collect();
                    if !wc.patterns.is_empty() {
                        c.def.patterns = wc.patterns;
                    }
                    if let Some(d) = wc.destination.as_ref().and_then(json_str) {
                        c.def.destination = d;
                    }
                    names.push(c.def.name.clone());
                    n += 1;
                }
            }
        }
        let mut st = self.status.write().unwrap();
        st.last_sync = Some(now_ms());
        st.last_error = None;
        st.synced_classes = names;
        Ok(n)
    }
}

fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Task: sincronizzazione periodica delle classi WMS.
pub async fn sync_task(s: Arc<Sorting>) {
    loop {
        let every = s.wms.read().unwrap().as_ref().map(|w| w.sync_interval_s.max(5)).unwrap_or(30);
        if s.wms_enabled() {
            match s.sync_classes().await {
                Ok(n) if n > 0 => tracing::debug!("WMS: {n} classi sincronizzate"),
                Err(e) => tracing::warn!("WMS sync classi: {e:#}"),
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_secs(every)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn globs() {
        assert!(glob("IT-MI*", "IT-MI-0001"));
        assert!(glob("??-PA-*", "FR-PA-12"));
        assert!(!glob("IT-MI*", "IT-RM-1"));
        assert!(glob("*", ""));
        assert!(glob("A*B*C", "AxxBCyyC"));
    }

    #[test]
    fn classify() {
        let s = Sorting::new();
        let defs: Vec<SortClass> = serde_yaml::from_str(
            "[{name: VIP, destination: '1', barcodes: [IT-RM-1]}, {name: SUD, destination: '2', patterns: ['IT-RM*', 'IT-NA*']}]",
        )
        .unwrap();
        s.configure(&defs, None);
        assert_eq!(s.classify("IT-RM-1").unwrap().0, "VIP");
        assert_eq!(s.classify("IT-RM-2").unwrap(), ("SUD".into(), "2".into()));
        assert!(s.classify("DE-1").is_none());
        assert!(s.in_class("IT-RM-1", "SUD"));
        assert_eq!(s.classes_of("IT-RM-1"), vec!["VIP", "SUD"]);
    }
}
