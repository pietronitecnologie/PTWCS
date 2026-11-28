//! Gestione dei progetti dal client: login al server, nuovo progetto, apri da file (.ptwcs),
//! salva su file, backup e ripristino. Le chiamate HTTP girano in thread separati; i dialoghi
//! di file sono quelli nativi del sistema operativo.

use serde_json::{Value as J, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Cookie di sessione per server (solo in memoria).
pub type Sessions = Arc<Mutex<HashMap<String, String>>>;

fn agent(timeout_s: u64) -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(timeout_s)).build()
}

fn err_text(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => {
            let body = r.into_string().unwrap_or_default();
            let msg = serde_json::from_str::<J>(&body).ok().and_then(|j| j.get("error").and_then(|v| v.as_str()).map(str::to_string)).unwrap_or(body);
            match code {
                401 => "accesso richiesto: effettua il login".into(),
                403 => format!("permesso negato: {msg}"),
                _ => format!("HTTP {code}: {msg}"),
            }
        }
        e => e.kind().to_string(),
    }
}

fn req(s: &Sessions, url: &str, method: &str, path: &str, timeout: u64) -> ureq::Request {
    let r = agent(timeout).request(method, &format!("{url}{path}"));
    match s.lock().unwrap().get(url) {
        Some(c) => r.set("Cookie", c),
        None => r,
    }
}

/// Stato di accesso: sicurezza attiva, utente, permessi per i progetti.
pub fn auth(s: &Sessions, url: &str) -> J {
    match req(s, url, "GET", "/api/auth/me", 4).call() {
        Ok(r) => {
            let me: J = r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            let all = me.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
            let perms: Vec<String> = me.get("permissions").and_then(|v| v.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let can = |p: &str| all || perms.iter().any(|x| x == p || x == "*");
            json!({"ok": true, "enabled": me.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false),
                "user": me.get("user").and_then(|v| v.as_str()).unwrap_or(""),
                "designer": can("designer"), "project": can("designer.project"), "security": can("users.manage")})
        }
        Err(e) => json!({"ok": false, "error": err_text(e)}),
    }
}

pub fn login(s: &Sessions, url: &str, user: &str, pass: &str) -> J {
    match agent(6).post(&format!("{url}/api/auth/login")).set("Content-Type", "application/json").send_string(&json!({"username": user, "password": pass}).to_string()) {
        Ok(r) => {
            if let Some(c) = r.header("set-cookie").and_then(|c| c.split(';').next()) {
                s.lock().unwrap().insert(url.to_string(), c.to_string());
            }
            let body: J = r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
            if body.get("must_change").and_then(|v| v.as_bool()) == Some(true) {
                return json!({"ok": false, "error": "password da cambiare: accedi una volta dal runtime per impostarne una nuova"});
            }
            auth(s, url)
        }
        Err(ureq::Error::Status(401, _)) => json!({"ok": false, "error": "utente o password non validi"}),
        Err(e) => json!({"ok": false, "error": err_text(e)}),
    }
}

pub fn logout(s: &Sessions, url: &str) {
    let _ = req(s, url, "POST", "/api/auth/logout", 4).call();
    s.lock().unwrap().remove(url);
}

/// Scarica il progetto e lo scrive nel file scelto.
pub fn export(s: &Sessions, url: &str, path: &Path) -> J {
    match req(s, url, "GET", "/api/project/export", 60).call() {
        Ok(r) => match r.into_string() {
            Ok(text) => {
                let name = serde_json::from_str::<J>(&text).ok().and_then(|j| j.get("name").and_then(|v| v.as_str()).map(str::to_string)).unwrap_or_default();
                match std::fs::write(path, &text) {
                    Ok(()) => json!({"ok": true, "path": path.display().to_string(), "name": name, "size": text.len()}),
                    Err(e) => json!({"ok": false, "error": format!("scrittura {}: {e}", path.display())}),
                }
            }
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        },
        Err(e) => json!({"ok": false, "error": err_text(e)}),
    }
}

/// Legge l'intestazione di un file di progetto (per la conferma prima di aprirlo).
pub fn inspect(path: &Path) -> J {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return json!({"ok": false, "error": format!("lettura {}: {e}", path.display())}),
    };
    match serde_json::from_str::<J>(&text) {
        Ok(j) if j.get("format").and_then(|v| v.as_str()) == Some("ptwcs-project") => {
            let files = j.get("files").and_then(|v| v.as_object());
            let count = |p: &str| files.map(|f| f.keys().filter(|k| k.starts_with(p)).count()).unwrap_or(0);
            json!({"ok": true, "path": path.display().to_string(), "name": j.get("name"), "exported": j.get("exported"), "exported_by": j.get("exported_by"),
                "tags": j.pointer("/project/tags").and_then(|v| v.as_array()).map_or(0, |a| a.len()),
                "views": count("views/"), "scripts": count("scripts/"), "rules": count("rules/"),
                "recipes": j.get("recipes").and_then(|v| v.as_array()).map_or(0, |a| a.len())})
        }
        Ok(_) => json!({"ok": false, "error": "il file non è un progetto PTWCS"}),
        Err(e) => json!({"ok": false, "error": format!("file non valido: {e}")}),
    }
}

/// Invia il file al server, che sostituisce il progetto (con backup automatico).
pub fn import(s: &Sessions, url: &str, path: &Path, keep_security: bool) -> J {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => return json!({"ok": false, "error": format!("lettura {}: {e}", path.display())}),
    };
    let body = format!("{{\"bundle\":{text},\"keep_security\":{keep_security}}}");
    match req(s, url, "POST", "/api/project/import", 120).set("Content-Type", "application/json").send_string(&body) {
        Ok(r) => r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({"ok": true})),
        Err(e) => json!({"ok": false, "error": err_text(e)}),
    }
}

pub fn new_project(s: &Sessions, url: &str, name: &str) -> J {
    match req(s, url, "POST", "/api/project/new", 60).set("Content-Type", "application/json").send_string(&json!({"name": name}).to_string()) {
        Ok(r) => r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({"ok": true})),
        Err(e) => json!({"ok": false, "error": err_text(e)}),
    }
}

pub fn backups(s: &Sessions, url: &str) -> J {
    match req(s, url, "GET", "/api/project/backups", 10).call() {
        Ok(r) => json!({"ok": true, "list": r.into_string().ok().and_then(|t| serde_json::from_str::<J>(&t).ok()).unwrap_or(json!([]))}),
        Err(e) => json!({"ok": false, "error": err_text(e)}),
    }
}

pub fn backup_now(s: &Sessions, url: &str) -> J {
    match req(s, url, "POST", "/api/project/backups", 30).call() {
        Ok(r) => r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({"ok": true})),
        Err(e) => json!({"ok": false, "error": err_text(e)}),
    }
}

pub fn restore(s: &Sessions, url: &str, file: &str) -> J {
    match req(s, url, "POST", &format!("/api/project/backups/{file}/restore"), 120).call() {
        Ok(r) => r.into_string().ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or(json!({"ok": true})),
        Err(e) => json!({"ok": false, "error": err_text(e)}),
    }
}

/// Nome di file proposto per il salvataggio.
pub fn default_file(name: &str) -> PathBuf {
    let n: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let n = if n.trim_matches('_').is_empty() { "progetto".to_string() } else { n };
    dirs::document_dir().or_else(dirs::home_dir).unwrap_or_default().join(format!("{n}.ptwcs"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_and_inspect() {
        assert!(default_file("Impianto Demo").ends_with("Impianto_Demo.ptwcs"));
        let p = std::env::temp_dir().join(format!("ptwcs_test_{}.ptwcs", std::process::id()));
        std::fs::write(&p, r#"{"format":"ptwcs-project","version":1,"name":"X","project":{"tags":[{"name":"a"}]},"files":{"views/a.json":"{}","scripts/s.py":""},"recipes":[]}"#).unwrap();
        let i = inspect(&p);
        assert_eq!(i["ok"], true);
        assert_eq!(i["views"], 1);
        assert_eq!(i["tags"], 1);
        std::fs::write(&p, r#"{"altro":1}"#).unwrap();
        assert_eq!(inspect(&p)["ok"], false);
        let _ = std::fs::remove_file(p);
    }
}
