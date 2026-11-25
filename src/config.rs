//! Configurazione dell'impianto (file YAML): driver, tag, punti di decisione
//! e allarmi. Le viste HMI sono file JSON creati con il Designer. Tutto l'impianto è descritto qui: il core è generico.

use crate::value::{DataType, Value};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub plant: PlantInfo,
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub scripting: ScriptingConfig,
    pub drivers: Vec<DriverConfig>,
    /// Tag definiti nel file di impianto (sola lettura dal Designer).
    #[serde(default)]
    pub tags: Vec<TagDef>,
    /// File dei tag gestiti dal Designer (creazione/modifica a caldo).
    #[serde(default = "default_tags_file")]
    pub tags_file: String,
    /// Directory delle regole di smistamento grafiche (un JSON per punto di decisione).
    #[serde(default = "default_rules_dir")]
    pub rules_dir: String,
    #[serde(default)]
    pub decision_points: Vec<DecisionPointDef>,
    #[serde(default)]
    pub alarms: Vec<AlarmDef>,
    /// Directory del file di configurazione: i path relativi partono da qui.
    #[serde(skip)]
    pub base_dir: PathBuf,
    /// Nomi dei tag che provengono da `tags_file` (modificabili dal Designer).
    #[serde(skip)]
    pub editable_tags: HashSet<String>,
}
fn default_tags_file() -> String {
    "tags.yaml".into()
}
fn default_rules_dir() -> String {
    "rules".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PlantInfo {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    #[serde(default = "default_db")]
    pub database: String,
    /// Directory dei file statici dell'HMI.
    #[serde(default = "default_web")]
    pub web_root: String,
    /// Directory delle viste HMI (JSON, create con l'editor).
    #[serde(default = "default_views")]
    pub views_dir: String,
    /// Directory dei simboli SVG importati.
    #[serde(default = "default_symbols")]
    pub symbols_dir: String,
    /// Abilita il Designer (modifica viste, simboli e script dal browser).
    #[serde(default = "yes")]
    pub designer: bool,
    /// Giorni di conservazione di storico e log decisioni.
    #[serde(default = "default_retention")]
    pub retention_days: u32,
}
fn default_views() -> String {
    "views".into()
}
fn default_symbols() -> String {
    "symbols".into()
}
fn yes() -> bool {
    true
}
fn default_retention() -> u32 {
    30
}
fn default_bind() -> String {
    "0.0.0.0:8080".into()
}
fn default_db() -> String {
    "wcs.db".into()
}
fn default_web() -> String {
    "../web".into()
}
impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            database: default_db(),
            web_root: default_web(),
            views_dir: default_views(),
            symbols_dir: default_symbols(),
            designer: true,
            retention_days: default_retention(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScriptingConfig {
    /// Interprete Python da usare.
    #[serde(default = "default_python")]
    pub python: String,
    /// Modulo Python con le funzioni di decisione (path relativo al file di config).
    pub module: Option<String>,
}
fn default_python() -> String {
    "python3".into()
}
impl Default for ScriptingConfig {
    fn default() -> Self {
        Self { python: default_python(), module: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    /// Siemens S7 (ISO-on-TCP, porta 102) – S7-300/400/1200/1500.
    S7,
    /// Modbus TCP.
    Modbus,
    /// OPC UA (client).
    Opcua,
    /// Allen-Bradley EtherNet/IP (CIP, tag Logix).
    Eip,
    /// PLC simulato in memoria (sviluppo / test).
    Sim,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DriverConfig {
    pub name: String,
    pub protocol: Protocol,
    #[serde(default)]
    pub host: String,
    pub port: Option<u16>,
    /// Modbus: unit id / slave id.
    #[serde(default = "one")]
    pub unit_id: u8,
    /// S7: rack e slot della CPU (S7-1200/1500: 0/1, S7-300: 0/2).
    #[serde(default)]
    pub rack: u16,
    #[serde(default = "one_u16")]
    pub slot: u16,
    /// OPC UA: endpoint completo, es. opc.tcp://10.0.0.5:4840
    #[serde(default)]
    pub endpoint: String,
    /// Periodo di polling in ms.
    #[serde(default = "default_poll")]
    pub poll_ms: u64,
    /// Timeout di comunicazione in ms.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}
fn one() -> u8 {
    1
}
fn one_u16() -> u16 {
    1
}
fn default_poll() -> u64 {
    100
}
fn default_timeout() -> u64 {
    2000
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TagDef {
    pub name: String,
    pub driver: String,
    /// Indirizzo specifico del protocollo:
    /// - S7:     `DB10.DBX0.1`, `DB10.DBW2`, `DB10.DBD4`, `DB10.DBB8`, `M10.0`, `MW20`, `I0.0`, `Q0.1`
    /// - Modbus: `coil:0`, `di:5`, `hr:100`, `ir:3`
    /// - OPC UA: NodeId, es. `ns=3;s="DB_WCS"."ReqID"`
    /// - EIP:    nome tag Logix, es. `Program:Main.ReqID`
    /// - Sim:    qualsiasi stringa (chiave di memoria)
    pub address: String,
    #[serde(rename = "type")]
    pub dtype: DataType,
    /// Lunghezza in caratteri per i tag `string`.
    #[serde(default = "default_str_len", skip_serializing_if = "is_default_len")]
    pub len: usize,
    #[serde(default, skip_serializing_if = "is_false")]
    pub writable: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// Registra i cambi valore nello storico (trend).
    #[serde(default, skip_serializing_if = "is_false")]
    pub historize: bool,
    /// Valore iniziale (solo driver `sim`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<Value>,
}
fn is_false(b: &bool) -> bool {
    !*b
}
fn is_default_len(n: &usize) -> bool {
    *n == default_str_len()
}
fn default_str_len() -> usize {
    20
}

/// Punto di decisione: il PLC pone una "domanda" (handshake ReqID/RespID)
/// e il WCS risponde scrivendo le uscite calcolate da uno script Python.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecisionPointDef {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Tag contatore richiesta scritto dal PLC (incrementato ad ogni domanda).
    pub request_tag: String,
    /// Tag di risposta scritto dal WCS (eco del request id a risposta pronta).
    pub response_tag: String,
    /// Dati letti dal PLC al momento della richiesta: alias -> tag.
    #[serde(default)]
    pub inputs: BTreeMap<String, String>,
    /// Dati scritti al PLC come risposta: alias -> tag.
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    /// Funzione Python da invocare: `def f(req, ctx) -> dict`. Opzionale se il
    /// punto usa le regole grafiche (in tal caso è chiamata se nessuna regola scatta).
    #[serde(default)]
    pub function: Option<String>,
    #[serde(default = "default_decision_timeout")]
    pub timeout_ms: u64,
    /// Uscite di fallback se lo script fallisce o va in timeout.
    #[serde(default)]
    pub fallback: BTreeMap<String, Value>,
}
fn default_decision_timeout() -> u64 {
    300
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlarmDef {
    pub id: String,
    pub tag: String,
    /// Operatore: ==, !=, >, >=, <, <=
    #[serde(default = "default_op")]
    pub op: String,
    #[serde(default = "default_alarm_value")]
    pub value: Value,
    pub message: String,
    #[serde(default = "default_severity")]
    pub severity: Severity,
}
fn default_op() -> String {
    "==".into()
}
fn default_alarm_value() -> Value {
    Value::Bool(true)
}
fn default_severity() -> Severity {
    Severity::Warning
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Alarm,
    Critical,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("lettura config {}", path.display()))?;
        let mut cfg: Config = serde_yaml::from_str(&text)
            .with_context(|| format!("parsing config {}", path.display()))?;
        cfg.base_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let tf = cfg.resolve(&cfg.tags_file);
        if tf.exists() {
            let text = std::fs::read_to_string(&tf).with_context(|| format!("lettura {}", tf.display()))?;
            let extra: Vec<TagDef> = if text.trim().is_empty() {
                vec![]
            } else {
                serde_yaml::from_str(&text).with_context(|| format!("parsing {}", tf.display()))?
            };
            cfg.editable_tags = extra.iter().map(|t| t.name.clone()).collect();
            cfg.tags.extend(extra);
        }
        cfg.validate()?;
        Ok(cfg)
    }

    /// Salva i tag modificabili nel file gestito dal Designer (scrittura atomica).
    pub fn save_editable_tags(&self, tags: &[TagDef]) -> Result<()> {
        let list: Vec<&TagDef> = tags.iter().filter(|t| self.editable_tags.contains(&t.name)).collect();
        let path = self.resolve(&self.tags_file);
        let tmp = path.with_extension("yaml.tmp");
        let header = "# Tag gestiti dal Designer di WCS-SCADA: modificabili anche a mano (a WCS fermo).\n";
        std::fs::write(&tmp, format!("{header}{}", serde_yaml::to_string(&list)?))?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn resolve(&self, rel: &str) -> PathBuf {
        let p = Path::new(rel);
        if p.is_absolute() { p.to_path_buf() } else { self.base_dir.join(p) }
    }

    pub fn tag(&self, name: &str) -> Option<&TagDef> {
        self.tags.iter().find(|t| t.name == name)
    }

    /// Controlli di coerenza: errori di configurazione emergono all'avvio,
    /// non durante la produzione.
    pub fn validate(&self) -> Result<()> {
        let mut drivers = HashSet::new();
        for d in &self.drivers {
            if !drivers.insert(d.name.as_str()) {
                bail!("driver duplicato: {}", d.name);
            }
        }
        let mut tags = HashSet::new();
        for t in &self.tags {
            if !tags.insert(t.name.as_str()) {
                bail!("tag duplicato: {}", t.name);
            }
            if !drivers.contains(t.driver.as_str()) {
                bail!("tag {}: driver sconosciuto '{}'", t.name, t.driver);
            }
        }
        let need_tag = |ctx: &str, name: &str| -> Result<()> {
            if !tags.contains(name) {
                bail!("{ctx}: tag sconosciuto '{name}'");
            }
            Ok(())
        };
        let need_writable = |ctx: &str, name: &str| -> Result<()> {
            need_tag(ctx, name)?;
            if !self.tag(name).map(|t| t.writable).unwrap_or(false) {
                bail!("{ctx}: il tag '{name}' deve essere writable");
            }
            Ok(())
        };
        for dp in &self.decision_points {
            let ctx = format!("decision point {}", dp.name);
            need_tag(&ctx, &dp.request_tag)?;
            need_writable(&ctx, &dp.response_tag)?;
            for t in dp.inputs.values() {
                need_tag(&ctx, t)?;
            }
            for t in dp.outputs.values() {
                need_writable(&ctx, t)?;
            }
            for k in dp.fallback.keys() {
                if !dp.outputs.contains_key(k) {
                    bail!("{ctx}: fallback '{k}' non è un output");
                }
            }
        }
        if self.decision_points.iter().any(|d| d.function.is_some()) && self.scripting.module.is_none() {
            bail!("ci sono decision_points con 'function' ma manca scripting.module");
        }
        for a in &self.alarms {
            need_tag(&format!("allarme {}", a.id), &a.tag)?;
            if !["==", "!=", ">", ">=", "<", "<="].contains(&a.op.as_str()) {
                bail!("allarme {}: operatore non valido '{}'", a.id, a.op);
            }
        }
        Ok(())
    }
}
