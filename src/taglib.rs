//! Libreria dei tag in formato testo (YAML): tipi UDT, istanze e tag, esportabili e
//! reimportabili tra progetti o nella libreria standard dello SCADA.
//!
//! ```yaml
//! format: ptwcs-tag-library
//! version: 1
//! name: Libreria motori
//! udt_types:
//!   - name: Motore
//!     params: [ { name: DB, default: 100 }, { name: Base, default: 0 } ]
//!     members:
//!       - { name: Run, type: bool, address: "DB{DB}.DBX{Base+0}.0" }
//! udt_instances: [ ... ]   # facoltativo
//! tags: [ ... ]            # facoltativo
//! ```
//! In lettura si accettano anche JSON, un elenco di tipi UDT o un singolo tipo.

use crate::config::{TagDef, UdtInstance, UdtType};
use crate::value::Value;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

pub const FORMAT: &str = "ptwcs-tag-library";

fn format() -> String {
    FORMAT.into()
}
fn one() -> u32 {
    1
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TagLibrary {
    #[serde(default = "format")]
    pub format: String,
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exported: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub udt_types: Vec<UdtType>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub udt_instances: Vec<UdtInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<TagDef>,
}

impl Default for TagLibrary {
    fn default() -> Self {
        TagLibrary { format: format(), version: 1, name: String::new(), description: String::new(), exported: String::new(), udt_types: vec![], udt_instances: vec![], tags: vec![] }
    }
}

/// Testo YAML della libreria, con un'intestazione di commento.
pub fn to_text(lib: &TagLibrary) -> Result<String> {
    let mut l = lib.clone();
    l.format = FORMAT.into();
    l.version = 1;
    let body = serde_yaml::to_string(&l)?;
    Ok(format!(
        "# Libreria tag PTWCS{}: {} tipi UDT, {} istanze, {} tag\n# Modificabile con un editor di testo e reimportabile dal Designer (Tag e UDT).\n{body}",
        if l.name.is_empty() { String::new() } else { format!(" \"{}\"", l.name) },
        l.udt_types.len(),
        l.udt_instances.len(),
        l.tags.len()
    ))
}

/// Legge una libreria da testo YAML/JSON e la valida.
pub fn parse(text: &str) -> Result<TagLibrary> {
    let v: serde_yaml::Value = serde_yaml::from_str(text).context("testo non valido (YAML o JSON)")?;
    let lib = match &v {
        serde_yaml::Value::Sequence(_) => TagLibrary { udt_types: serde_yaml::from_value(v).context("elenco di tipi UDT")?, ..Default::default() },
        serde_yaml::Value::Mapping(m) if m.contains_key("members") => {
            TagLibrary { udt_types: vec![serde_yaml::from_value(v).context("tipo UDT")?], ..Default::default() }
        }
        serde_yaml::Value::Mapping(_) => serde_yaml::from_value::<TagLibrary>(v).context("libreria")?,
        _ => bail!("il testo non contiene una libreria di tag"),
    };
    if lib.format != FORMAT {
        bail!("formato '{}' non riconosciuto (atteso {FORMAT})", lib.format);
    }
    validate(&lib)?;
    Ok(lib)
}

/// Controlli di coerenza: nomi univoci, indirizzi espandibili, istanze di tipi noti.
pub fn validate(lib: &TagLibrary) -> Result<()> {
    let mut names = HashSet::new();
    for t in &lib.udt_types {
        if t.name.trim().is_empty() {
            bail!("tipo UDT senza nome");
        }
        if !names.insert(t.name.as_str()) {
            bail!("tipo UDT '{}' ripetuto", t.name);
        }
        check_type(t)?;
    }
    let mut inst = HashSet::new();
    for i in &lib.udt_instances {
        if !inst.insert(i.name.as_str()) {
            bail!("istanza '{}' ripetuta", i.name);
        }
    }
    let mut tags = HashSet::new();
    for t in &lib.tags {
        if t.name.trim().is_empty() || !tags.insert(t.name.as_str()) {
            bail!("tag '{}' senza nome o ripetuto", t.name);
        }
    }
    Ok(())
}

/// Verifica un tipo UDT espandendolo con i parametri di default (0 se mancano).
pub fn check_type(t: &UdtType) -> Result<()> {
    let mut members = HashSet::new();
    for m in &t.members {
        if m.name.trim().is_empty() || !members.insert(m.name.as_str()) {
            bail!("tipo {}: membro '{}' senza nome o ripetuto", t.name, m.name);
        }
    }
    let mut ps = HashSet::new();
    for p in &t.params {
        if !ps.insert(p.name.as_str()) {
            bail!("tipo {}: parametro '{}' ripetuto", t.name, p.name);
        }
    }
    let params: BTreeMap<String, Value> = t.params.iter().filter(|p| p.default.is_none()).map(|p| (p.name.clone(), Value::Int(0))).collect();
    let probe = UdtInstance {
        name: "Prova".into(),
        udt: t.name.clone(),
        driver: String::new(),
        area: None,
        scan_class: None,
        params,
        description: String::new(),
        write_roles: vec![],
    };
    t.expand(&probe).with_context(|| format!("tipo {}", t.name))?;
    Ok(())
}

/// Tipi UDT della libreria standard (`udts.json`).
pub fn read_std(dir: &Path) -> Result<Vec<UdtType>> {
    let p = dir.join("udts.json");
    if !p.exists() {
        return Ok(vec![]);
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(&p)?).with_context(|| format!("lettura {}", p.display()))?)
}

/// Salva i tipi UDT della libreria standard; la versione precedente resta in `udts.json.bak`.
pub fn write_std(dir: &Path, types: &[UdtType]) -> Result<()> {
    validate(&TagLibrary { udt_types: types.to_vec(), ..Default::default() })?;
    std::fs::create_dir_all(dir)?;
    let p = dir.join("udts.json");
    if p.exists() {
        std::fs::copy(&p, dir.join("udts.json.bak")).context("copia di sicurezza della libreria")?;
    }
    let tmp = dir.join("udts.json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(types)? + "\n")?;
    std::fs::rename(&tmp, &p)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TXT: &str = r#"
format: ptwcs-tag-library
name: Prova
udt_types:
  - name: Motore
    params: [ { name: DB, default: 100 }, { name: Base } ]
    members:
      - { name: Run, type: bool, address: "DB{DB}.DBX{Base+0}.0", writable: true }
      - { name: Speed, type: f32, address: "DB{DB}.DBD{Base+2}", unit: "m/s" }
udt_instances:
  - { name: M1, type: Motore, driver: SIM, params: { Base: 10 } }
tags:
  - { name: Start, driver: SIM, address: a, type: bool }
"#;

    #[test]
    fn roundtrip() {
        let l = parse(TXT).unwrap();
        assert_eq!(l.udt_types.len(), 1);
        assert_eq!(l.udt_types[0].members.len(), 2);
        assert_eq!(l.udt_instances[0].udt, "Motore");
        let text = to_text(&l).unwrap();
        assert!(text.starts_with("# Libreria tag PTWCS \"Prova\""), "{text}");
        let back = parse(&text).unwrap();
        assert_eq!(back.udt_types[0].members[1].unit, "m/s");
        assert_eq!(back.tags.len(), 1);
        // JSON e singolo tipo
        let j = serde_json::to_string(&l.udt_types).unwrap();
        assert_eq!(parse(&j).unwrap().udt_types.len(), 1);
        let one = serde_yaml::to_string(&l.udt_types[0]).unwrap();
        assert_eq!(parse(&one).unwrap().udt_types[0].name, "Motore");
    }

    #[test]
    fn errors() {
        assert!(parse("format: altro\n").is_err());
        assert!(parse("ciao").is_err());
        let dup = TXT.replace("name: Speed", "name: Run");
        assert!(format!("{:#}", parse(&dup).unwrap_err()).contains("ripetuto"));
        let bad = TXT.replace("{Base+2}", "{Base+}");
        assert!(parse(&bad).is_err());
    }
}
