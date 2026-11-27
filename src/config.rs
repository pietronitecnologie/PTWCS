//! Configurazione.
//!
//! - `plant.yaml` (file di impianto, scritto a mano): server, scripting, driver,
//!   più eventuali tag/aree/UDT "statici" (sola lettura per il Designer).
//! - `project.yaml` (progetto, gestito dal Designer): aree, scan class, tipi UDT
//!   e istanze, tag, punti di decisione (protocollo request/reply), classi di
//!   smistamento, connessione WMS, allarmi.
//!
//! La configurazione *effettiva* è l'unione dei due, con le UDT espanse in tag
//! e i punti di decisione basati su telegramma risolti.

use crate::value::{DataType, Value};
use anyhow::{Context, Result, anyhow, bail};
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
    /// File di progetto gestito dal Designer.
    #[serde(default = "default_project_file")]
    pub project_file: String,
    /// Directory delle regole di smistamento grafiche (un JSON per punto di decisione).
    #[serde(default = "default_rules_dir")]
    pub rules_dir: String,

    // --- sezioni presenti sia nel file di impianto sia nel progetto
    #[serde(default)]
    pub areas: Vec<Area>,
    #[serde(default)]
    pub scan_classes: Vec<ScanClass>,
    #[serde(default)]
    pub udt_types: Vec<UdtType>,
    #[serde(default)]
    pub udt_instances: Vec<UdtInstance>,
    #[serde(default)]
    pub tags: Vec<TagDef>,
    #[serde(default)]
    pub decision_points: Vec<DecisionPointDef>,
    #[serde(default)]
    pub sort_classes: Vec<SortClass>,
    #[serde(default)]
    pub wms: Option<WmsConfig>,
    #[serde(default)]
    pub alarms: Vec<AlarmDef>,
    #[serde(default)]
    pub timer_scripts: Vec<TimerScript>,
    #[serde(default)]
    pub warehouses: Vec<WarehouseDef>,

    /// Directory del file di configurazione: i path relativi partono da qui.
    #[serde(skip)]
    pub base_dir: PathBuf,
    /// Contenuto del progetto (parte modificabile dal Designer).
    #[serde(skip)]
    pub project: Project,
}
fn default_project_file() -> String {
    "project.yaml".into()
}
fn default_rules_dir() -> String {
    "rules".into()
}

/// Parte della configurazione modificabile dal Designer (project.yaml).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Project {
    #[serde(default)]
    pub areas: Vec<Area>,
    #[serde(default)]
    pub scan_classes: Vec<ScanClass>,
    #[serde(default)]
    pub udt_types: Vec<UdtType>,
    #[serde(default)]
    pub udt_instances: Vec<UdtInstance>,
    #[serde(default)]
    pub tags: Vec<TagDef>,
    #[serde(default)]
    pub decision_points: Vec<DecisionPointDef>,
    #[serde(default)]
    pub sort_classes: Vec<SortClass>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wms: Option<WmsConfig>,
    #[serde(default)]
    pub alarms: Vec<AlarmDef>,
    /// Script Python eseguiti periodicamente (logiche di supporto, simulazioni).
    #[serde(default)]
    pub timer_scripts: Vec<TimerScript>,
    /// Magazzini automatici (locazioni, missioni, trasli).
    #[serde(default)]
    pub warehouses: Vec<WarehouseDef>,
    /// Login, ruoli e permessi.
    #[serde(default)]
    pub security: SecurityDef,
    /// Tipi di ricetta (parametri → tag); le ricette (valori) sono nel database.
    #[serde(default)]
    pub recipe_types: Vec<RecipeType>,
    /// Sequenze di processo a passi (stile SFC).
    #[serde(default)]
    pub sequences: Vec<SequenceDef>,
}

/// Tipo di ricetta: elenco di parametri, ognuno scritto su un tag. Con `units`
/// la stessa ricetta si carica su più unità identiche: `{unit}` nei tag.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RecipeType {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub units: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<RecipeParam>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RecipeParam {
    pub name: String,
    /// Tag di destinazione (può contenere `{unit}`).
    pub tag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

/// Sequenza a passi: ogni passo esegue azioni all'ingresso e attende una
/// transizione (condizioni sui tag) verso il passo successivo.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SequenceDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    /// Unità su cui lavora (sostituisce `{unit}` in tag, valori e ricette).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    /// Avvio dal PLC: fronte di salita di questo tag.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub start_tag: String,
    /// Parametri di avvio con default (es. `recipe`), usabili come `{nome}`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    #[serde(default)]
    pub steps: Vec<StepDef>,
    /// Azioni all'arresto (stato sicuro).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_actions: Vec<SeqAction>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StepDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Azioni eseguite all'ingresso nel passo.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<SeqAction>,
    /// Transizioni valutate in ordine: la prima vera porta al passo `next`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<Transition>,
    /// Tempo minimo nel passo prima di valutare le transizioni (s).
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub min_time_s: f64,
    /// Tempo massimo nel passo (s, 0 = nessuno).
    #[serde(default, skip_serializing_if = "is_zero_f")]
    pub timeout_s: f64,
    /// Passo a cui andare in timeout (vuoto = sequenza in errore con allarme).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub on_timeout: String,
}
fn is_zero_f(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Transition {
    /// Condizioni; pseudo-tag: `@elapsed` (s nel passo), `@confirm` (conferma operatore).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub when: Vec<SeqCond>,
    /// "all" (tutte) oppure "any" (almeno una).
    #[serde(default = "all_mode", skip_serializing_if = "is_all")]
    pub mode: String,
    /// Passo successivo: nome, vuoto = il seguente in elenco, "END" = fine.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub next: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
}
fn all_mode() -> String {
    "all".into()
}
fn is_all(s: &String) -> bool {
    s == "all"
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SeqCond {
    pub tag: String,
    /// ==, !=, >, >=, <, <=
    pub op: String,
    /// Costante o "tag:Nome" (valore di un altro tag).
    pub value: Value,
}

/// Azione di un passo: `write` (tag, value), `recipe` (recipe_type, recipe),
/// `script` (fn), `log` (text).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SeqAction {
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recipe_type: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recipe: String,
    #[serde(default, rename = "fn", skip_serializing_if = "String::is_empty")]
    pub func: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

/// Sicurezza: i ruoli e i loro permessi sono nel progetto; gli utenti (con le
/// password) sono nel database del server, mai nel file di progetto.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct SecurityDef {
    /// Disattivata: nessun login, tutti possono tutto (compatibilità/demo).
    #[serde(default)]
    pub enabled: bool,
    /// Ruolo di chi non ha fatto login (vuoto = login obbligatorio).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub anonymous_role: String,
    /// Durata massima della sessione (minuti).
    #[serde(default = "session_min")]
    pub session_minutes: u32,
    /// Logout per inattività (minuti, 0 = mai).
    #[serde(default)]
    pub idle_logout_minutes: u32,
    #[serde(default = "default_roles")]
    pub roles: Vec<RoleDef>,
}
fn session_min() -> u32 {
    720
}
impl Default for SecurityDef {
    fn default() -> Self {
        Self { enabled: false, anonymous_role: String::new(), session_minutes: session_min(), idle_logout_minutes: 0, roles: default_roles() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct RoleDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Permessi (vedi `security::PERMISSIONS`); "*" = tutti.
    #[serde(default)]
    pub permissions: Vec<String>,
}

/// Ruoli predefiniti.
pub fn default_roles() -> Vec<RoleDef> {
    let r = |n: &str, d: &str, p: &[&str]| RoleDef { name: n.into(), description: d.into(), permissions: p.iter().map(|x| x.to_string()).collect() };
    vec![
        r("amministratore", "Accesso completo, gestione utenti e sicurezza", &["*"]),
        r("progettista", "Designer completo (viste, progetto, regole, script)", &["runtime.view", "tag.write", "alarm.ack", "script.call", "wh.operate", "wh.config", "designer", "designer.views", "designer.project", "rules.edit", "scripts.edit", "seq.operate", "recipe.load", "recipe.edit", "audit.view"]),
        r("manutentore", "Runtime completo e WCS, Designer in sola lettura", &["runtime.view", "tag.write", "alarm.ack", "script.call", "wh.operate", "wh.config", "designer", "seq.operate", "recipe.load", "recipe.edit", "audit.view"]),
        r("operatore", "Conduzione impianto: comandi, allarmi, missioni, sequenze, ricette", &["runtime.view", "tag.write", "alarm.ack", "script.call", "wh.operate", "seq.operate", "recipe.load"]),
        r("visualizzatore", "Sola visualizzazione", &["runtime.view"]),
    ]
}

/// Magazzino automatico: geometria, trasli per corsia, baie.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct WarehouseDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    pub aisles: Vec<AisleDef>,
    #[serde(default = "two")]
    pub sides: u16,
    pub columns: u16,
    pub levels: u16,
    /// Colonne per blocco nelle viste aggregate.
    #[serde(default = "five")]
    pub block_cols: u16,
    /// Baie d'ingresso (nomi, per riferimento).
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Baie d'uscita.
    #[serde(default)]
    pub outputs: Vec<String>,
    /// Tempi per colonna/livello (s) usati per scegliere la cella più vicina.
    #[serde(default = "col_time")]
    pub col_time_s: f64,
    #[serde(default = "lvl_time")]
    pub lvl_time_s: f64,
    /// Prefisso degli identificativi UdC generati dal riempimento iniziale.
    #[serde(default = "lu_prefix")]
    pub lu_prefix: String,
    /// Riempimento iniziale demo (%) alla prima creazione.
    #[serde(default)]
    pub initial_fill_pct: u8,
    /// Demo: crea un prelievo casuale ogni N secondi (0 = no).
    #[serde(default)]
    pub auto_outbound_s: u64,
    /// Demo: crea un ordine di prelievo casuale ogni N secondi (0 = no).
    #[serde(default)]
    pub auto_orders_s: u64,
    /// Demo: numero di articoli (SKU001..) del catalogo usato dal riempimento iniziale.
    #[serde(default)]
    pub demo_skus: u32,
    /// Altezza massima UdC per livello (mm), indice 0 = livello 1. Vuoto = nessun limite.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub level_max_height_mm: Vec<u16>,
    #[serde(default)]
    pub strategy: StrategyDef,
    #[serde(default)]
    pub reorg: ReorgDef,
}

/// Strategia di stoccaggio.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct StrategyDef {
    /// Classi ABC degli articoli per frequenza di prelievo e zone di corsia corrispondenti.
    #[serde(default = "yes")]
    pub abc: bool,
    /// Quota delle celle (più vicine al P&D) in zona A e B; il resto è zona C.
    #[serde(default = "a_zone")]
    pub a_zone_pct: f64,
    #[serde(default = "b_zone")]
    pub b_zone_pct: f64,
    /// Quota degli articoli più prelevati in classe A e B.
    #[serde(default = "a_sku")]
    pub a_sku_pct: f64,
    #[serde(default = "b_sku")]
    pub b_sku_pct: f64,
    /// Finestra (ore) per il calcolo delle frequenze di prelievo.
    #[serde(default = "abc_window")]
    pub abc_window_h: u32,
    /// Distribuisce lo stesso articolo su più corsie (riduce il rischio di fermo).
    #[serde(default = "yes")]
    pub spread_sku: bool,
}
fn a_zone() -> f64 {
    0.2
}
fn b_zone() -> f64 {
    0.3
}
fn a_sku() -> f64 {
    0.2
}
fn b_sku() -> f64 {
    0.3
}
fn abc_window() -> u32 {
    168
}
impl Default for StrategyDef {
    fn default() -> Self {
        Self { abc: true, a_zone_pct: a_zone(), b_zone_pct: b_zone(), a_sku_pct: a_sku(), b_sku_pct: b_sku(), abc_window_h: abc_window(), spread_sku: true }
    }
}

/// Riorganizzazione automatica (defrag) nei tempi morti dei trasli.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ReorgDef {
    #[serde(default)]
    pub enabled: bool,
    /// Secondi di inattività della corsia prima di avviare una mossa.
    #[serde(default = "idle_s")]
    pub idle_s: f64,
    /// Mosse massime per ora e per corsia.
    #[serde(default = "max_moves")]
    pub max_per_hour: u32,
    /// Guadagno minimo (s di tempo di viaggio) perché una mossa valga la pena.
    #[serde(default = "min_gain")]
    pub min_gain_s: f64,
}
fn idle_s() -> f64 {
    15.0
}
fn max_moves() -> u32 {
    60
}
fn min_gain() -> f64 {
    2.0
}
impl Default for ReorgDef {
    fn default() -> Self {
        Self { enabled: false, idle_s: idle_s(), max_per_hour: max_moves(), min_gain_s: min_gain() }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct AisleDef {
    pub name: String,
    /// Istanza UDT del traslo (telegramma missioni).
    pub crane: String,
    /// Posti UdC sul traslo: 1 (singola) o 2 (doppia UdC: due missioni per ciclo).
    #[serde(default = "one_slot")]
    pub slots: u8,
    /// Doppia UdC: attesa massima (s) per accoppiare una seconda missione.
    #[serde(default = "pair_wait")]
    pub pair_wait_s: f64,
}
fn one_slot() -> u8 {
    1
}
fn pair_wait() -> f64 {
    4.0
}
fn two() -> u16 {
    2
}
fn five() -> u16 {
    5
}
fn col_time() -> f64 {
    0.6
}
fn lvl_time() -> f64 {
    1.2
}
fn lu_prefix() -> String {
    "PAL".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimerScript {
    pub name: String,
    /// Funzione Python `def f(payload, ctx)`.
    pub function: String,
    pub interval_ms: u64,
    #[serde(default = "yes")]
    pub enabled: bool,
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
    /// Directory dei simboli SVG.
    #[serde(default = "default_symbols")]
    pub symbols_dir: String,
    /// Libreria standard dello SCADA (simboli, faceplate, UDT), indipendente dal
    /// progetto e in sola lettura. Le viste/simboli del progetto con lo stesso id
    /// la personalizzano. Sovrascrivibile con la variabile `PTWCS_LIBRARY`.
    #[serde(default = "default_library")]
    pub library_dir: String,
    /// Abilita il Designer (modifica viste, simboli, progetto e script dal browser).
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
fn default_library() -> String {
    "../stdlib".into()
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
            library_dir: default_library(),
            designer: true,
            retention_days: default_retention(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScriptingConfig {
    #[serde(default = "default_python")]
    pub python: String,
    /// Cartella degli script (un file per funzione) o, per compatibilità, un singolo modulo .py.
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
    S7,
    Modbus,
    Opcua,
    Eip,
    Sim,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DriverConfig {
    pub name: String,
    pub protocol: Protocol,
    #[serde(default)]
    pub host: String,
    pub port: Option<u16>,
    #[serde(default = "one")]
    pub unit_id: u8,
    #[serde(default)]
    pub rack: u16,
    #[serde(default = "one_u16")]
    pub slot: u16,
    #[serde(default)]
    pub endpoint: String,
    /// Periodo di polling di default (tag senza scan class), ms.
    #[serde(default = "default_poll")]
    pub poll_ms: u64,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Area a cui appartengono gli allarmi di comunicazione del driver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
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

// ------------------------------------------------------------------ aree, scan class

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Area {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Area padre (gerarchia), opzionale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScanClass {
    pub name: String,
    pub rate_ms: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

// ------------------------------------------------------------------ tag

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TagDef {
    pub name: String,
    pub driver: String,
    /// Indirizzo specifico del protocollo (vedi README).
    pub address: String,
    #[serde(rename = "type")]
    pub dtype: DataType,
    #[serde(default = "default_str_len", skip_serializing_if = "is_default_len")]
    pub len: usize,
    #[serde(default, skip_serializing_if = "is_false")]
    pub writable: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub historize: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<Value>,
    /// Area di appartenenza (allarmi e errori di qualità finiscono qui).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    /// Scan class (frequenza di lettura); default = poll_ms del driver.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_class: Option<String>,
    /// Allarmi definiti sul tag.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alarms: Vec<TagAlarm>,
    /// Istanza UDT da cui il tag è generato (impostato dall'espansione).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub udt: Option<String>,
    /// Ruoli autorizzati a scrivere il tag (vuoto = chi ha il permesso `tag.write`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_roles: Vec<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TagAlarm {
    /// Nome breve (l'id allarme sarà `<tag>.<name>`).
    pub name: String,
    #[serde(default = "default_op")]
    pub op: String,
    #[serde(default = "default_alarm_value")]
    pub value: Value,
    pub message: String,
    #[serde(default = "default_severity")]
    pub severity: Severity,
}

// ------------------------------------------------------------------ UDT

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Contatore/flag di richiesta scritto dal PLC.
    Request,
    /// Eco/ack di risposta scritto dal WCS.
    Response,
    /// Dato della richiesta (PLC → WCS).
    Input,
    /// Dato della risposta (WCS → PLC).
    Output,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UdtParam {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UdtMember {
    pub name: String,
    #[serde(rename = "type")]
    pub dtype: DataType,
    /// Indirizzo con parametri: `DB{DB}.DBW{Base+2}`, `hr:{Base+12}`, `ns=2;s={Instance}.Run`
    pub address: String,
    #[serde(default = "default_str_len", skip_serializing_if = "is_default_len")]
    pub len: usize,
    #[serde(default, skip_serializing_if = "is_false")]
    pub writable: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub unit: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub historize: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_class: Option<String>,
    /// Ruolo nel protocollo request/reply (telegrammi).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alarms: Vec<TagAlarm>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial: Option<Value>,
    /// Ruoli autorizzati a scrivere il membro (vuoto = quelli dell'istanza).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_roles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UdtType {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default)]
    pub params: Vec<UdtParam>,
    #[serde(default)]
    pub members: Vec<UdtMember>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UdtInstance {
    pub name: String,
    /// Nome del tipo UDT.
    #[serde(rename = "type")]
    pub udt: String,
    pub driver: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_class: Option<String>,
    #[serde(default)]
    pub params: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Ruoli autorizzati a scrivere i tag dell'istanza (vuoto = chi ha `tag.write`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_roles: Vec<String>,
}

// ------------------------------------------------------------------ protocollo request/reply

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Handshake {
    /// Il PLC incrementa ReqID (≠0); il WCS risponde con RespID = ReqID.
    #[default]
    Counter,
    /// Il PLC mette Req=1; il WCS scrive le uscite e Ack=1; il PLC rimette
    /// Req=0 e il WCS azzera Ack.
    Flag,
}

/// Punto di decisione: il PLC pone una "domanda" e il WCS risponde.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecisionPointDef {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Istanza UDT "telegramma": request/response/input/output derivati dai ruoli dei membri.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram: Option<String>,
    #[serde(default)]
    pub handshake: Handshake,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub request_tag: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub response_tag: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub outputs: BTreeMap<String, String>,
    /// Funzione Python: chiamata se nessuna regola grafica scatta.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(default = "default_decision_timeout")]
    pub timeout_ms: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fallback: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
}
fn default_decision_timeout() -> u64 {
    300
}

// ------------------------------------------------------------------ smistamento / WMS

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClassSource {
    /// Barcode/pattern definiti nel WCS.
    #[default]
    Local,
    /// Appartenenza fornita dal WMS (sincronizzata periodicamente).
    Wms,
}

/// Classe di smistamento: un insieme di barcode (espliciti o per pattern) con una destinazione.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SortClass {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Destinazione ("3", oppure "1|2" a rotazione).
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub source: ClassSource,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub barcodes: Vec<String>,
    /// Pattern con caratteri jolly: `IT-MI*`, `??-PA-*`
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<String>,
}

/// Connessione REST al WMS.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WmsConfig {
    #[serde(default = "yes")]
    pub enabled: bool,
    /// es. http://wms.local/api
    pub base_url: String,
    /// GET: destinazione di un collo. `{barcode}` viene sostituito.
    #[serde(default = "default_dest_path")]
    pub destination_path: String,
    /// Campo JSON con la destinazione nella risposta.
    #[serde(default = "default_dest_field")]
    pub destination_field: String,
    /// Campo JSON con la classe (opzionale) nella risposta.
    #[serde(default = "default_class_field")]
    pub class_field: String,
    /// GET: elenco classi [{name, destination, barcodes, patterns}] (vuoto = nessuna sync).
    #[serde(default = "default_classes_path")]
    pub classes_path: String,
    #[serde(default = "default_sync")]
    pub sync_interval_s: u64,
    #[serde(default = "default_wms_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_cache")]
    pub cache_ttl_s: u64,
    /// Header HTTP (valori `${VAR}` letti dalle variabili d'ambiente, per i token).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}
fn default_dest_path() -> String {
    "/parcels/{barcode}/destination".into()
}
fn default_dest_field() -> String {
    "destination".into()
}
fn default_class_field() -> String {
    "class".into()
}
fn default_classes_path() -> String {
    "/sort-classes".into()
}
fn default_sync() -> u64 {
    60
}
fn default_wms_timeout() -> u64 {
    250
}
fn default_cache() -> u64 {
    300
}

// ------------------------------------------------------------------ allarmi

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AlarmDef {
    pub id: String,
    pub tag: String,
    #[serde(default = "default_op")]
    pub op: String,
    #[serde(default = "default_alarm_value")]
    pub value: Value,
    pub message: String,
    #[serde(default = "default_severity")]
    pub severity: Severity,
    /// Area (default: area del tag).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Alarm,
    Critical,
}

// ------------------------------------------------------------------ template indirizzi UDT

/// Valuta un'espressione intera con + - * / % e parentesi.
fn eval_int(expr: &str, vars: &dyn Fn(&str) -> Option<Value>) -> Result<i64> {
    struct P<'a> {
        s: &'a [u8],
        i: usize,
        vars: &'a dyn Fn(&str) -> Option<Value>,
    }
    impl P<'_> {
        fn ws(&mut self) {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
        }
        fn expr(&mut self) -> Result<i64> {
            let mut v = self.term()?;
            loop {
                self.ws();
                match self.s.get(self.i) {
                    Some(b'+') => {
                        self.i += 1;
                        v += self.term()?
                    }
                    Some(b'-') => {
                        self.i += 1;
                        v -= self.term()?
                    }
                    _ => return Ok(v),
                }
            }
        }
        fn term(&mut self) -> Result<i64> {
            let mut v = self.atom()?;
            loop {
                self.ws();
                match self.s.get(self.i) {
                    Some(b'*') => {
                        self.i += 1;
                        v *= self.atom()?
                    }
                    Some(b'/') | Some(b'%') => {
                        let op = self.s[self.i];
                        self.i += 1;
                        let d = self.atom()?;
                        if d == 0 {
                            bail!("divisione per zero");
                        }
                        v = if op == b'/' { v / d } else { v % d };
                    }
                    _ => return Ok(v),
                }
            }
        }
        fn atom(&mut self) -> Result<i64> {
            self.ws();
            let start = self.i;
            match self.s.get(self.i) {
                Some(b'(') => {
                    self.i += 1;
                    let v = self.expr()?;
                    self.ws();
                    if self.s.get(self.i) != Some(&b')') {
                        bail!("manca ')'");
                    }
                    self.i += 1;
                    Ok(v)
                }
                Some(b'-') => {
                    self.i += 1;
                    Ok(-self.atom()?)
                }
                Some(c) if c.is_ascii_digit() => {
                    while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                        self.i += 1;
                    }
                    Ok(std::str::from_utf8(&self.s[start..self.i])?.parse()?)
                }
                Some(c) if c.is_ascii_alphabetic() || *c == b'_' => {
                    while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
                        self.i += 1;
                    }
                    let name = std::str::from_utf8(&self.s[start..self.i])?;
                    let v = (self.vars)(name).ok_or_else(|| anyhow!("parametro sconosciuto '{name}'"))?;
                    v.as_i64().ok_or_else(|| anyhow!("il parametro '{name}' = {v} non è un intero"))
                }
                _ => bail!("espressione non valida"),
            }
        }
    }
    let mut p = P { s: expr.as_bytes(), i: 0, vars };
    let v = p.expr()?;
    p.ws();
    if p.i != p.s.len() {
        bail!("carattere inatteso in '{expr}'");
    }
    Ok(v)
}

/// Sostituisce i `{...}` di un template: un parametro singolo viene inserito
/// così com'è (anche testo), altrimenti l'espressione è valutata come intero.
pub fn expand_template(tpl: &str, vars: &dyn Fn(&str) -> Option<Value>) -> Result<String> {
    let mut out = String::new();
    let mut rest = tpl;
    while let Some(a) = rest.find('{') {
        out.push_str(&rest[..a]);
        let b = rest[a..].find('}').ok_or_else(|| anyhow!("manca '}}' in '{tpl}'"))? + a;
        let expr = rest[a + 1..b].trim();
        let is_ident = !expr.is_empty() && expr.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
        match (is_ident, vars(expr)) {
            (true, Some(v)) => out.push_str(&v.to_string()),
            _ => out.push_str(&eval_int(expr, vars).with_context(|| format!("in '{{{expr}}}'"))?.to_string()),
        }
        rest = &rest[b + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl UdtType {
    /// Genera i tag di un'istanza.
    pub fn expand(&self, inst: &UdtInstance) -> Result<Vec<TagDef>> {
        for k in inst.params.keys() {
            if !self.params.iter().any(|p| &p.name == k) {
                bail!("istanza {}: parametro '{k}' non definito nel tipo {}", inst.name, self.name);
            }
        }
        let vars = |name: &str| -> Option<Value> {
            if name == "Instance" {
                return Some(Value::Str(inst.name.clone()));
            }
            inst.params
                .get(name)
                .cloned()
                .or_else(|| self.params.iter().find(|p| p.name == name).and_then(|p| p.default.clone()))
        };
        for p in &self.params {
            if vars(&p.name).is_none() {
                bail!("istanza {}: manca il parametro '{}' (senza default)", inst.name, p.name);
            }
        }
        self.members
            .iter()
            .map(|m| {
                let address = expand_template(&m.address, &vars)
                    .with_context(|| format!("istanza {} membro {}", inst.name, m.name))?;
                Ok(TagDef {
                    name: format!("{}.{}", inst.name, m.name),
                    driver: inst.driver.clone(),
                    address,
                    dtype: m.dtype,
                    len: m.len,
                    writable: m.writable,
                    description: if m.description.is_empty() { inst.description.clone() } else { m.description.clone() },
                    unit: m.unit.clone(),
                    historize: m.historize,
                    initial: m.initial.clone(),
                    area: inst.area.clone(),
                    scan_class: m.scan_class.clone().or_else(|| inst.scan_class.clone()),
                    alarms: m
                        .alarms
                        .iter()
                        .map(|a| {
                            let mut a = a.clone();
                            a.message = expand_template(&a.message, &vars).unwrap_or(a.message.clone());
                            a
                        })
                        .collect(),
                    udt: Some(inst.name.clone()),
                    write_roles: if m.write_roles.is_empty() { inst.write_roles.clone() } else { m.write_roles.clone() },
                })
            })
            .collect()
    }
}

// ------------------------------------------------------------------ caricamento e validazione

impl Config {
    /// Carica il file di impianto e il progetto, e costruisce la configurazione effettiva.
    pub fn load(path: &Path) -> Result<Self> {
        let base = Self::load_base(path)?;
        let pf = base.resolve(&base.project_file);
        let project: Project = if pf.exists() {
            let text = std::fs::read_to_string(&pf).with_context(|| format!("lettura {}", pf.display()))?;
            if text.trim().is_empty() {
                Project::default()
            } else {
                serde_yaml::from_str(&text).with_context(|| format!("parsing {}", pf.display()))?
            }
        } else {
            Project::default()
        };
        base.with_project(project)
    }

    /// Solo il file di impianto (senza progetto).
    pub fn load_base(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("lettura config {}", path.display()))?;
        let mut cfg: Config =
            serde_yaml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))?;
        cfg.base_dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        Ok(cfg)
    }

    /// Configurazione effettiva = impianto + progetto (UDT espanse, telegrammi risolti), validata.
    pub fn with_project(&self, project: Project) -> Result<Config> {
        let mut c = self.clone();
        c.areas.extend(project.areas.clone());
        c.scan_classes.extend(project.scan_classes.clone());
        c.udt_types.extend(project.udt_types.clone());
        c.udt_instances.extend(project.udt_instances.clone());
        c.tags.extend(project.tags.clone());
        c.decision_points.extend(project.decision_points.clone());
        c.sort_classes.extend(project.sort_classes.clone());
        if project.wms.is_some() {
            c.wms = project.wms.clone();
        }
        c.alarms.extend(project.alarms.clone());
        c.timer_scripts.extend(project.timer_scripts.clone());
        c.warehouses.extend(project.warehouses.clone());
        // espansione UDT
        for inst in &c.udt_instances {
            let t = c
                .udt_types
                .iter()
                .find(|t| t.name == inst.udt)
                .ok_or_else(|| anyhow!("istanza {}: tipo UDT '{}' sconosciuto", inst.name, inst.udt))?;
            let generated = t.expand(inst)?;
            c.tags.extend(generated);
        }
        // risoluzione telegrammi
        let types = c.udt_types.clone();
        let insts = c.udt_instances.clone();
        for dp in &mut c.decision_points {
            let Some(tel) = dp.telegram.clone() else { continue };
            let inst = insts
                .iter()
                .find(|i| i.name == tel)
                .ok_or_else(|| anyhow!("punto {}: istanza telegramma '{tel}' sconosciuta", dp.name))?;
            let ty = types.iter().find(|t| t.name == inst.udt).unwrap();
            dp.inputs.clear();
            dp.outputs.clear();
            dp.request_tag.clear();
            dp.response_tag.clear();
            for m in &ty.members {
                let tag = format!("{}.{}", inst.name, m.name);
                let alias = m.name.to_lowercase();
                match m.role {
                    Some(Role::Request) => dp.request_tag = tag,
                    Some(Role::Response) => dp.response_tag = tag,
                    Some(Role::Input) => {
                        dp.inputs.insert(alias, tag);
                    }
                    Some(Role::Output) => {
                        dp.outputs.insert(alias, tag);
                    }
                    None => {}
                }
            }
            if dp.request_tag.is_empty() || dp.response_tag.is_empty() {
                bail!("punto {}: il tipo UDT '{}' deve avere un membro con ruolo request e uno con ruolo response", dp.name, ty.name);
            }
            if dp.area.is_none() {
                dp.area = inst.area.clone();
            }
        }
        c.project = project;
        c.validate()?;
        Ok(c)
    }

    pub fn resolve(&self, rel: &str) -> PathBuf {
        let p = Path::new(rel);
        if p.is_absolute() { p.to_path_buf() } else { self.base_dir.join(p) }
    }

    pub fn security(&self) -> SecurityDef {
        self.project.security.clone()
    }

    pub fn tag(&self, name: &str) -> Option<&TagDef> {
        self.tags.iter().find(|t| t.name == name)
    }

    /// Salva il progetto (scrittura atomica).
    pub fn save_project(&self, p: &Project) -> Result<()> {
        let path = self.resolve(&self.project_file);
        let tmp = path.with_extension("yaml.tmp");
        let header = "# Progetto WCS-SCADA – gestito dal Designer (modificabile a mano a WCS fermo).\n";
        std::fs::write(&tmp, format!("{header}{}", serde_yaml::to_string(p)?))?;
        if path.exists() {
            let _ = std::fs::copy(&path, path.with_extension("yaml.bak"));
        }
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    /// Controlli di coerenza: gli errori emergono all'avvio o al salvataggio, non in produzione.
    pub fn validate(&self) -> Result<()> {
        let uniq = |what: &str, names: Vec<&str>| -> Result<HashSet<String>> {
            let mut s = HashSet::new();
            for n in names {
                if n.trim().is_empty() {
                    bail!("{what}: nome vuoto");
                }
                if !s.insert(n.to_string()) {
                    bail!("{what} duplicato: {n}");
                }
            }
            Ok(s)
        };
        // sicurezza: ruoli, permessi e riferimenti ai ruoli
        let sec = &self.project.security;
        let roles = uniq("ruolo", sec.roles.iter().map(|r| r.name.as_str()).collect())?;
        for r in &sec.roles {
            for p in &r.permissions {
                let known = p == "*" || crate::security::PERMISSIONS.iter().any(|(id, _, _)| id == p || (p.ends_with(".*") && id.starts_with(&p[..p.len() - 1])));
                if !known {
                    bail!("ruolo {}: permesso sconosciuto '{p}'", r.name);
                }
            }
        }
        if !sec.anonymous_role.is_empty() && !roles.contains(&sec.anonymous_role) {
            bail!("sicurezza: ruolo anonimo '{}' non definito", sec.anonymous_role);
        }
        if sec.enabled && !sec.roles.iter().any(|r| r.permissions.iter().any(|p| p == "*" || p == "users.manage")) {
            bail!("sicurezza: serve almeno un ruolo con il permesso users.manage (o *), altrimenti nessuno potrebbe più gestirla");
        }
        for (what, name, wr) in self.project.tags.iter().map(|t| ("tag", t.name.as_str(), &t.write_roles))
            .chain(self.project.udt_instances.iter().map(|i| ("istanza", i.name.as_str(), &i.write_roles)))
            .chain(self.project.udt_types.iter().flat_map(|t| t.members.iter().map(move |m| ("membro UDT", m.name.as_str(), &m.write_roles))))
        {
            if let Some(r) = wr.iter().find(|r| !roles.contains(*r)) {
                bail!("{what} {name}: ruolo di scrittura '{r}' non definito");
            }
        }
        // ricette e sequenze
        let rtypes = uniq("tipo di ricetta", self.project.recipe_types.iter().map(|r| r.name.as_str()).collect())?;
        for r in &self.project.recipe_types {
            uniq(&format!("ricetta {}: parametro", r.name), r.parameters.iter().map(|p| p.name.as_str()).collect())?;
            for p in &r.parameters {
                if p.tag.trim().is_empty() {
                    bail!("ricetta {}: parametro {} senza tag", r.name, p.name);
                }
                if let (Some(a), Some(b)) = (p.min, p.max)
                    && a > b
                {
                    bail!("ricetta {}: parametro {}: min > max", r.name, p.name);
                }
            }
        }
        uniq("sequenza", self.project.sequences.iter().map(|s| s.name.as_str()).collect())?;
        for sq in &self.project.sequences {
            let steps = uniq(&format!("sequenza {}: passo", sq.name), sq.steps.iter().map(|s| s.name.as_str()).collect())?;
            for st in &sq.steps {
                for t in &st.transitions {
                    if !t.next.is_empty() && t.next != "END" && !steps.contains(&t.next) {
                        bail!("sequenza {} passo {}: transizione verso passo inesistente '{}'", sq.name, st.name, t.next);
                    }
                    for c in &t.when {
                        if !["==", "!=", ">", ">=", "<", "<="].contains(&c.op.as_str()) {
                            bail!("sequenza {} passo {}: operatore non valido '{}'", sq.name, st.name, c.op);
                        }
                    }
                }
                if !st.on_timeout.is_empty() && st.on_timeout != "END" && !steps.contains(&st.on_timeout) {
                    bail!("sequenza {} passo {}: passo di timeout inesistente '{}'", sq.name, st.name, st.on_timeout);
                }
                for a in st.actions.iter().chain(sq.stop_actions.iter()) {
                    match a.kind.as_str() {
                        "write" if a.tag.is_empty() => bail!("sequenza {} passo {}: azione write senza tag", sq.name, st.name),
                        "recipe" if !rtypes.contains(&a.recipe_type) => bail!("sequenza {} passo {}: tipo di ricetta '{}' inesistente", sq.name, st.name, a.recipe_type),
                        "write" | "recipe" | "script" | "log" => {}
                        k => bail!("sequenza {} passo {}: azione sconosciuta '{k}'", sq.name, st.name),
                    }
                }
            }
        }
        let drivers = uniq("driver", self.drivers.iter().map(|d| d.name.as_str()).collect())?;
        let areas = uniq("area", self.areas.iter().map(|a| a.name.as_str()).collect())?;
        let scans = uniq("scan class", self.scan_classes.iter().map(|a| a.name.as_str()).collect())?;
        uniq("tipo UDT", self.udt_types.iter().map(|a| a.name.as_str()).collect())?;
        uniq("istanza UDT", self.udt_instances.iter().map(|a| a.name.as_str()).collect())?;
        uniq("classe di smistamento", self.sort_classes.iter().map(|a| a.name.as_str()).collect())?;
        uniq("punto di decisione", self.decision_points.iter().map(|a| a.name.as_str()).collect())?;
        uniq("timer script", self.timer_scripts.iter().map(|a| a.name.as_str()).collect())?;
        uniq("magazzino", self.warehouses.iter().map(|a| a.name.as_str()).collect())?;
        for w in &self.warehouses {
            if w.aisles.is_empty() || w.columns == 0 || w.levels == 0 || w.sides == 0 {
                bail!("magazzino {}: geometria non valida", w.name);
            }
            if w.outputs.is_empty() {
                bail!("magazzino {}: definire almeno una baia d'uscita", w.name);
            }
            for a in &w.aisles {
                if !(1..=2).contains(&a.slots) {
                    bail!("magazzino {} corsia {}: slots deve essere 1 o 2", w.name, a.name);
                }
                let base = ["Mission", "Type", "SrcSide", "SrcCol", "SrcLvl", "DstSide", "DstCol", "DstLvl", "DstOut", "Done"];
                let mut need: Vec<String> = base.iter().map(|m| m.to_string()).collect();
                need.extend(["Start", "Busy", "Fault"].map(String::from));
                if a.slots == 2 {
                    need.extend(base.iter().map(|m| format!("{m}2")));
                }
                for m in need {
                    let t = format!("{}.{m}", a.crane);
                    if !self.tags.iter().any(|x| x.name == t) {
                        bail!("magazzino {} corsia {}: manca il tag {t} (telegramma missioni del traslo{})", w.name, a.name,
                              if a.slots == 2 { " a doppia UdC" } else { "" });
                    }
                }
            }
        }
        for t in &self.timer_scripts {
            if t.interval_ms < 50 {
                bail!("timer script {}: intervallo minimo 50 ms", t.name);
            }
            if self.scripting.module.is_none() {
                bail!("timer script {}: manca scripting.module", t.name);
            }
        }
        for a in &self.areas {
            if let Some(p) = &a.parent
                && !areas.contains(p) {
                    bail!("area {}: area padre sconosciuta '{p}'", a.name);
                }
        }
        for s in &self.scan_classes {
            if s.rate_ms < 10 {
                bail!("scan class {}: periodo minimo 10 ms", s.name);
            }
        }
        let need_area = |ctx: &str, a: &Option<String>| -> Result<()> {
            if let Some(a) = a
                && !areas.contains(a) {
                    bail!("{ctx}: area sconosciuta '{a}'");
                }
            Ok(())
        };
        for d in &self.drivers {
            need_area(&format!("driver {}", d.name), &d.area)?;
        }
        for t in &self.udt_types {
            uniq(&format!("membro di {}", t.name), t.members.iter().map(|m| m.name.as_str()).collect())?;
            uniq(&format!("parametro di {}", t.name), t.params.iter().map(|m| m.name.as_str()).collect())?;
        }
        let mut tags = HashSet::new();
        for t in &self.tags {
            if t.name.trim().is_empty() || t.name.chars().any(|c| c.is_whitespace() || c == '\'' || c == '"') {
                bail!("nome tag non valido: '{}' (niente spazi o apici)", t.name);
            }
            if !tags.insert(t.name.as_str()) {
                bail!("tag duplicato: {}", t.name);
            }
            if !drivers.contains(&t.driver) {
                bail!("tag {}: driver sconosciuto '{}'", t.name, t.driver);
            }
            need_area(&format!("tag {}", t.name), &t.area)?;
            if let Some(s) = &t.scan_class
                && !scans.contains(s) {
                    bail!("tag {}: scan class sconosciuta '{s}'", t.name);
                }
            for a in &t.alarms {
                check_op(&format!("allarme {}.{}", t.name, a.name), &a.op)?;
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
            let ctx = format!("punto di decisione {}", dp.name);
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
                    bail!("{ctx}: fallback '{k}' non è un'uscita");
                }
            }
            need_area(&ctx, &dp.area)?;
        }
        if self.decision_points.iter().any(|d| d.function.is_some()) && self.scripting.module.is_none() {
            bail!("ci sono punti di decisione con 'function' ma manca scripting.module");
        }
        for a in &self.alarms {
            need_tag(&format!("allarme {}", a.id), &a.tag)?;
            check_op(&format!("allarme {}", a.id), &a.op)?;
            need_area(&format!("allarme {}", a.id), &a.area)?;
        }
        if let Some(w) = &self.wms
            && w.enabled && !(w.base_url.starts_with("http://") || w.base_url.starts_with("https://")) {
                bail!("WMS: base_url deve iniziare con http:// o https://");
            }
        Ok(())
    }
}

fn check_op(ctx: &str, op: &str) -> Result<()> {
    if !["==", "!=", ">", ">=", "<", "<="].contains(&op) {
        bail!("{ctx}: operatore non valido '{op}'");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates() {
        let vars = |n: &str| match n {
            "DB" => Some(Value::Int(100)),
            "Base" => Some(Value::Int(20)),
            "Name" => Some(Value::Str("Motore_1".into())),
            _ => None,
        };
        assert_eq!(expand_template("DB{DB}.DBW{Base+2}", &vars).unwrap(), "DB100.DBW22");
        assert_eq!(expand_template("hr:{(Base + 3) * 2 - 1}", &vars).unwrap(), "hr:45");
        assert_eq!(expand_template("ns=2;s={Name}.Run", &vars).unwrap(), "ns=2;s=Motore_1.Run");
        assert!(expand_template("hr:{Nope+1}", &vars).is_err());
        assert!(expand_template("hr:{Base", &vars).is_err());
        assert!(expand_template("hr:{Name+1}", &vars).is_err());
    }

    #[test]
    fn udt_expand() {
        let t: UdtType = serde_yaml::from_str(
            "{name: Nastro, params: [{name: Idx}, {name: Off, default: 100}],
              members: [{name: Run, type: bool, address: 'coil:{Idx}', writable: true},
                        {name: Speed, type: u16, address: 'ir:{Off+Idx*2}'}]}",
        )
        .unwrap();
        let i: UdtInstance = serde_yaml::from_str("{name: CV7, type: Nastro, driver: P, area: A, params: {Idx: 3}}").unwrap();
        let tags = t.expand(&i).unwrap();
        assert_eq!(tags[0].name, "CV7.Run");
        assert_eq!(tags[0].address, "coil:3");
        assert_eq!(tags[1].address, "ir:106");
        assert_eq!(tags[1].area.as_deref(), Some("A"));
        let bad: UdtInstance = serde_yaml::from_str("{name: X, type: Nastro, driver: P}").unwrap();
        assert!(t.expand(&bad).is_err());
    }
}
