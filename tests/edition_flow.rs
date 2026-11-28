//! Edizioni del runtime: i moduli esclusi non partono e le loro API rispondono 404.
use serde_json::Value as J;
use wcs_scada::{app, config::Config, web};

async fn serve(edition: &str) -> (String, std::sync::Arc<app::AppState>) {
    let dir = std::env::temp_dir().join(format!("wcs_ed_{}_{edition}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("views")).unwrap();
    std::fs::write(
        dir.join("plant.yaml"),
        format!("edition: {edition}\nplant: {{ name: T }}\nserver: {{ database: t.db, web_root: ., views_dir: views }}\ndrivers: [ {{ name: SIM, protocol: sim, poll_ms: 20 }} ]\n"),
    )
    .unwrap();
    std::fs::write(dir.join("project.yaml"), "tags:\n- { name: T1, driver: SIM, address: t1, type: u16, writable: true }\n").unwrap();
    std::fs::write(dir.join("views/home.json"), r#"{"title":"Home","home":true,"components":[]}"#).unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let r = web::router(a.clone());
    tokio::spawn(async move { axum::serve(listener, r).await.unwrap() });
    (base, a)
}

async fn status(base: &str, path: &str) -> u16 {
    reqwest::get(format!("{base}{path}")).await.unwrap().status().as_u16()
}

#[tokio::test(flavor = "multi_thread")]
async fn editions_gate_modules() {
    // Tag + WCS: niente viste, processo, simulazione
    let (b, a) = serve("wcs").await;
    assert_eq!(a.edition.name, "wcs");
    let info: J = reqwest::get(format!("{b}/api/info")).await.unwrap().json().await.unwrap();
    assert_eq!(info["edition"]["modules"]["wcs"], true);
    assert_eq!(info["edition"]["modules"]["scada"], false);
    for p in ["/api/views", "/api/proc", "/api/physics", "/api/recipes"] {
        assert_eq!(status(&b, p).await, 404, "{p}");
    }
    for p in ["/api/tags", "/api/wh", "/api/decisions", "/api/alarms", "/api/trend?tag=T1&from=0&to=1"] {
        assert_eq!(status(&b, p).await, 200, "{p}");
    }
    let r: J = reqwest::get(format!("{b}/api/views")).await.unwrap().json().await.unwrap();
    assert!(r["error"].as_str().unwrap().contains("Tag + WCS"), "{r}");

    // Tag + SCADA + simulazione: niente WCS
    let (b, a) = serve("scada-sim").await;
    assert_eq!(a.edition.label, "Tag + SCADA + Simulazione");
    for p in ["/api/views", "/api/proc", "/api/physics"] {
        assert_eq!(status(&b, p).await, 200, "{p}");
    }
    for p in ["/api/wh", "/api/decisions", "/api/rules", "/api/sorting/status"] {
        assert_eq!(status(&b, p).await, 404, "{p}");
    }

    // Tag + SCADA
    let (b, _) = serve("tag+scada").await;
    assert_eq!(status(&b, "/api/views").await, 200);
    assert_eq!(status(&b, "/api/physics").await, 404);
    assert_eq!(status(&b, "/api/wh").await, 404);

    // edizione non valida: avvio rifiutato
    let dir = std::env::temp_dir().join(format!("wcs_ed_bad_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plant.yaml"), "edition: erp\nplant: { name: T }\nserver: { database: t.db, web_root: . }\ndrivers: []\n").unwrap();
    let p = dir.join("plant.yaml");
    assert!(app::start(Config::load_base(&p).unwrap(), Config::load(&p).unwrap()).await.is_err());
}
