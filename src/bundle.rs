//! File di progetto (`.ptwcs`): tutto ciò che il Designer configura, in un solo file JSON.
//!
//! Contiene `project.yaml` (tag, UDT, aree, punti di decisione, magazzini, fasi, fisica,
//! sicurezza…), le viste, le regole di smistamento, gli script Python, i simboli SVG del
//! progetto e le ricette. Non contiene utenti e password (sono nel database del server),
//! né il file di impianto `plant.yaml` (driver, porte, percorsi), che resta della macchina.
//!
//! Operazioni: esporta, importa (sostituisce il progetto), nuovo progetto (vuoto), backup
//! automatici prima di ogni sostituzione e ripristino.

use crate::app::AppState;
use crate::config::Project;
use crate::recipes::RecipeDef;
use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FORMAT: &str = "ptwcs-project";
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub exported: i64,
    #[serde(default)]
    pub exported_by: String,
    pub project: Project,
    /// File di testo del progetto: `views/<id>.json`, `rules/<punto>.json`, `scripts/<nome>.py`, `symbols/<id>.svg`.
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default)]
    pub recipes: Vec<RecipeDef>,
}

/// Cartelle del progetto sul server: (prefisso nel file, cartella, estensione).
fn dirs(app: &AppState) -> Vec<(&'static str, PathBuf, &'static str)> {
    let cfg = app.cfg();
    let mut v = vec![("views", app.views_dir.clone(), "json"), ("rules", cfg.resolve(&cfg.rules_dir), "json"), ("symbols", app.symbols_dir.clone(), "svg")];
    if let Some(p) = &app.script_path
        && p.is_dir()
    {
        v.push(("scripts", p.clone(), "py"));
    }
    v
}

fn safe_name(n: &str) -> bool {
    !n.is_empty() && n.len() <= 120 && !n.starts_with('.') && n.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// Esporta il progetto corrente.
pub fn export(app: &AppState, name: &str, user: &str) -> Result<Bundle> {
    let cfg = app.cfg();
    let mut files = BTreeMap::new();
    for (prefix, dir, ext) in dirs(app) {
        for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_file() && p.extension().is_some_and(|x| x == ext) {
                let fname = p.file_name().unwrap().to_string_lossy().to_string();
                if safe_name(&fname) {
                    files.insert(format!("{prefix}/{fname}"), std::fs::read_to_string(&p).with_context(|| format!("lettura {}", p.display()))?);
                }
            }
        }
    }
    Ok(Bundle {
        format: FORMAT.into(),
        version: VERSION,
        name: if name.trim().is_empty() { cfg.plant.name.clone() } else { name.trim().to_string() },
        description: cfg.plant.description.clone(),
        exported: crate::tagdb::now_ms(),
        exported_by: user.into(),
        project: cfg.project.clone(),
        files,
        recipes: app.recipes.list(),
    })
}

/// Esito di un'importazione.
#[derive(Debug, Default, Serialize)]
pub struct Imported {
    pub name: String,
    pub backup: String,
    pub files: usize,
    pub tags: usize,
    pub views: usize,
    pub recipes: usize,
    pub warnings: Vec<String>,
}

/// Sostituisce il progetto con quello del file. Se `keep_security`, ruoli e impostazioni di
/// sicurezza restano quelli attuali (chi importa senza `users.manage` non può cambiarli).
pub async fn import(app: &AppState, mut b: Bundle, keep_security: bool, user: &str, reason: &str) -> Result<Imported> {
    if b.format != FORMAT {
        bail!("non è un file di progetto PTWCS (formato '{}')", b.format);
    }
    if b.version > VERSION {
        bail!("file di progetto della versione {} (questo server legge fino alla {VERSION}): aggiorna il server", b.version);
    }
    let roots = dirs(app);
    for k in b.files.keys() {
        let (prefix, name) = k.split_once('/').ok_or_else(|| anyhow!("percorso non valido nel file: {k}"))?;
        let Some((_, _, ext)) = roots.iter().find(|(p, _, _)| *p == prefix) else {
            if prefix == "scripts" {
                bail!("il file contiene script, ma il server non ha una cartella di script (scripting.module in plant.yaml)");
            }
            bail!("cartella sconosciuta nel file: {prefix}");
        };
        if !safe_name(name) || !name.ends_with(&format!(".{ext}")) {
            bail!("nome di file non valido: {k}");
        }
    }
    if keep_security {
        b.project.security = app.cfg().project.security.clone();
    }
    // validazione completa prima di toccare qualsiasi cosa
    let cand = app.check_project(b.project.clone()).context("il progetto del file non è valido per questo impianto")?;
    let backup = save_backup(app, reason, user)?;

    let _g = app.edit_lock.lock().await;
    for (prefix, dir, ext) in &roots {
        std::fs::create_dir_all(dir)?;
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_file() && p.extension().is_some_and(|x| x == *ext) {
                std::fs::remove_file(&p).with_context(|| format!("rimozione {}", p.display()))?;
            }
        }
        for (k, text) in b.files.iter().filter(|(k, _)| k.starts_with(&format!("{prefix}/"))) {
            let name = &k[prefix.len() + 1..];
            std::fs::write(dir.join(name), text).with_context(|| format!("scrittura {k}"))?;
        }
    }
    app.apply_project(b.project.clone())?;
    let cfg = app.cfg();
    let mut out = Imported {
        name: b.name.clone(),
        backup,
        files: b.files.len(),
        tags: cand.tags.len(),
        views: b.files.keys().filter(|k| k.starts_with("views/")).count(),
        ..Default::default()
    };
    if let Err(e) = app.rules.reload(&cfg.decision_points) {
        out.warnings.push(format!("regole: {e:#}"));
    }
    if let Err(e) = app.reload_scripts().await {
        out.warnings.push(format!("script: {e:#}"));
    }
    // ricette: sostituite con quelle del file
    for r in app.recipes.list() {
        let _ = app.recipes.delete(&r.name);
    }
    for r in b.recipes {
        let name = r.name.clone();
        match app.recipes.save(&cfg, r, user) {
            Ok(_) => out.recipes += 1,
            Err(e) => out.warnings.push(format!("ricetta {name}: {e:#}")),
        }
    }
    app.audit_ext("designer", "project_import", &out.name, None, None, format!("{reason}: {} file, {} tag, {} ricette; backup {}", out.files, out.tags, out.recipes, out.backup));
    Ok(out)
}

/// Progetto vuoto: tiene solo la sicurezza (ruoli) per non chiudere fuori nessuno.
pub fn empty(app: &AppState, name: &str) -> Bundle {
    let cfg = app.cfg();
    let project = Project { security: cfg.project.security.clone(), ..Default::default() };
    let mut files = BTreeMap::new();
    let view = serde_json::json!({"title": "Pagina iniziale", "width": 1600, "height": 900, "background": "#0b1220", "home": true,
        "components": [{"id": "title", "name": "title", "type": "label", "x": 40, "y": 30, "w": 900, "h": 40, "rotation": 0,
            "props": {"text": name, "fontSize": 28, "bold": true}, "bindings": {}, "events": {}}]});
    files.insert("views/home.json".into(), serde_json::to_string_pretty(&view).unwrap_or_default());
    Bundle { format: FORMAT.into(), version: VERSION, name: name.into(), description: String::new(), exported: crate::tagdb::now_ms(), exported_by: String::new(), project, files, recipes: vec![] }
}

fn backups_dir(app: &AppState) -> PathBuf {
    let cfg = app.cfg();
    cfg.resolve("backups")
}

/// Salva un backup del progetto corrente; ritorna il nome del file.
pub fn save_backup(app: &AppState, reason: &str, user: &str) -> Result<String> {
    let dir = backups_dir(app);
    std::fs::create_dir_all(&dir)?;
    let b = export(app, "", user)?;
    let tag: String = reason.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let name = format!("{}-{}.ptwcs", chrono::Local::now().format("%Y%m%d-%H%M%S"), tag.trim_matches('-'));
    std::fs::write(dir.join(&name), serde_json::to_string(&b)?)?;
    prune(&dir, 30);
    Ok(name)
}

/// Tiene solo gli ultimi `keep` backup.
fn prune(dir: &Path, keep: usize) {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "ptwcs")).collect();
    v.sort();
    while v.len() > keep {
        let _ = std::fs::remove_file(v.remove(0));
    }
}

#[derive(Debug, Serialize)]
pub struct BackupInfo {
    pub file: String,
    pub size: u64,
    pub name: String,
    pub exported: i64,
    pub exported_by: String,
}

pub fn list_backups(app: &AppState) -> Vec<BackupInfo> {
    let dir = backups_dir(app);
    let mut v: Vec<BackupInfo> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "ptwcs"))
        .map(|e| {
            let p = e.path();
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            // intestazione senza caricare tutto il progetto in un tipo
            let head: serde_json::Value = std::fs::read_to_string(&p).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            BackupInfo {
                file: p.file_name().unwrap().to_string_lossy().to_string(),
                size,
                name: head.get("name").and_then(|v| v.as_str()).unwrap_or("").into(),
                exported: head.get("exported").and_then(|v| v.as_i64()).unwrap_or(0),
                exported_by: head.get("exported_by").and_then(|v| v.as_str()).unwrap_or("").into(),
            }
        })
        .collect();
    v.sort_by(|a, b| b.file.cmp(&a.file));
    v
}

pub fn read_backup(app: &AppState, file: &str) -> Result<Bundle> {
    if !safe_name(file) || !file.ends_with(".ptwcs") {
        bail!("nome di backup non valido");
    }
    let p = backups_dir(app).join(file);
    let text = std::fs::read_to_string(&p).with_context(|| format!("backup {file} non trovato"))?;
    Ok(serde_json::from_str(&text)?)
}
