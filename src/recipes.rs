//! Ricette (procedure ISA-88): schedulano le sequenze (fasi) su dispositivi.
//!
//! Una ricetta dichiara i **dispositivi** che le servono (alias + classe), i suoi
//! **parametri** (formula) e una rete SFC di passi le cui azioni avviano le fasi
//! (`phase`: sequenza, dispositivo `{alias}`, parametri `{parametro}`), anche in
//! rami paralleli. All'avvio di un batch i dispositivi vengono allocati in
//! esclusiva fino alla fine del batch. Le ricette stanno nel database (dati di
//! produzione, modificabili dal runtime con `recipe.edit`), con versione.

use crate::config::{Config, SeqAction, SeqParam, StepDef, check_sfc};
use crate::store::Store;
use crate::tagdb::now_ms;
use crate::value::Value;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::RwLock;

/// Dispositivo richiesto dalla ricetta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DeviceReq {
    /// Nome usato nella ricetta (`{alias}`), es. "reattore".
    pub alias: String,
    /// Classe richiesta, es. "Reattore".
    pub class: String,
    /// Dispositivo proposto all'avvio.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct RecipeDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub devices: Vec<DeviceReq>,
    /// Parametri della ricetta (il default è il valore di formula).
    #[serde(default)]
    pub parameters: Vec<SeqParam>,
    #[serde(default)]
    pub steps: Vec<StepDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hold_actions: Vec<SeqAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restart_actions: Vec<SeqAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_actions: Vec<SeqAction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abort_actions: Vec<SeqAction>,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub modified: i64,
    #[serde(default)]
    pub modified_by: String,
}

/// Verifica i valori dei parametri (limiti, default, nomi) e li ritorna come testo per `{nome}`.
pub fn resolve_params(what: &str, formal: &[SeqParam], given: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
    if let Some(k) = given.keys().find(|k| !formal.iter().any(|p| &p.name == *k)) {
        bail!("{what}: parametro sconosciuto '{k}'");
    }
    let mut out = BTreeMap::new();
    for p in formal {
        let v = match given.get(&p.name).map(|s| s.trim()).filter(|s| !s.is_empty()) {
            Some(v) => v.to_string(),
            None => match &p.default {
                Some(Value::Null) | None => bail!("{what}: parametro '{}' mancante", p.name),
                Some(v) => v.to_string(),
            },
        };
        if p.min.is_some() || p.max.is_some() {
            let x: f64 = v.parse().map_err(|_| anyhow!("{what}: parametro '{}' deve essere un numero (\"{v}\")", p.name))?;
            if p.min.is_some_and(|m| x < m) || p.max.is_some_and(|m| x > m) {
                bail!(
                    "{what}: parametro '{}' = {x} fuori dai limiti [{} .. {}]",
                    p.name,
                    p.min.map(|m| m.to_string()).unwrap_or("-∞".into()),
                    p.max.map(|m| m.to_string()).unwrap_or("+∞".into())
                );
            }
        }
        out.insert(p.name.clone(), v);
    }
    Ok(out)
}

impl RecipeDef {
    /// Coerenza con il progetto: classi dei dispositivi, sequenze, parametri delle fasi.
    pub fn validate(&self, cfg: &Config) -> Result<()> {
        let n = self.name.trim();
        if n.is_empty() || n.len() > 64 || n.contains('/') {
            bail!("nome ricetta non valido");
        }
        let mut aliases = HashSet::new();
        for d in &self.devices {
            if d.alias.trim().is_empty() || !aliases.insert(d.alias.as_str()) {
                bail!("alias di dispositivo vuoto o duplicato: '{}'", d.alias);
            }
            if !cfg.project.devices.iter().any(|x| x.class == d.class) {
                bail!("dispositivo {}: nessun dispositivo di classe '{}'", d.alias, d.class);
            }
        }
        let mut pn = HashSet::new();
        for p in &self.parameters {
            if !pn.insert(p.name.as_str()) {
                bail!("parametro duplicato '{}'", p.name);
            }
        }
        let seqs: HashSet<String> = cfg.project.sequences.iter().map(|s| s.name.clone()).collect();
        let extra: Vec<&SeqAction> = self.hold_actions.iter().chain(&self.restart_actions).chain(&self.stop_actions).chain(&self.abort_actions).collect();
        check_sfc(&format!("ricetta {n}"), &self.steps, &extra, &seqs, true)?;
        // fasi: dispositivo dichiarato e della classe giusta, parametri esistenti
        fn walk<'a>(steps: &'a [StepDef], out: &mut Vec<&'a SeqAction>) {
            for s in steps {
                out.extend(s.actions.iter().filter(|a| a.kind == "phase"));
                for b in &s.branches {
                    walk(b, out);
                }
            }
        }
        let mut phases = vec![];
        walk(&self.steps, &mut phases);
        for a in phases {
            let sq = cfg.project.sequences.iter().find(|s| s.name == a.sequence).unwrap();
            if !sq.device_class.is_empty() {
                let alias = a.device.trim().trim_start_matches('{').trim_end_matches('}');
                match self.devices.iter().find(|d| d.alias == alias) {
                    Some(d) if d.class != sq.device_class => bail!("fase {}: il dispositivo {{{alias}}} è di classe {} ma la sequenza vuole {}", a.sequence, d.class, sq.device_class),
                    Some(_) => {}
                    None if cfg.project.devices.iter().any(|x| x.name == a.device && x.class == sq.device_class) => {}
                    None => bail!("fase {}: dispositivo '{}' non dichiarato nella ricetta (usa {{alias}})", a.sequence, a.device),
                }
            }
            if let Some(k) = a.params.keys().find(|k| !sq.parameters.iter().any(|p| &p.name == *k)) {
                bail!("fase {}: la sequenza non ha il parametro '{k}'", a.sequence);
            }
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct Recipes {
    data: RwLock<HashMap<String, RecipeDef>>,
    store: RwLock<Option<Store>>,
}

impl Recipes {
    pub async fn open(store: Store) -> Result<Self> {
        let me = Self::default();
        for (k, d) in store.kv_load("recipe_procs").await? {
            if let Ok(r) = serde_json::from_str::<RecipeDef>(&d) {
                me.data.write().unwrap().insert(k, r);
            }
        }
        *me.store.write().unwrap() = Some(store);
        Ok(me)
    }

    fn persist(&self, k: String, data: Option<String>) {
        if let Some(s) = self.store.read().unwrap().clone() {
            tokio::spawn(async move {
                let _ = s.kv_put("recipe_procs", k, data).await;
            });
        }
    }

    pub fn list(&self) -> Vec<RecipeDef> {
        let mut v: Vec<RecipeDef> = self.data.read().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn get(&self, name: &str) -> Option<RecipeDef> {
        self.data.read().unwrap().get(name).cloned()
    }

    /// Salva (nuova versione se cambia). Ritorna la ricetta e la versione precedente.
    pub fn save(&self, cfg: &Config, mut r: RecipeDef, user: &str) -> Result<(RecipeDef, Option<RecipeDef>)> {
        r.name = r.name.trim().to_string();
        r.validate(cfg)?;
        let mut data = self.data.write().unwrap();
        let old = data.get(&r.name).cloned();
        let same = old.as_ref().is_some_and(|o| {
            let (mut a, mut b) = (o.clone(), r.clone());
            for x in [&mut a, &mut b] {
                x.version = 0;
                x.modified = 0;
                x.modified_by.clear();
            }
            a == b
        });
        r.version = match &old {
            Some(o) if same => o.version,
            Some(o) => o.version + 1,
            None => 1,
        };
        r.modified = if same { old.as_ref().map(|o| o.modified).unwrap_or_else(now_ms) } else { now_ms() };
        r.modified_by = if same { old.as_ref().map(|o| o.modified_by.clone()).unwrap_or_default() } else { user.into() };
        data.insert(r.name.clone(), r.clone());
        drop(data);
        self.persist(r.name.clone(), serde_json::to_string(&r).ok());
        Ok((r, old))
    }

    pub fn delete(&self, name: &str) -> Result<RecipeDef> {
        let r = self.data.write().unwrap().remove(name).ok_or_else(|| anyhow!("ricetta {name} non trovata"))?;
        self.persist(name.into(), None);
        Ok(r)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_limits_and_defaults() {
        let f = vec![
            SeqParam { name: "Temp".into(), min: Some(20.0), max: Some(90.0), default: Some(Value::Float(60.0)), ..Default::default() },
            SeqParam { name: "Prod".into(), ..Default::default() },
        ];
        let mut g = BTreeMap::new();
        assert!(resolve_params("t", &f, &g).is_err(), "Prod senza default è obbligatorio");
        g.insert("Prod".to_string(), "Crema".to_string());
        assert_eq!(resolve_params("t", &f, &g).unwrap()["Temp"], "60");
        g.insert("Temp".into(), "95".into());
        assert!(resolve_params("t", &f, &g).is_err());
        g.insert("Temp".into(), "x".into());
        assert!(resolve_params("t", &f, &g).is_err());
        g.insert("Temp".into(), "70".into());
        g.insert("Altro".into(), "1".into());
        assert!(resolve_params("t", &f, &g).is_err());
    }
}
