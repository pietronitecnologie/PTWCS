//! Ricette di processo.
//!
//! Il **tipo di ricetta** (progetto) elenca i parametri e il tag su cui ognuno
//! viene scritto; con `units` lo stesso tipo serve più unità identiche (`{unit}`
//! nei tag). Le **ricette** (insiemi di valori) sono dati di produzione: stanno
//! nel database, si modificano dal runtime con il permesso `recipe.edit` e si
//! caricano sul PLC con `recipe.load`. Ogni modifica e ogni caricamento finiscono
//! nell'audit trail con i valori.

use crate::config::{RecipeType, Config};
use crate::store::Store;
use crate::tagdb::{TagDb, now_ms};
use crate::value::Value;
use anyhow::{Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Recipe {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub values: BTreeMap<String, Value>,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub modified: i64,
    #[serde(default)]
    pub modified_by: String,
}

/// Ultima ricetta caricata su un'unità.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveRecipe {
    pub recipe: String,
    pub version: u32,
    pub ts: i64,
    pub by: String,
    pub ok: bool,
}

/// Esito della scrittura di un parametro.
#[derive(Debug, Clone, Serialize)]
pub struct ParamResult {
    pub param: String,
    pub tag: String,
    pub value: Value,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Modifica di un parametro: (nome, valore precedente, valore nuovo).
pub type Change = (String, Option<Value>, Value);

#[derive(Default)]
pub struct Recipes {
    data: RwLock<HashMap<String, Recipe>>,
    active: RwLock<HashMap<String, ActiveRecipe>>,
    store: RwLock<Option<Store>>,
}

fn key(t: &str, n: &str) -> String {
    format!("{t}/{n}")
}

/// Tag di un parametro per un'unità.
pub fn param_tag(tag: &str, unit: &str) -> String {
    tag.replace("{unit}", unit)
}

fn check_unit(def: &RecipeType, unit: &str) -> Result<()> {
    if def.units.is_empty() {
        if !unit.is_empty() {
            bail!("il tipo di ricetta {} non prevede unità", def.name);
        }
    } else if !def.units.iter().any(|u| u == unit) {
        bail!("unità '{unit}' non valida per {} (ammesse: {})", def.name, def.units.join(", "));
    }
    Ok(())
}

impl Recipes {
    pub async fn open(store: Store) -> Result<Self> {
        let me = Self::default();
        for (k, d) in store.kv_load("recipes").await? {
            if let Ok(r) = serde_json::from_str::<Recipe>(&d) {
                me.data.write().unwrap().insert(k, r);
            }
        }
        for (k, d) in store.kv_load("recipe_active").await? {
            if let Ok(a) = serde_json::from_str::<ActiveRecipe>(&d) {
                me.active.write().unwrap().insert(k, a);
            }
        }
        *me.store.write().unwrap() = Some(store);
        Ok(me)
    }

    fn persist(&self, table: &'static str, k: String, data: Option<String>) {
        if let Some(s) = self.store.read().unwrap().clone() {
            tokio::spawn(async move {
                let _ = s.kv_put(table, k, data).await;
            });
        }
    }

    pub fn list(&self, rtype: &str) -> Vec<Recipe> {
        let pre = format!("{rtype}/");
        let mut v: Vec<Recipe> = self.data.read().unwrap().iter().filter(|(k, _)| k.starts_with(&pre)).map(|(_, r)| r.clone()).collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    pub fn get(&self, rtype: &str, name: &str) -> Option<Recipe> {
        self.data.read().unwrap().get(&key(rtype, name)).cloned()
    }

    /// Ricette attive per unità ("" se il tipo non ha unità).
    pub fn active(&self, rtype: &str) -> BTreeMap<String, ActiveRecipe> {
        let pre = format!("{rtype}/");
        self.active.read().unwrap().iter().filter_map(|(k, a)| k.strip_prefix(&pre).map(|u| (u.to_string(), a.clone()))).collect()
    }

    /// Normalizza e verifica i valori: parametri noti, numeri nei limiti, default per i mancanti.
    pub fn validate(def: &RecipeType, values: &BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>> {
        if let Some(k) = values.keys().find(|k| !def.parameters.iter().any(|p| &p.name == *k)) {
            bail!("parametro sconosciuto: {k}");
        }
        let mut out = BTreeMap::new();
        for p in &def.parameters {
            let v = match values.get(&p.name) {
                Some(Value::Null) | None => p.default.clone().ok_or_else(|| anyhow!("parametro {}: valore mancante", p.name))?,
                Some(v) => v.clone(),
            };
            if p.min.is_some() || p.max.is_some() {
                let x = v.as_f64().ok_or_else(|| anyhow!("parametro {}: serve un numero", p.name))?;
                if p.min.is_some_and(|m| x < m) || p.max.is_some_and(|m| x > m) {
                    bail!("parametro {}: {x} fuori dai limiti [{} .. {}]", p.name, p.min.map(|m| m.to_string()).unwrap_or("-∞".into()), p.max.map(|m| m.to_string()).unwrap_or("+∞".into()));
                }
            }
            out.insert(p.name.clone(), v);
        }
        Ok(out)
    }

    /// Crea o aggiorna una ricetta; ritorna la ricetta e le modifiche (parametro, prima, dopo).
    pub fn save(&self, def: &RecipeType, name: &str, description: String, values: &BTreeMap<String, Value>, user: &str) -> Result<(Recipe, Vec<Change>)> {
        let name = name.trim();
        if name.is_empty() || name.contains('/') || name.len() > 64 {
            bail!("nome ricetta non valido");
        }
        let values = Self::validate(def, values)?;
        let k = key(&def.name, name);
        let mut data = self.data.write().unwrap();
        let old = data.get(&k).cloned();
        let changes: Vec<Change> = values
            .iter()
            .filter(|(p, v)| old.as_ref().and_then(|o| o.values.get(*p)) != Some(*v))
            .map(|(p, v)| (p.clone(), old.as_ref().and_then(|o| o.values.get(p).cloned()), v.clone()))
            .collect();
        let now = now_ms();
        let r = Recipe {
            name: name.into(),
            description,
            values,
            version: old.as_ref().map(|o| o.version + if changes.is_empty() { 0 } else { 1 }).unwrap_or(1),
            created: old.as_ref().map(|o| o.created).unwrap_or(now),
            modified: now,
            modified_by: user.into(),
        };
        data.insert(k.clone(), r.clone());
        drop(data);
        self.persist("recipes", k, serde_json::to_string(&r).ok());
        Ok((r, changes))
    }

    pub fn delete(&self, rtype: &str, name: &str) -> Result<Recipe> {
        let k = key(rtype, name);
        let r = self.data.write().unwrap().remove(&k).ok_or_else(|| anyhow!("ricetta {name} non trovata"))?;
        self.persist("recipes", k, None);
        Ok(r)
    }

    /// Carica una ricetta su un'unità: scrive tutti i parametri sui tag.
    pub async fn load(&self, cfg: &Config, db: &TagDb, rtype: &str, name: &str, unit: &str, user: &str) -> Result<Vec<ParamResult>> {
        let def = cfg.project.recipe_types.iter().find(|r| r.name == rtype).ok_or_else(|| anyhow!("tipo di ricetta {rtype} non trovato"))?;
        check_unit(def, unit)?;
        let r = self.get(rtype, name).ok_or_else(|| anyhow!("ricetta {name} non trovata"))?;
        // rivalida (i limiti del tipo possono essere cambiati dopo il salvataggio)
        let values = Self::validate(def, &r.values)?;
        let mut out = vec![];
        for p in &def.parameters {
            let tag = param_tag(&p.tag, unit);
            let v = values[&p.name].clone();
            let res = db.write(&tag, v.clone()).await;
            out.push(ParamResult { param: p.name.clone(), tag, value: v, ok: res.is_ok(), error: res.err().map(|e| format!("{e:#}")) });
        }
        let ok = out.iter().all(|x| x.ok);
        let a = ActiveRecipe { recipe: name.into(), version: r.version, ts: now_ms(), by: user.into(), ok };
        let k = key(rtype, unit);
        self.persist("recipe_active", k.clone(), serde_json::to_string(&a).ok());
        self.active.write().unwrap().insert(k, a);
        Ok(out)
    }

    /// Valori attuali dei tag di un'unità (lettura dal PLC).
    pub fn read(def: &RecipeType, db: &TagDb, unit: &str) -> Result<BTreeMap<String, Value>> {
        check_unit(def, unit)?;
        Ok(def.parameters.iter().map(|p| (p.name.clone(), db.value(&param_tag(&p.tag, unit)))).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::RecipeParam;

    #[test]
    fn validation_limits_and_defaults() {
        let def = RecipeType {
            name: "Batch".into(),
            description: String::new(),
            area: None,
            units: vec!["R1".into()],
            parameters: vec![
                RecipeParam { name: "Temp".into(), tag: "{unit}.SP_Temp".into(), unit: "°C".into(), min: Some(20.0), max: Some(90.0), default: Some(Value::Float(60.0)), description: String::new() },
                RecipeParam { name: "Nome".into(), tag: "{unit}.Prod".into(), unit: String::new(), min: None, max: None, default: None, description: String::new() },
            ],
        };
        let mut v = BTreeMap::new();
        v.insert("Nome".to_string(), Value::Str("Impasto".into()));
        let out = Recipes::validate(&def, &v).unwrap();
        assert_eq!(out["Temp"], Value::Float(60.0));
        v.insert("Temp".into(), Value::Float(95.0));
        assert!(Recipes::validate(&def, &v).is_err());
        v.insert("Altro".into(), Value::Int(1));
        assert!(Recipes::validate(&def, &v).is_err());
        assert_eq!(param_tag("{unit}.SP_Temp", "R1"), "R1.SP_Temp");
        assert!(check_unit(&def, "R9").is_err());
    }
}
