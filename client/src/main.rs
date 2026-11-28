//! PTWCS Client: applicazione desktop per runtime e Designer.
//!
//! - launcher con i server salvati (prova di connessione, nome impianto), apertura del
//!   runtime e del Designer in finestre native (WebView2 su Windows, WebKit su Linux/macOS);
//! - più finestre, modalità chiosco per le postazioni operatore, avvio diretto da riga di comando;
//! - pagina di riconnessione se il server non risponde (riprova da sola);
//! - sessione di login condivisa tra le finestre e conservata tra un avvio e l'altro;
//! - link "nuova finestra" (manuale, Designer ↗) in finestre del client; download nella
//!   cartella Download; scorciatoie F5, F11, Ctrl+±0, Ctrl+L, Ctrl+N.
//!
//! Uso: ptwcs-client [--server URL] [--runtime | --designer] [--kiosk] [--launcher]

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod project;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::{Fullscreen, Window, WindowBuilder, WindowId};
use wry::{WebContext, WebView, WebViewBuilder};

const LAUNCHER: &str = include_str!("launcher.html");
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Server {
    #[serde(default)]
    name: String,
    url: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    servers: Vec<Server>,
    #[serde(default)]
    last: Option<String>,
    #[serde(default)]
    kiosk: bool,
    #[serde(default)]
    autostart: bool,
    /// File di progetto aperti o salvati di recente.
    #[serde(default)]
    recent: Vec<Recent>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Recent {
    path: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    server: String,
    #[serde(default)]
    ts: u64,
}

fn config_path() -> PathBuf {
    dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("ptwcs-client").join("config.json")
}

fn load_config() -> Config {
    std::fs::read_to_string(config_path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| Config {
        servers: vec![Server { name: "Server locale".into(), url: "http://localhost:8080".into() }],
        ..Default::default()
    })
}

fn save_config(c: &Config) {
    let p = config_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, serde_json::to_string_pretty(c).unwrap_or_default());
}

/// Verifica un server: GET /api/info → nome impianto e Designer abilitato.
fn probe(url: &str) -> serde_json::Value {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(3)).build();
    match agent.get(&format!("{url}/api/info")).call() {
        Ok(r) => match r.into_string().map_err(|e| e.to_string()).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).map_err(|e| e.to_string())) {
            Ok(j) => serde_json::json!({"ok": true, "plant": j.pointer("/plant/name").and_then(|v| v.as_str()).unwrap_or("PTWCS"),
                "designer": j.get("designer").and_then(|v| v.as_bool()).unwrap_or(true)}),
            Err(e) => serde_json::json!({"ok": false, "error": format!("risposta non valida: {e}")}),
        },
        Err(ureq::Error::Status(code, _)) => serde_json::json!({"ok": false, "error": format!("HTTP {code}")}),
        Err(e) => serde_json::json!({"ok": false, "error": e.kind().to_string()}),
    }
}

/// Pagina mostrata quando il server non risponde: chiede al client di riprovare.
fn offline_page(url: &str, err: &str) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>
body{{margin:0;height:100vh;display:grid;place-items:center;background:#0b1220;color:#e2e8f0;font:15px system-ui,sans-serif}}
.c{{text-align:center;max-width:520px}} .s{{width:42px;height:42px;border:4px solid #334155;border-top-color:#3b82f6;border-radius:50%;margin:0 auto 18px;animation:r 1s linear infinite}}
@keyframes r{{to{{transform:rotate(360deg)}}}} code{{color:#93c5fd}} small{{color:#94a3b8}} button{{margin-top:14px;font:inherit;padding:8px 16px;border-radius:7px;border:1px solid #3b82f6;background:#1d4ed8;color:#fff;cursor:pointer}}
</style></head><body><div class="c"><div class="s"></div><h2>Server non raggiungibile</h2><p><code>{url}</code></p><small>{err}<br>Nuovo tentativo ogni 3 secondi…</small><br>
<button onclick="window.ipc.postMessage(JSON.stringify({{cmd:'launcher'}}))">Torna al launcher</button></div>
<script>setInterval(()=>window.ipc.postMessage(JSON.stringify({{cmd:'retry'}})),3000)</script></body></html>"#
    )
}

/// Script iniettato nelle finestre di runtime/Designer: scorciatoie da tastiera.
const SHORTCUTS: &str = r#"
(() => {
  if (window.__ptwcsClient) return; window.__ptwcsClient = true;
  const send = (m) => window.ipc && window.ipc.postMessage(JSON.stringify(m));
  window.addEventListener('keydown', (e) => {
    const c = e.ctrlKey || e.metaKey;
    if (e.key === 'F5' || (c && !e.shiftKey && e.key.toLowerCase() === 'r')) { e.preventDefault(); location.reload(); }
    else if (e.key === 'F11') { e.preventDefault(); send({cmd: 'fullscreen'}); }
    else if (c && (e.key === '+' || e.key === '=')) { e.preventDefault(); send({cmd: 'zoom', d: 0.1}); }
    else if (c && e.key === '-') { e.preventDefault(); send({cmd: 'zoom', d: -0.1}); }
    else if (c && e.key === '0') { e.preventDefault(); send({cmd: 'zoom', d: 0}); }
    else if (c && e.key.toLowerCase() === 'l') { e.preventDefault(); send({cmd: 'launcher'}); }
    else if (c && !e.shiftKey && e.key.toLowerCase() === 'n') { e.preventDefault(); send({cmd: 'newwin', url: location.href}); }
    else if (c && e.shiftKey && e.key.toLowerCase() === 'q') { e.preventDefault(); send({cmd: 'close'}); }
  }, true);
})();
"#;

#[derive(Debug)]
enum Ev {
    /// Apre runtime/Designer (o un URL qualunque del server) in una nuova finestra.
    Open { url: String, kiosk: bool },
    Launcher,
    Ipc { win: WindowId, msg: String },
    Eval { win: WindowId, js: String },
    Load { win: WindowId, url: String },
    /// Il progetto del server è cambiato: ricarica le sue finestre.
    Reload { base: String },
    /// Aggiunge un file ai recenti.
    Recent { path: String, name: String, server: String },
}

struct AppWin {
    window: Window,
    webview: WebView,
    /// URL da caricare (per la riconnessione); None = launcher.
    target: Option<String>,
    kiosk: bool,
    zoom: f64,
}

struct Client {
    proxy: EventLoopProxy<Ev>,
    ctx: WebContext,
    wins: HashMap<WindowId, AppWin>,
    cfg: Config,
    sessions: project::Sessions,
}

impl Client {
    fn build(&mut self, target: &EventLoopWindowTarget<Ev>, title: &str, url: Option<&str>, html: Option<String>, kiosk: bool) -> WindowId {
        let mut wb = WindowBuilder::new().with_title(title).with_inner_size(LogicalSize::new(1440.0, 900.0)).with_min_inner_size(LogicalSize::new(640.0, 420.0));
        if kiosk {
            wb = wb.with_decorations(false).with_fullscreen(Some(Fullscreen::Borderless(None)));
        }
        let window = wb.build(target).expect("creazione finestra");
        let wid = window.id();
        let p1 = self.proxy.clone();
        let p2 = self.proxy.clone();
        let mut b = WebViewBuilder::with_web_context(&mut self.ctx)
            .with_ipc_handler(move |req| {
                let _ = p1.send_event(Ev::Ipc { win: wid, msg: req.body().clone() });
            })
            .with_new_window_req_handler(move |u| {
                let _ = p2.send_event(Ev::Open { url: u, kiosk: false });
                false
            })
            .with_download_started_handler(|url, path| {
                let name = path.file_name().map(|n| n.to_os_string()).unwrap_or_else(|| url.rsplit('/').next().unwrap_or("download").into());
                *path = dirs::download_dir().unwrap_or_else(std::env::temp_dir).join(name);
                true
            })
            .with_initialization_script(SHORTCUTS)
            .with_devtools(cfg!(debug_assertions));
        b = match (url, html) {
            (_, Some(h)) => b.with_html(h),
            (Some(u), None) => b.with_url(u),
            _ => b,
        };
        #[cfg(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd"))]
        let webview = {
            use tao::platform::unix::WindowExtUnix;
            use wry::WebViewBuilderExtUnix;
            b.build_gtk(window.default_vbox().expect("vbox gtk")).expect("creazione webview")
        };
        #[cfg(not(any(target_os = "linux", target_os = "dragonfly", target_os = "freebsd", target_os = "netbsd", target_os = "openbsd")))]
        let webview = b.build(&window).expect("creazione webview");
        self.wins.insert(wid, AppWin { window, webview, target: url.map(str::to_string), kiosk, zoom: 1.0 });
        wid
    }

    fn launcher(&mut self, target: &EventLoopWindowTarget<Ev>) {
        if let Some((_, w)) = self.wins.iter().find(|(_, w)| w.target.is_none()) {
            w.window.set_focus();
            return;
        }
        let id = self.build(target, "PTWCS Client", None, Some(LAUNCHER.to_string()), false);
        if let Some(w) = self.wins.get(&id) {
            w.window.set_inner_size(LogicalSize::new(1000.0, 720.0));
        }
    }

    /// Apre un URL del server: prima verifica che risponda (altrimenti pagina di riconnessione).
    fn open(&mut self, target: &EventLoopWindowTarget<Ev>, url: &str, kiosk: bool) {
        let base = base_of(url);
        let st = probe(&base);
        let app = if url.contains("designer.html") { "Designer" } else { "Runtime" };
        let plant = st.get("plant").and_then(|v| v.as_str()).unwrap_or("PTWCS");
        let title = format!("{plant} – {app} · PTWCS");
        if st.get("ok").and_then(|v| v.as_bool()) == Some(true) {
            self.build(target, &title, Some(url), None, kiosk);
        } else {
            let err = st.get("error").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let id = self.build(target, &title, None, Some(offline_page(&base, &err)), kiosk);
            if let Some(w) = self.wins.get_mut(&id) {
                w.target = Some(url.to_string());
            }
        }
        self.cfg.last = Some(base);
        save_config(&self.cfg);
    }

    fn ipc(&mut self, target: &EventLoopWindowTarget<Ev>, win: WindowId, msg: &str, control: &mut ControlFlow) {
        let Ok(m) = serde_json::from_str::<serde_json::Value>(msg) else { return };
        let s = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        match m.get("cmd").and_then(|v| v.as_str()).unwrap_or("") {
            "state" => {
                let js = format!("window.__state({}, {:?})", serde_json::to_string(&self.cfg).unwrap_or_default(), VERSION);
                let _ = self.proxy.send_event(Ev::Eval { win, js });
            }
            "save" => {
                if let Ok(c) = serde_json::from_value::<Config>(m.get("config").cloned().unwrap_or_default()) {
                    let last = self.cfg.last.clone();
                    self.cfg = Config { last, ..c };
                    save_config(&self.cfg);
                }
            }
            "test" => {
                let (url, proxy) = (s("url"), self.proxy.clone());
                std::thread::spawn(move || {
                    let r = probe(&url);
                    let _ = proxy.send_event(Ev::Eval { win, js: format!("window.__test({:?}, {})", url, r) });
                });
            }
            "open" => {
                let path = if s("app") == "designer" { "/designer.html" } else { "/" };
                let kiosk = m.get("kiosk").and_then(|v| v.as_bool()).unwrap_or(false);
                self.open(target, &format!("{}{path}", s("url")), kiosk);
            }
            "launcher" => self.launcher(target),
            "newwin" => self.open(target, &s("url"), false),
            "retry" => {
                if let Some(url) = self.wins.get(&win).and_then(|w| w.target.clone()) {
                    let proxy = self.proxy.clone();
                    std::thread::spawn(move || {
                        if probe(&base_of(&url)).get("ok").and_then(|v| v.as_bool()) == Some(true) {
                            let _ = proxy.send_event(Ev::Load { win, url });
                        }
                    });
                }
            }
            "fullscreen" => {
                if let Some(w) = self.wins.get(&win).filter(|w| !w.kiosk) {
                    w.window.set_fullscreen(if w.window.fullscreen().is_some() { None } else { Some(Fullscreen::Borderless(None)) });
                }
            }
            "zoom" => {
                if let Some(w) = self.wins.get_mut(&win) {
                    let d = m.get("d").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    w.zoom = if d == 0.0 { 1.0 } else { (w.zoom + d).clamp(0.5, 2.5) };
                    let _ = w.webview.zoom(w.zoom);
                }
            }
            "proj" => self.project_cmd(win, &m),
            "close" => {
                self.wins.remove(&win);
                if self.wins.is_empty() {
                    *control = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    }
}

impl Client {
    /// Comandi del menu Progetto del launcher. Risposta con `window.__res(id, risultato)`.
    fn project_cmd(&mut self, win: WindowId, m: &serde_json::Value) {
        let s = |k: &str| m.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let id = m.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let (url, op) = (s("url"), s("op"));
        let (proxy, sess) = (self.proxy.clone(), self.sessions.clone());
        let reply = move |proxy: &EventLoopProxy<Ev>, r: serde_json::Value| {
            let _ = proxy.send_event(Ev::Eval { win, js: format!("window.__res({id}, {r})") });
        };
        // dialoghi di file nel thread principale (nativi), rete in un thread separato
        match op.as_str() {
            "save" => {
                let name = s("name");
                let def = project::default_file(&name);
                let Some(path) = file_dialog(true, "Salva il progetto", &def) else {
                    return reply(&proxy, serde_json::json!({"ok": false, "cancel": true}));
                };
                let path = if path.extension().is_none() { path.with_extension("ptwcs") } else { path };
                std::thread::spawn(move || {
                    let r = project::export(&sess, &url, &path);
                    if r["ok"] == true {
                        let _ = proxy.send_event(Ev::Recent { path: path.display().to_string(), name: r["name"].as_str().unwrap_or("").into(), server: url.clone() });
                    }
                    reply(&proxy, r);
                });
            }
            "pick" => {
                let path = if s("path").is_empty() {
                    match file_dialog(false, "Apri un progetto", &project::default_file("")) {
                        Some(p) => p,
                        None => return reply(&proxy, serde_json::json!({"ok": false, "cancel": true})),
                    }
                } else {
                    PathBuf::from(s("path"))
                };
                reply(&proxy, project::inspect(&path));
            }
            _ => {
                let keep = m.get("keep_security").and_then(|v| v.as_bool()).unwrap_or(true);
                let (user, pass, name, file, path) = (s("user"), s("pass"), s("name"), s("file"), s("path"));
                std::thread::spawn(move || {
                    let r = match op.as_str() {
                        "auth" => project::auth(&sess, &url),
                        "login" => project::login(&sess, &url, &user, &pass),
                        "logout" => {
                            project::logout(&sess, &url);
                            project::auth(&sess, &url)
                        }
                        "open" => project::import(&sess, &url, std::path::Path::new(&path), keep),
                        "new" => project::new_project(&sess, &url, &name),
                        "backups" => project::backups(&sess, &url),
                        "backup_now" => project::backup_now(&sess, &url),
                        "restore" => project::restore(&sess, &url, &file),
                        _ => serde_json::json!({"ok": false, "error": format!("operazione sconosciuta: {op}")}),
                    };
                    if r["ok"] == true && matches!(op.as_str(), "open" | "new" | "restore") {
                        let _ = proxy.send_event(Ev::Reload { base: url.clone() });
                        if op == "open" {
                            let nm = r.pointer("/result/name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            let _ = proxy.send_event(Ev::Recent { path, name: nm, server: url.clone() });
                        }
                    }
                    reply(&proxy, r);
                });
            }
        }
    }
}

/// Dialogo nativo per aprire o salvare un file .ptwcs (`def` = cartella e nome proposti).
#[cfg(target_os = "linux")]
fn file_dialog(save: bool, title: &str, def: &std::path::Path) -> Option<PathBuf> {
    use gtk::prelude::*;
    let action = if save { gtk::FileChooserAction::Save } else { gtk::FileChooserAction::Open };
    let ok = if save { "Salva" } else { "Apri" };
    let d = gtk::FileChooserDialog::with_buttons(Some(title), None::<&gtk::Window>, action, &[("Annulla", gtk::ResponseType::Cancel), (ok, gtk::ResponseType::Accept)]);
    let f = gtk::FileFilter::new();
    f.add_pattern("*.ptwcs");
    f.set_name(Some("Progetto PTWCS (*.ptwcs)"));
    d.add_filter(f);
    if let Some(dir) = def.parent() {
        let _ = std::fs::create_dir_all(dir);
        d.set_current_folder(dir);
    }
    if save {
        d.set_current_name(&def.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        d.set_do_overwrite_confirmation(true);
    }
    let r = d.run();
    let path = if r == gtk::ResponseType::Accept { d.filename() } else { None };
    d.close();
    path
}

#[cfg(not(target_os = "linux"))]
fn file_dialog(save: bool, title: &str, def: &std::path::Path) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title(title).add_filter("Progetto PTWCS", &["ptwcs"]);
    if let Some(dir) = def.parent() {
        d = d.set_directory(dir);
    }
    if save {
        d.set_file_name(def.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).save_file()
    } else {
        d.pick_file()
    }
}

fn base_of(url: &str) -> String {
    let rest = url.split_once("://");
    match rest {
        Some((scheme, r)) => format!("{scheme}://{}", r.split('/').next().unwrap_or(r)),
        None => url.trim_end_matches('/').to_string(),
    }
}

fn main() {
    // riga di comando
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str| args.iter().any(|a| a == k);
    let server = args.iter().position(|a| a == "--server").and_then(|i| args.get(i + 1)).map(|s| s.trim_end_matches('/').to_string());
    if arg("--help") || arg("-h") {
        println!("PTWCS Client {VERSION}\nUso: ptwcs-client [--server URL] [--runtime | --designer] [--kiosk] [--launcher]");
        return;
    }
    let cfg = load_config();
    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let proxy = event_loop.create_proxy();
    let data = dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("ptwcs-client").join("webdata");
    let _ = std::fs::create_dir_all(&data);
    let mut client = Client { proxy: proxy.clone(), ctx: WebContext::new(Some(data)), wins: HashMap::new(), cfg, sessions: Default::default() };

    // cosa aprire all'avvio
    let first = match (&server, arg("--launcher")) {
        (_, true) => Ev::Launcher,
        (Some(s), _) => Ev::Open { url: format!("{s}{}", if arg("--designer") { "/designer.html" } else { "/" }), kiosk: arg("--kiosk") },
        (None, _) if client.cfg.autostart && client.cfg.last.is_some() => Ev::Open { url: format!("{}/", client.cfg.last.clone().unwrap()), kiosk: client.cfg.kiosk },
        _ => Ev::Launcher,
    };
    let _ = proxy.send_event(first);

    event_loop.run(move |event, target, control_flow| {
        *control_flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {}
            Event::UserEvent(ev) => match ev {
                Ev::Open { url, kiosk } => client.open(target, &url, kiosk),
                Ev::Launcher => client.launcher(target),
                Ev::Ipc { win, msg } => client.ipc(target, win, &msg, control_flow),
                Ev::Eval { win, js } => {
                    if let Some(w) = client.wins.get(&win) {
                        let _ = w.webview.evaluate_script(&js);
                    }
                }
                Ev::Load { win, url } => {
                    if let Some(w) = client.wins.get(&win) {
                        let _ = w.webview.load_url(&url);
                    }
                }
                Ev::Reload { base } => {
                    for w in client.wins.values().filter(|w| w.target.as_deref().is_some_and(|t| base_of(t) == base)) {
                        let _ = w.webview.evaluate_script("location.reload()");
                    }
                }
                Ev::Recent { path, name, server } => {
                    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
                    client.cfg.recent.retain(|r| r.path != path);
                    client.cfg.recent.insert(0, Recent { path, name, server, ts });
                    client.cfg.recent.truncate(10);
                    save_config(&client.cfg);
                    let js = format!("window.__recent && window.__recent({})", serde_json::to_string(&client.cfg.recent).unwrap_or_default());
                    for (id, w) in &client.wins {
                        if w.target.is_none() {
                            let _ = client.proxy.send_event(Ev::Eval { win: *id, js: js.clone() });
                        }
                    }
                }
            },
            Event::WindowEvent { window_id, event: WindowEvent::CloseRequested, .. } => {
                client.wins.remove(&window_id);
                if client.wins.is_empty() {
                    *control_flow = ControlFlow::Exit;
                }
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url() {
        assert_eq!(base_of("http://10.0.0.5:8080/designer.html"), "http://10.0.0.5:8080");
        assert_eq!(base_of("https://scada.local/#/view/hub"), "https://scada.local");
        assert_eq!(base_of("http://localhost:8080/"), "http://localhost:8080");
    }

    #[test]
    fn offline_page_has_retry() {
        let p = offline_page("http://x:8080", "connessione rifiutata");
        assert!(p.contains("retry") && p.contains("http://x:8080"));
    }
}
