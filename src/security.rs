//! Login, sessioni, utenti e permessi.
//!
//! - **Ruoli e permessi** sono nel progetto (`security.roles`), configurabili dal Designer.
//! - **Utenti** (con hash Argon2 della password) e **sessioni** sono nel database
//!   del server, mai nel file di progetto.
//! - La sessione è un token casuale in un cookie `HttpOnly; SameSite=Strict`
//!   (o header `Authorization: Bearer <token>` per i client API); nel database
//!   è salvato solo l'hash SHA-256 del token.
//! - A sicurezza disattivata ogni richiesta ha tutti i permessi.

use crate::config::SecurityDef;
use crate::store::Store;
use crate::tagdb::now_ms;
use anyhow::{Result, anyhow, bail};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng, rand_core::RngCore};
use argon2::Argon2;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

/// Catalogo dei permessi: (id, gruppo, descrizione).
pub const PERMISSIONS: &[(&str, &str, &str)] = &[
    ("runtime.view", "Runtime", "Aprire il runtime e le viste (salvo restrizioni della singola vista)"),
    ("tag.write", "Runtime", "Scrivere tag: comandi da pulsanti e faceplate, pagina Tag (salvo ruoli del tag)"),
    ("alarm.ack", "Runtime", "Riconoscere gli allarmi"),
    ("script.call", "Runtime", "Eseguire script Python da pulsanti delle viste"),
    ("wh.operate", "WCS", "Missioni, ordini, prelievi, depositi, movimentazioni, blocco celle"),
    ("wh.config", "WCS", "Riorganizzazione automatica e rettifiche dell'anagrafica UdC"),
    ("seq.operate", "Processo", "Fasi (sequenze) in manuale: avvio, hold, restart, stop, abort, reset, avanzamento"),
    ("batch.operate", "Processo", "Batch: avviare una ricetta (allocando i dispositivi) e comandarla"),
    ("recipe.edit", "Processo", "Ricette: creare, modificare ed eliminare le procedure"),
    ("audit.view", "Amministrazione", "Consultare ed esportare l'audit trail"),
    ("designer", "Designer", "Aprire il Designer (consultazione)"),
    ("designer.views", "Designer", "Modificare viste, faceplate e simboli"),
    ("designer.project", "Designer", "Modificare e salvare il progetto: tag, UDT, aree, protocollo, classi, WMS, magazzini"),
    ("rules.edit", "Designer", "Modificare le regole di smistamento"),
    ("scripts.edit", "Designer", "Modificare, testare e ricaricare gli script Python"),
    ("users.manage", "Amministrazione", "Gestire utenti, ruoli e impostazioni di sicurezza"),
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct User {
    pub username: String,
    #[serde(default)]
    pub full_name: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Hash Argon2 (PHC). Mai restituito dalle API.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
    /// Al prossimo login deve cambiare la password.
    #[serde(default)]
    pub must_change: bool,
    #[serde(default)]
    pub created: i64,
    #[serde(default)]
    pub last_login: Option<i64>,
}
fn yes() -> bool {
    true
}

impl User {
    /// Vista pubblica (senza hash).
    pub fn public(&self) -> serde_json::Value {
        serde_json::json!({"username": self.username, "full_name": self.full_name, "roles": self.roles, "enabled": self.enabled,
            "must_change": self.must_change, "created": self.created, "last_login": self.last_login})
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    username: String,
    created: i64,
    last_seen: i64,
}

/// Chi sta facendo la richiesta.
#[derive(Debug, Clone, Serialize)]
pub struct Principal {
    /// None = anonimo.
    pub user: Option<String>,
    pub full_name: String,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
    /// tutti i permessi (sicurezza disattivata o permesso "*")
    pub all: bool,
    pub must_change: bool,
}

impl Principal {
    pub fn superuser() -> Self {
        Self { user: None, full_name: String::new(), roles: vec![], permissions: vec!["*".into()], all: true, must_change: false }
    }
    pub fn can(&self, perm: &str) -> bool {
        self.all || self.permissions.iter().any(|p| p == perm || (p.ends_with(".*") && perm.starts_with(&p[..p.len() - 1])))
    }
    /// Ha almeno uno dei ruoli (lista vuota = nessuna restrizione).
    pub fn in_roles(&self, roles: &[String]) -> bool {
        roles.is_empty() || self.all || self.roles.iter().any(|r| roles.contains(r))
    }
    /// Nome per log e audit.
    pub fn name(&self) -> String {
        self.user.clone().unwrap_or_else(|| if self.all { String::new() } else { "anonimo".into() })
    }
}

#[derive(Default)]
pub struct Security {
    users: RwLock<HashMap<String, User>>,
    sessions: RwLock<HashMap<String, Session>>,
    /// tentativi falliti: utente → (conteggio, bloccato fino a)
    failures: RwLock<HashMap<String, (u32, i64)>>,
    store: RwLock<Option<Store>>,
}

fn token_key(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn hash_password(pw: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(pw.as_bytes(), &salt).map(|h| h.to_string()).map_err(|e| anyhow!("hash password: {e}"))
}

fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
}

pub fn check_password_policy(pw: &str) -> Result<()> {
    if pw.chars().count() < 6 {
        bail!("la password deve avere almeno 6 caratteri");
    }
    Ok(())
}

fn valid_username(u: &str) -> bool {
    !u.is_empty() && u.len() <= 64 && u.chars().all(|c| c.is_ascii_alphanumeric() || "._-@".contains(c))
}

impl Security {
    /// Carica utenti e sessioni; se non esistono utenti crea `admin` (password
    /// da `PTWCS_ADMIN_PASSWORD`, altrimenti "admin" con cambio obbligatorio).
    pub async fn open(store: Store) -> Result<Self> {
        let me = Self::default();
        for (k, d) in store.kv_load("sec_users").await? {
            if let Ok(u) = serde_json::from_str::<User>(&d) {
                me.users.write().unwrap().insert(k, u);
            }
        }
        for (k, d) in store.kv_load("sec_sessions").await? {
            if let Ok(s) = serde_json::from_str::<Session>(&d) {
                me.sessions.write().unwrap().insert(k, s);
            }
        }
        *me.store.write().unwrap() = Some(store);
        if me.users.read().unwrap().is_empty() {
            let (pw, must) = match std::env::var("PTWCS_ADMIN_PASSWORD") {
                Ok(p) if !p.is_empty() => (p, false),
                _ => ("admin".to_string(), true),
            };
            let u = User { username: "admin".into(), full_name: "Amministratore".into(), roles: vec!["amministratore".into()], enabled: true, hash: hash_password(&pw)?, must_change: must, created: now_ms(), last_login: None };
            me.persist_user(&u);
            me.users.write().unwrap().insert(u.username.clone(), u);
            tracing::warn!("sicurezza: creato l'utente 'admin'{}", if must { " con password 'admin' (cambio obbligatorio al primo accesso)" } else { "" });
        }
        Ok(me)
    }

    fn persist_user(&self, u: &User) {
        if let Some(s) = self.store.read().unwrap().clone() {
            let (k, d) = (u.username.clone(), serde_json::to_string(u).ok());
            tokio::spawn(async move {
                let _ = s.kv_put("sec_users", k, d).await;
            });
        }
    }
    fn persist_session(&self, key: &str, s: Option<&Session>) {
        if let Some(st) = self.store.read().unwrap().clone() {
            let (k, d) = (key.to_string(), s.and_then(|x| serde_json::to_string(x).ok()));
            tokio::spawn(async move {
                let _ = st.kv_put("sec_sessions", k, d).await;
            });
        }
    }

    fn principal_of(&self, user: &User, cfg: &SecurityDef) -> Principal {
        let mut perms: HashSet<String> = HashSet::new();
        for r in cfg.roles.iter().filter(|r| user.roles.contains(&r.name)) {
            perms.extend(r.permissions.iter().cloned());
        }
        let all = perms.contains("*");
        let mut permissions: Vec<String> = perms.into_iter().collect();
        permissions.sort();
        Principal { user: Some(user.username.clone()), full_name: user.full_name.clone(), roles: user.roles.clone(), permissions, all, must_change: user.must_change }
    }

    fn anonymous(&self, cfg: &SecurityDef) -> Principal {
        let role = cfg.roles.iter().find(|r| !cfg.anonymous_role.is_empty() && r.name == cfg.anonymous_role);
        let permissions = role.map(|r| r.permissions.clone()).unwrap_or_default();
        Principal {
            user: None,
            full_name: String::new(),
            roles: role.map(|r| vec![r.name.clone()]).unwrap_or_default(),
            all: permissions.iter().any(|p| p == "*"),
            permissions,
            must_change: false,
        }
    }

    /// Principal della richiesta (token di sessione facoltativo).
    pub fn principal(&self, token: Option<&str>, cfg: &SecurityDef) -> Principal {
        if !cfg.enabled {
            return Principal::superuser();
        }
        let Some(tok) = token.filter(|t| !t.is_empty()) else { return self.anonymous(cfg) };
        let key = token_key(tok);
        let now = now_ms();
        let user = {
            let mut sessions = self.sessions.write().unwrap();
            let Some(s) = sessions.get_mut(&key) else { return self.anonymous(cfg) };
            let expired = now - s.created > cfg.session_minutes.max(1) as i64 * 60_000
                || (cfg.idle_logout_minutes > 0 && now - s.last_seen > cfg.idle_logout_minutes as i64 * 60_000);
            if expired {
                sessions.remove(&key);
                drop(sessions);
                self.persist_session(&key, None);
                return self.anonymous(cfg);
            }
            // aggiorna last_seen al massimo ogni 30 s (evita scritture continue)
            if now - s.last_seen > 30_000 {
                s.last_seen = now;
                let snap = s.clone();
                drop(sessions);
                self.persist_session(&key, Some(&snap));
                snap.username
            } else {
                s.username.clone()
            }
        };
        match self.users.read().unwrap().get(&user) {
            Some(u) if u.enabled => self.principal_of(u, cfg),
            _ => self.anonymous(cfg),
        }
    }

    /// Login: ritorna il token di sessione.
    pub fn login(&self, username: &str, password: &str, cfg: &SecurityDef) -> Result<(String, Principal)> {
        let username = username.trim();
        let now = now_ms();
        if let Some((n, until)) = self.failures.read().unwrap().get(username)
            && *n >= 5
            && now < *until
        {
            bail!("troppi tentativi falliti: riprova tra {} s", (until - now) / 1000 + 1);
        }
        let ok = {
            let users = self.users.read().unwrap();
            match users.get(username) {
                Some(u) => u.enabled && verify_password(password, &u.hash),
                None => {
                    // tempo costante: verifica comunque un hash fittizio
                    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
                    let _ = verify_password(password, DUMMY.get_or_init(|| hash_password("-").unwrap_or_default()));
                    false
                }
            }
        };
        if !ok {
            let mut f = self.failures.write().unwrap();
            let e = f.entry(username.to_string()).or_insert((0, 0));
            e.0 += 1;
            if e.0 >= 5 {
                e.1 = now + 30_000 * (e.0 as i64 - 4).min(20);
            }
            bail!("utente o password non validi");
        }
        self.failures.write().unwrap().remove(username);
        let mut raw = [0u8; 32];
        OsRng.fill_bytes(&mut raw);
        let token = hex::encode(raw);
        let s = Session { username: username.to_string(), created: now, last_seen: now };
        let key = token_key(&token);
        self.persist_session(&key, Some(&s));
        self.sessions.write().unwrap().insert(key, s);
        let p = {
            let mut users = self.users.write().unwrap();
            let u = users.get_mut(username).unwrap();
            u.last_login = Some(now);
            let u = u.clone();
            drop(users);
            self.persist_user(&u);
            self.principal_of(&u, cfg)
        };
        Ok((token, p))
    }

    pub fn logout(&self, token: &str) {
        let key = token_key(token);
        self.sessions.write().unwrap().remove(&key);
        self.persist_session(&key, None);
    }

    /// Chiude tutte le sessioni di un utente (disattivazione, cambio password da admin).
    fn drop_sessions_of(&self, username: &str, except: Option<&str>) {
        let keep = except.map(token_key);
        let keys: Vec<String> = self.sessions.read().unwrap().iter().filter(|(k, s)| s.username == username && Some(*k) != keep.as_ref()).map(|(k, _)| k.clone()).collect();
        for k in keys {
            self.sessions.write().unwrap().remove(&k);
            self.persist_session(&k, None);
        }
    }

    pub fn users(&self) -> Vec<serde_json::Value> {
        let mut v: Vec<User> = self.users.read().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.username.cmp(&b.username));
        v.iter().map(User::public).collect()
    }

    /// Crea o aggiorna un utente (la password, se presente, viene impostata).
    pub fn upsert_user(&self, username: &str, full_name: String, roles: Vec<String>, enabled: bool, password: Option<&str>, must_change: Option<bool>) -> Result<()> {
        let username = username.trim();
        if !valid_username(username) {
            bail!("nome utente non valido (lettere, numeri, . _ - @)");
        }
        let mut users = self.users.write().unwrap();
        let exists = users.contains_key(username);
        if !exists && password.is_none() {
            bail!("password obbligatoria per un nuovo utente");
        }
        let mut u = users.get(username).cloned().unwrap_or(User { username: username.into(), created: now_ms(), enabled: true, ..Default::default() });
        u.full_name = full_name;
        u.roles = roles;
        u.enabled = enabled;
        if let Some(pw) = password {
            check_password_policy(pw)?;
            u.hash = hash_password(pw)?;
            u.must_change = must_change.unwrap_or(true);
        } else if let Some(m) = must_change {
            u.must_change = m;
        }
        let disable = !u.enabled;
        let pw_changed = password.is_some();
        users.insert(username.into(), u.clone());
        drop(users);
        // non lasciare il sistema senza un amministratore attivo
        self.persist_user(&u);
        if disable || pw_changed {
            self.drop_sessions_of(username, None);
        }
        Ok(())
    }

    pub fn delete_user(&self, username: &str) -> Result<()> {
        if self.users.write().unwrap().remove(username).is_none() {
            bail!("utente {username} non trovato");
        }
        if let Some(s) = self.store.read().unwrap().clone() {
            let k = username.to_string();
            tokio::spawn(async move {
                let _ = s.kv_put("sec_users", k, None).await;
            });
        }
        self.drop_sessions_of(username, None);
        Ok(())
    }

    /// Numero di utenti attivi con un ruolo che ha `users.manage` (o "*").
    pub fn admins(&self, cfg: &SecurityDef, excluding: Option<&str>) -> usize {
        let admin_roles: Vec<&str> = cfg.roles.iter().filter(|r| r.permissions.iter().any(|p| p == "*" || p == "users.manage")).map(|r| r.name.as_str()).collect();
        self.users.read().unwrap().values().filter(|u| u.enabled && Some(u.username.as_str()) != excluding && u.roles.iter().any(|r| admin_roles.contains(&r.as_str()))).count()
    }

    /// Cambio della propria password (verifica la vecchia).
    pub fn change_password(&self, username: &str, old: &str, new: &str, keep_token: Option<&str>) -> Result<()> {
        check_password_policy(new)?;
        if old == new {
            bail!("la nuova password deve essere diversa dalla precedente");
        }
        let mut users = self.users.write().unwrap();
        let u = users.get_mut(username).ok_or_else(|| anyhow!("utente non trovato"))?;
        if !verify_password(old, &u.hash) {
            bail!("password attuale errata");
        }
        u.hash = hash_password(new)?;
        u.must_change = false;
        let u = u.clone();
        drop(users);
        self.persist_user(&u);
        self.drop_sessions_of(username, keep_token);
        Ok(())
    }

    pub fn sessions(&self) -> Vec<serde_json::Value> {
        self.sessions.read().unwrap().values().map(|s| serde_json::json!({"username": s.username, "created": s.created, "last_seen": s.last_seen})).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permissions_and_wildcards() {
        let p = Principal { user: Some("x".into()), full_name: String::new(), roles: vec!["op".into()], permissions: vec!["wh.*".into(), "tag.write".into()], all: false, must_change: false };
        assert!(p.can("wh.operate") && p.can("wh.config") && p.can("tag.write"));
        assert!(!p.can("designer") && !p.can("alarm.ack"));
        assert!(p.in_roles(&[]) && p.in_roles(&["op".into()]) && !p.in_roles(&["admin".into()]));
        let h = hash_password("segreta1").unwrap();
        assert!(verify_password("segreta1", &h) && !verify_password("altra", &h));
    }
}
