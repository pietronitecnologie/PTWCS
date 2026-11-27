//! Login, permessi per ruolo, ruoli delle viste e ruoli di scrittura dei tag.
use serde_json::{Value as J, json};
use wcs_scada::{app, config::Config, web};

struct Client {
    base: String,
    http: reqwest::Client,
    cookie: Option<String>,
}

impl Client {
    async fn req(&mut self, m: reqwest::Method, path: &str, body: Option<J>) -> (u16, J) {
        let mut rq = self.http.request(m, format!("{}{}", self.base, path));
        if let Some(c) = &self.cookie {
            rq = rq.header("cookie", c);
        }
        if let Some(b) = body {
            rq = rq.json(&b);
        }
        let r = rq.send().await.unwrap();
        if let Some(sc) = r.headers().get("set-cookie") {
            let c = sc.to_str().unwrap().split(';').next().unwrap().to_string();
            self.cookie = if c.ends_with('=') { None } else { Some(c) };
        }
        let st = r.status().as_u16();
        (st, r.json().await.unwrap_or(J::Null))
    }
    async fn get(&mut self, p: &str) -> (u16, J) {
        self.req(reqwest::Method::GET, p, None).await
    }
    async fn post(&mut self, p: &str, b: J) -> (u16, J) {
        self.req(reqwest::Method::POST, p, Some(b)).await
    }
    async fn put(&mut self, p: &str, b: J) -> (u16, J) {
        self.req(reqwest::Method::PUT, p, Some(b)).await
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn login_roles_views_and_tag_write_permissions() {
    let dir = std::env::temp_dir().join(format!("wcs_sec_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("views")).unwrap();
    std::fs::write(dir.join("plant.yaml"), "plant: { name: T }\nserver: { database: t.db, web_root: ., views_dir: views }\ndrivers: [ { name: SIM, protocol: sim, poll_ms: 20 } ]\n").unwrap();
    std::fs::write(dir.join("project.yaml"), "security: { enabled: true }\ntags:\n\
- { name: T1, driver: SIM, address: t1, type: u16, writable: true, write_roles: [manutentore] }\n\
- { name: T2, driver: SIM, address: t2, type: u16, writable: true }\n").unwrap();
    for (id, roles) in [("v_op", "[\"operatore\"]"), ("v_man", "[\"manutentore\"]"), ("v_all", "[]")] {
        std::fs::write(dir.join(format!("views/{id}.json")), format!("{{\"title\":\"{id}\",\"roles\":{roles},\"operate_roles\":{roles},\"components\":[]}}")).unwrap();
    }
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, web::router(a)).await.unwrap() });
    let mut c = Client { base, http: reqwest::Client::new(), cookie: None };

    // senza login: API protette chiuse, login e info aperti
    assert_eq!(c.get("/api/tags").await.0, 401);
    assert_eq!(c.get("/api/info").await.0, 200);
    let (_, me) = c.get("/api/auth/me").await;
    assert_eq!((me["enabled"].clone(), me["user"].clone()), (json!(true), J::Null));

    // admin iniziale con password "admin" e cambio obbligatorio
    assert_eq!(c.post("/api/auth/login", json!({"username": "admin", "password": "sbagliata"})).await.0, 401);
    let (st, me) = c.post("/api/auth/login", json!({"username": "admin", "password": "admin"})).await;
    assert_eq!(st, 200);
    assert_eq!(me["must_change"], true);
    let (st, e) = c.get("/api/tags").await;
    assert_eq!((st, e["must_change"].clone()), (403, json!(true)));
    let (st, _) = c.post("/api/auth/password", json!({"old": "admin", "new": "Adm1n-nuova"})).await;
    assert_eq!(st, 200);
    assert_eq!(c.get("/api/tags").await.0, 200);

    // l'amministratore crea un operatore
    let (st, users) = c.put("/api/users/mario", json!({"full_name": "Mario Rossi", "roles": ["operatore"], "password": "op12345", "must_change": false})).await;
    assert_eq!(st, 200, "{users}");
    assert_eq!(c.put("/api/users/x", json!({"roles": ["inesistente"], "password": "abcdef"})).await.0, 400);
    // non si può lasciare il sistema senza amministratori
    assert_eq!(c.put("/api/users/admin", json!({"roles": ["operatore"]})).await.0, 400);

    // login operatore
    c.post("/api/auth/logout", json!({})).await;
    assert!(c.cookie.is_none());
    let (st, me) = c.post("/api/auth/login", json!({"username": "mario", "password": "op12345"})).await;
    assert_eq!(st, 200);
    assert!(me["permissions"].as_array().unwrap().contains(&json!("tag.write")));

    // viste: solo quelle dei propri ruoli
    let (_, views) = c.get("/api/views").await;
    let ids: Vec<&str> = views.as_array().unwrap().iter().filter_map(|v| v["id"].as_str()).collect();
    assert!(ids.contains(&"v_op") && ids.contains(&"v_all") && !ids.contains(&"v_man"), "{ids:?}");
    assert_eq!(c.get("/api/views/v_man").await.0, 403);
    assert_eq!(c.get("/api/views/v_op").await.0, 200);

    // scritture: T2 libero, T1 solo manutentore; comandi dalla vista v_man negati
    assert_eq!(c.post("/api/tags/write", json!({"tag": "T2", "value": 5})).await.0, 200);
    let (st, e) = c.post("/api/tags/write", json!({"tag": "T1", "value": 5})).await;
    assert_eq!(st, 403, "{e}");
    assert_eq!(c.post("/api/tags/write", json!({"tag": "T2", "value": 6, "view": "v_man"})).await.0, 403);
    let (_, tags) = c.get("/api/tags").await;
    let cw = |n: &str| tags.as_array().unwrap().iter().find(|t| t["name"] == n).unwrap()["can_write"].clone();
    assert_eq!((cw("T1"), cw("T2")), (json!(false), json!(true)));

    // niente Designer né gestione utenti per l'operatore
    assert_eq!(c.put("/api/views/nuova", json!({"components": []})).await.0, 403);
    assert_eq!(c.get("/api/users").await.0, 403);
    assert_eq!(c.put("/api/project", json!({})).await.0, 403);

    // tentativi ripetuti: blocco temporaneo
    c.post("/api/auth/logout", json!({})).await;
    for _ in 0..5 {
        c.post("/api/auth/login", json!({"username": "mario", "password": "no"})).await;
    }
    let (st, e) = c.post("/api/auth/login", json!({"username": "mario", "password": "op12345"})).await;
    assert_eq!(st, 401);
    assert!(e["error"].as_str().unwrap().contains("troppi tentativi"), "{e}");
}
