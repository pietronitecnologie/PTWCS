//! Regole di smistamento "no-code", create graficamente nel Designer.
//!
//! Ogni punto di decisione può avere un insieme ordinato di regole: la prima
//! regola abilitata le cui condizioni sono vere determina le uscite. Se nessuna
//! regola scatta si usa l'azione di default; se manca, la funzione Python del
//! punto (se configurata); altrimenti il fallback del punto.
//!
//! Campi delle condizioni: alias degli input del punto (es. `barcode`),
//! `request_id`, oppure un tag qualsiasi con il prefisso `tag:` (es. `tag:WCS.Out3Enabled`).
//! I valori di confronto e delle uscite possono anch'essi essere `tag:Nome`.
//! Uscite con valori alternativi `1|2|3` vengono distribuite a rotazione
//! (bilanciamento tra destinazioni equivalenti).

use crate::config::DecisionPointDef;
use crate::value::Value;
use anyhow::{Context, Result, bail};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    /// valore "min..max" (estremi inclusi)
    Between,
    StartsWith,
    EndsWith,
    Contains,
    /// lista separata da virgole
    In,
    NotIn,
    Regex,
    Empty,
    NotEmpty,
    /// il valore appartiene a una delle classi (lista separata da virgole)
    InClass,
    NotInClass,
    /// il valore appartiene ad almeno una classe di smistamento
    HasClass,
    /// il WMS conosce una destinazione per il valore
    WmsKnown,
}

/// Accesso ai dati esterni durante la valutazione.
pub trait Resolver {
    fn tag(&self, name: &str) -> Value;
    fn in_class(&self, value: &str, class: &str) -> bool;
    /// Destinazione della prima classe che contiene il valore.
    fn class_dest(&self, value: &str) -> Option<String>;
    /// Destinazione fornita dal WMS (già in cache).
    fn wms_dest(&self, value: &str) -> Option<String>;
}

/// Controlli di esistenza per la validazione.
pub struct Checker<'a> {
    pub tag: &'a dyn Fn(&str) -> bool,
    pub class: &'a dyn Fn(&str) -> bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub field: String,
    pub op: Op,
    #[serde(default)]
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Action {
    /// alias uscita -> valore ("3", "1|2" a rotazione, "tag:Nome")
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    /// Funzione Python a cui delegare la decisione (opzionale).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<String>,
    /// Messaggio nel log eventi ({campo} viene sostituito con il valore).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// "all" (tutte vere) oppure "any" (almeno una vera)
    #[serde(default = "all")]
    pub mode: String,
    #[serde(default)]
    pub when: Vec<Condition>,
    #[serde(default)]
    pub then: Action,
}
fn yes() -> bool {
    true
}
fn all() -> String {
    "all".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSet {
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Action>,
}

impl Default for RuleSet {
    fn default() -> Self {
        Self { enabled: true, description: String::new(), rules: vec![], default: None }
    }
}

/// Esito della valutazione.
#[derive(Debug, Clone, Serialize)]
pub struct Outcome {
    /// id regola, oppure "default"
    pub rule: String,
    pub rule_name: String,
    pub outputs: BTreeMap<String, Value>,
    pub script: Option<String>,
    pub log: Option<String>,
}

/// Traccia per il test dal Designer: esito di ogni condizione di ogni regola.
#[derive(Debug, Clone, Serialize)]
pub struct RuleTrace {
    pub id: String,
    pub enabled: bool,
    pub conditions: Vec<bool>,
    pub matched: bool,
}

struct Compiled {
    set: RuleSet,
    regex: HashMap<String, Regex>,
    hits: HashMap<String, u64>,
    rr: HashMap<String, usize>,
}

pub struct RulesEngine {
    dir: PathBuf,
    sets: RwLock<HashMap<String, Compiled>>,
}

fn compile(set: &RuleSet) -> Result<HashMap<String, Regex>> {
    let mut m = HashMap::new();
    for r in &set.rules {
        for c in &r.when {
            if c.op == Op::Regex && !m.contains_key(&c.value) {
                let re = Regex::new(&c.value).with_context(|| format!("regola '{}': regex non valida", r.name))?;
                m.insert(c.value.clone(), re);
            }
        }
    }
    Ok(m)
}

fn num(s: &str) -> Option<f64> {
    s.trim().replace(',', ".").parse().ok()
}

/// Confronto "intelligente": numerico se entrambi sono numeri, testuale altrimenti.
fn cmp(a: &str, b: &str) -> std::cmp::Ordering {
    match (num(a), num(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.cmp(b),
    }
}

fn value_str(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// Converte un valore testuale di uscita nel tipo più naturale.
fn parse_out(s: &str) -> Value {
    let t = s.trim();
    if let Ok(i) = t.parse::<i64>() {
        Value::Int(i)
    } else if let Some(f) = num(t).filter(|_| t.contains('.') || t.contains(',')) {
        Value::Float(f)
    } else if t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("false") {
        Value::Bool(t.eq_ignore_ascii_case("true"))
    } else {
        Value::Str(t.to_string())
    }
}

impl RulesEngine {
    /// Carica `<dir>/<punto>.json` per ogni punto di decisione.
    pub fn load(dir: PathBuf, points: &[DecisionPointDef]) -> Result<Self> {
        std::fs::create_dir_all(&dir)?;
        let mut sets = HashMap::new();
        for dp in points {
            let p = dir.join(format!("{}.json", dp.name));
            if !p.exists() {
                continue;
            }
            let set: RuleSet = serde_json::from_str(&std::fs::read_to_string(&p)?)
                .with_context(|| format!("regole {}", p.display()))?;
            validate(&set, dp, &Checker { tag: &|_| true, class: &|_| true })?;
            let regex = compile(&set)?;
            tracing::info!("regole {}: {} caricate", dp.name, set.rules.len());
            sets.insert(dp.name.clone(), Compiled { set, regex, hits: Default::default(), rr: Default::default() });
        }
        Ok(Self { dir, sets: RwLock::new(sets) })
    }

    pub fn get(&self, point: &str) -> Option<RuleSet> {
        self.sets.read().unwrap().get(point).map(|c| c.set.clone())
    }

    pub fn hits(&self, point: &str) -> HashMap<String, u64> {
        self.sets.read().unwrap().get(point).map(|c| c.hits.clone()).unwrap_or_default()
    }

    pub fn reset_hits(&self, point: &str) {
        if let Some(c) = self.sets.write().unwrap().get_mut(point) {
            c.hits.clear();
        }
    }

    /// Salva e applica subito un insieme di regole.
    pub fn save(&self, dp: &DecisionPointDef, set: RuleSet, chk: &Checker) -> Result<()> {
        validate(&set, dp, chk)?;
        let regex = compile(&set)?;
        let p = self.dir.join(format!("{}.json", dp.name));
        let tmp = p.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(&set)?)?;
        std::fs::rename(tmp, &p)?;
        let mut sets = self.sets.write().unwrap();
        let hits = sets.remove(&dp.name).map(|c| c.hits).unwrap_or_default();
        sets.insert(dp.name.clone(), Compiled { set, regex, hits, rr: Default::default() });
        Ok(())
    }

    /// Campi di input per cui le regole chiedono la destinazione al WMS
    /// (da interrogare prima della valutazione).
    pub fn wms_fields(&self, point: &str) -> Vec<String> {
        let sets = self.sets.read().unwrap();
        let Some(c) = sets.get(point) else { return vec![] };
        let mut out = vec![];
        let actions = c.set.rules.iter().map(|r| &r.then).chain(c.set.default.iter());
        for a in actions {
            for v in a.outputs.values() {
                if let Some(f) = v.trim().strip_prefix("wms:") {
                    out.push(f.trim().to_string());
                }
            }
        }
        for r in &c.set.rules {
            for cond in &r.when {
                if cond.op == Op::WmsKnown {
                    out.push(cond.field.clone());
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    /// Valuta le regole di un punto. `inputs`: alias -> valore.
    /// Con `dry_run` non aggiorna contatori e rotazioni (test dal Designer).
    pub fn evaluate(
        &self,
        point: &str,
        inputs: &HashMap<String, Value>,
        res: &dyn Resolver,
        dry_run: bool,
    ) -> (Option<Outcome>, Vec<RuleTrace>) {
        let mut sets = self.sets.write().unwrap();
        let Some(c) = sets.get_mut(point) else { return (None, vec![]) };
        if !c.set.enabled {
            return (None, vec![]);
        }
        let resolve = |s: &str| -> String {
            match s.strip_prefix("tag:") {
                Some(t) => value_str(&res.tag(t.trim())),
                None => s.to_string(),
            }
        };
        let field = |f: &str| -> String {
            match f.strip_prefix("tag:") {
                Some(t) => value_str(&res.tag(t.trim())),
                None => inputs.get(f).map(value_str).unwrap_or_default(),
            }
        };
        // Risolve le uscite di un'azione; None se un valore class:/wms: non è disponibile.
        let outputs_of = |c: &mut Compiled, id: &str, a: &Action| -> Option<BTreeMap<String, Value>> {
            let mut outputs = BTreeMap::new();
            for (alias, raw) in &a.outputs {
                let raw = raw.trim();
                let raw = if let Some(f) = raw.strip_prefix("class:") {
                    res.class_dest(&field(f.trim()))?
                } else if let Some(f) = raw.strip_prefix("wms:") {
                    res.wms_dest(&field(f.trim()))?
                } else {
                    raw.to_string()
                };
                let chosen = if raw.contains('|') {
                    let opts: Vec<&str> = raw.split('|').map(str::trim).filter(|s| !s.is_empty()).collect();
                    let key = format!("{id}/{alias}");
                    let i = *c.rr.get(&key).unwrap_or(&0);
                    if !dry_run {
                        c.rr.insert(key, i + 1);
                    }
                    opts.get(i % opts.len().max(1)).copied().unwrap_or("").to_string()
                } else {
                    raw
                };
                let v = match chosen.strip_prefix("tag:") {
                    Some(t) => res.tag(t.trim()),
                    None => parse_out(&chosen),
                };
                outputs.insert(alias.clone(), v);
            }
            Some(outputs)
        };
        let mut trace = vec![];
        let mut hit: Option<(String, String, Action, BTreeMap<String, Value>)> = None;
        let rules = c.set.rules.clone();
        for r in &rules {
            let results: Vec<bool> = r
                .when
                .iter()
                .map(|cond| {
                    let a = field(&cond.field);
                    let b = resolve(&cond.value);
                    match cond.op {
                        Op::Eq => cmp(&a, &b).is_eq(),
                        Op::Ne => !cmp(&a, &b).is_eq(),
                        Op::Gt => !a.is_empty() && cmp(&a, &b).is_gt(),
                        Op::Ge => !a.is_empty() && cmp(&a, &b).is_ge(),
                        Op::Lt => !a.is_empty() && cmp(&a, &b).is_lt(),
                        Op::Le => !a.is_empty() && cmp(&a, &b).is_le(),
                        Op::Between => match b.split_once("..") {
                            Some((lo, hi)) => {
                                !a.is_empty() && cmp(&a, &resolve(lo.trim())).is_ge() && cmp(&a, &resolve(hi.trim())).is_le()
                            }
                            None => false,
                        },
                        Op::StartsWith => a.starts_with(&b),
                        Op::EndsWith => a.ends_with(&b),
                        Op::Contains => a.contains(&b),
                        Op::In => b.split(',').any(|x| cmp(&a, &resolve(x.trim())).is_eq()),
                        Op::NotIn => !b.split(',').any(|x| cmp(&a, &resolve(x.trim())).is_eq()),
                        Op::Regex => c.regex.get(&cond.value).is_some_and(|re| re.is_match(&a)),
                        Op::Empty => a.trim().is_empty(),
                        Op::NotEmpty => !a.trim().is_empty(),
                        Op::InClass => b.split(',').any(|cl| res.in_class(&a, cl.trim())),
                        Op::NotInClass => !b.split(',').any(|cl| res.in_class(&a, cl.trim())),
                        Op::HasClass => res.class_dest(&a).is_some(),
                        Op::WmsKnown => res.wms_dest(&a).is_some(),
                    }
                })
                .collect();
            let mut matched = r.enabled
                && if r.mode == "any" { results.iter().any(|x| *x) } else { results.iter().all(|x| *x) };
            if matched && hit.is_none() {
                match outputs_of(c, &r.id, &r.then) {
                    Some(o) => hit = Some((r.id.clone(), r.name.clone(), r.then.clone(), o)),
                    None => matched = false, // uscita class:/wms: non risolta: si prova la regola successiva
                }
            }
            trace.push(RuleTrace { id: r.id.clone(), enabled: r.enabled, conditions: results, matched });
            if hit.is_some() && !dry_run {
                break;
            }
        }
        let (id, name, action, outputs) = match hit {
            Some(h) => h,
            None => match c.set.default.clone() {
                Some(d) => match outputs_of(c, "default", &d) {
                    Some(o) => ("default".to_string(), "Default".to_string(), d, o),
                    None => return (None, trace),
                },
                None => return (None, trace),
            },
        };
        let log = action.log.as_ref().map(|l| {
            let mut s = l.clone();
            for (k, v) in inputs {
                s = s.replace(&format!("{{{k}}}"), &value_str(v));
            }
            s
        });
        if !dry_run {
            *c.hits.entry(id.clone()).or_default() += 1;
        }
        (Some(Outcome { rule: id, rule_name: name, outputs, script: action.script.clone(), log }), trace)
    }
}

fn validate(set: &RuleSet, dp: &DecisionPointDef, chk: &Checker) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    let known_field = |f: &str| f == "request_id" || dp.inputs.contains_key(f);
    let check_ref = |s: &str, ctx: &str| -> Result<()> {
        let s = s.trim();
        if let Some(t) = s.strip_prefix("tag:") {
            if !(chk.tag)(t.trim()) {
                bail!("{ctx}: tag sconosciuto '{}'", t.trim());
            }
        }
        for p in ["class:", "wms:"] {
            if let Some(f) = s.strip_prefix(p) {
                if !known_field(f.trim()) {
                    bail!("{ctx}: '{s}' – campo '{}' non è un input del punto", f.trim());
                }
            }
        }
        Ok(())
    };
    let check_action = |a: &Action, ctx: &str| -> Result<()> {
        for (k, v) in &a.outputs {
            if !dp.outputs.contains_key(k) {
                bail!("{ctx}: uscita '{k}' non esiste nel punto {} (uscite: {:?})", dp.name, dp.outputs.keys().collect::<Vec<_>>());
            }
            for part in v.split('|') {
                check_ref(part, ctx)?;
            }
        }
        if a.outputs.is_empty() && a.script.is_none() {
            bail!("{ctx}: indicare almeno un'uscita o uno script");
        }
        Ok(())
    };
    for r in &set.rules {
        let ctx = format!("regola '{}'", if r.name.is_empty() { &r.id } else { &r.name });
        if !ids.insert(r.id.clone()) {
            bail!("{ctx}: id duplicato");
        }
        if r.when.is_empty() {
            bail!("{ctx}: nessuna condizione (usa il default per il caso generale)");
        }
        for c in &r.when {
            let f = c.field.trim();
            let known = f == "request_id" || dp.inputs.contains_key(f) || f.starts_with("tag:");
            if !known {
                bail!("{ctx}: campo '{f}' sconosciuto (input: {:?}, oppure tag:Nome)", dp.inputs.keys().collect::<Vec<_>>());
            }
            check_ref(f, &ctx)?;
            if matches!(c.op, Op::InClass | Op::NotInClass) {
                for cl in c.value.split(',') {
                    if !(chk.class)(cl.trim()) {
                        bail!("{ctx}: classe di smistamento sconosciuta '{}'", cl.trim());
                    }
                }
            } else if !matches!(c.op, Op::Empty | Op::NotEmpty | Op::Regex | Op::HasClass | Op::WmsKnown) {
                for part in c.value.split([',']).flat_map(|x| x.split("..")) {
                    check_ref(part, &ctx)?;
                }
            }
            if c.op == Op::Between && !c.value.contains("..") {
                bail!("{ctx}: 'compreso tra' richiede il formato min..max");
            }
        }
        check_action(&r.then, &ctx)?;
    }
    if let Some(d) = &set.default {
        check_action(d, "default")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dp() -> DecisionPointDef {
        serde_yaml::from_str(
            "{name: SC1, request_tag: R, response_tag: A, inputs: {barcode: B, weight: W}, outputs: {destination: D, result: X}}",
        )
        .unwrap()
    }

    fn engine(set: &str) -> RulesEngine {
        let dir = std::env::temp_dir().join(format!("wcs_rules_{}", std::process::id()));
        let e = RulesEngine::load(dir, &[]).unwrap();
        let set: RuleSet = serde_json::from_str(set).unwrap();
        e.save(&dp(), set, &Checker { tag: &|t| t == "MAXW" || t == "OUT3", class: &|c| c == "SUD" }).unwrap();
        e
    }

    struct R;
    impl Resolver for R {
        fn tag(&self, t: &str) -> Value {
            match t {
                "MAXW" => Value::Int(3000),
                "OUT3" => Value::Bool(false),
                _ => Value::Null,
            }
        }
        fn in_class(&self, v: &str, c: &str) -> bool {
            c == "SUD" && v.starts_with("IT-NA")
        }
        fn class_dest(&self, v: &str) -> Option<String> {
            v.starts_with("IT-NA").then(|| "2".into())
        }
        fn wms_dest(&self, v: &str) -> Option<String> {
            (v == "WMS-1").then(|| "7".into())
        }
    }

    fn eval(e: &RulesEngine, bc: &str, w: i64) -> Option<Outcome> {
        let inputs = HashMap::from([("barcode".to_string(), Value::Str(bc.into())), ("weight".to_string(), Value::Int(w))]);
        e.evaluate("SC1", &inputs, &R, false).0
    }

    #[test]
    fn rules_first_match_default_rr() {
        let e = engine(
            r#"{"rules":[
              {"id":"a","name":"noread","when":[{"field":"barcode","op":"in","value":"NOREAD,"}],"then":{"outputs":{"destination":"99","result":"1"}}},
              {"id":"b","when":[{"field":"weight","op":"gt","value":"tag:MAXW"}],"then":{"outputs":{"destination":"99","result":"2"}}},
              {"id":"c","when":[{"field":"barcode","op":"regex","value":"^IT-(MI|TO)"}],"then":{"outputs":{"destination":"1"}}},
              {"id":"d","when":[{"field":"barcode","op":"starts_with","value":"DE"},{"field":"tag:OUT3","op":"eq","value":"true"}],"then":{"outputs":{"destination":"3"}}},
              {"id":"e","when":[{"field":"barcode","op":"starts_with","value":"DE"}],"then":{"outputs":{"destination":"2|3"}}}
            ],"default":{"outputs":{"destination":"99","result":"3"}}}"#,
        );
        assert_eq!(eval(&e, "NOREAD", 10).unwrap().rule, "a");
        assert_eq!(eval(&e, "IT-MI-1", 3500).unwrap().rule, "b");
        let o = eval(&e, "IT-TO-9", 100).unwrap();
        assert_eq!((o.rule.as_str(), &o.outputs["destination"]), ("c", &Value::Int(1)));
        // OUT3=false -> regola d non scatta, e alterna 2/3
        let d1 = eval(&e, "DE-1", 1).unwrap();
        let d2 = eval(&e, "DE-2", 1).unwrap();
        assert_eq!((d1.rule.as_str(), &d1.outputs["destination"], &d2.outputs["destination"]), ("e", &Value::Int(2), &Value::Int(3)));
        assert_eq!(eval(&e, "XX", 1).unwrap().rule, "default");
        assert_eq!(e.hits("SC1")["e"], 2);
    }

    #[test]
    fn classes_and_wms_chain() {
        let e = engine(
            r#"{"rules":[
              {"id":"w","when":[{"field":"barcode","op":"not_empty"}],"then":{"outputs":{"destination":"wms:barcode"}}},
              {"id":"c","when":[{"field":"barcode","op":"in_class","value":"SUD"}],"then":{"outputs":{"destination":"class:barcode"}}}
            ],"default":{"outputs":{"destination":"99"}}}"#,
        );
        // WMS conosce il collo
        assert_eq!(eval(&e, "WMS-1", 1).unwrap().outputs["destination"], Value::Int(7));
        // WMS no, classe sì: la regola 'w' viene saltata
        let o = eval(&e, "IT-NA-5", 1).unwrap();
        assert_eq!((o.rule.as_str(), &o.outputs["destination"]), ("c", &Value::Int(2)));
        assert_eq!(eval(&e, "ZZ", 1).unwrap().rule, "default");
        assert_eq!(e.wms_fields("SC1"), vec!["barcode"]);
    }

    #[test]
    fn validation() {
        let dir = std::env::temp_dir().join(format!("wcs_rules_v_{}", std::process::id()));
        let e = RulesEngine::load(dir, &[]).unwrap();
        let bad = |s: &str| e.save(&dp(), serde_json::from_str(s).unwrap(), &Checker { tag: &|_| false, class: &|_| false }).is_err();
        assert!(bad(r#"{"rules":[{"id":"a","when":[{"field":"barcode","op":"in_class","value":"NOPE"}],"then":{"outputs":{"destination":"1"}}}]}"#));
        assert!(bad(r#"{"rules":[{"id":"a","when":[{"field":"nope","op":"eq","value":"1"}],"then":{"outputs":{"destination":"1"}}}]}"#));
        assert!(bad(r#"{"rules":[{"id":"a","when":[{"field":"barcode","op":"eq","value":"1"}],"then":{"outputs":{"bogus":"1"}}}]}"#));
        assert!(bad(r#"{"rules":[{"id":"a","when":[{"field":"barcode","op":"regex","value":"("}],"then":{"outputs":{"destination":"1"}}}]}"#));
        assert!(bad(r#"{"rules":[{"id":"a","when":[{"field":"tag:X","op":"eq","value":"1"}],"then":{"outputs":{"destination":"1"}}}]}"#));
    }
}
