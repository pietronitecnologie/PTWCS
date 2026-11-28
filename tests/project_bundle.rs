//! File di progetto .ptwcs: esporta, nuovo progetto (con backup automatico), riapri dal file,
//! ripristino da backup; la sicurezza resta quella attuale se richiesto.
use std::time::Duration;
use wcs_scada::{app, bundle, config::Config};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn export_new_open_and_restore() {
    let dir = std::env::temp_dir().join(format!("wcs_bundle_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["views", "rules", "scripts", "symbols"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    std::fs::write(
        dir.join("plant.yaml"),
        "plant: { name: Prova }\nserver: { database: t.db, web_root: . }\nscripting: { python: python3, module: scripts }\ndrivers: [ { name: SIM, protocol: sim, poll_ms: 50 } ]\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("project.yaml"),
        "areas: [ { name: Linea } ]\ntags:\n- { name: A, driver: SIM, address: a, type: bool, writable: true, area: Linea }\n- { name: B, driver: SIM, address: b, type: u16, writable: true }\n",
    )
    .unwrap();
    std::fs::write(dir.join("views/main.json"), r#"{"title":"Main","width":800,"height":600,"components":[]}"#).unwrap();
    std::fs::write(dir.join("views/second.json"), r#"{"title":"Second","width":800,"height":600,"components":[]}"#).unwrap();
    std::fs::write(dir.join("scripts/hello.py"), "def hello(payload, ctx):\n    return {'ok': True}\n").unwrap();
    std::fs::write(dir.join("symbols/s1.svg"), r#"<svg viewBox="0 0 10 10"><rect id="body" width="10" height="10"/></svg>"#).unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // esporta: progetto + viste + script + simboli
    let b = bundle::export(&a, "Linea di prova", "test").unwrap();
    assert_eq!(b.format, bundle::FORMAT);
    assert_eq!(b.name, "Linea di prova");
    assert_eq!(b.project.tags.len(), 2);
    for f in ["views/main.json", "views/second.json", "scripts/hello.py", "symbols/s1.svg"] {
        assert!(b.files.contains_key(f), "manca {f}: {:?}", b.files.keys());
    }
    let saved = serde_json::to_string(&b).unwrap();

    // nuovo progetto: vuoto (una vista iniziale), backup automatico del precedente
    let out = bundle::import(&a, bundle::empty(&a, "Nuovo impianto"), true, "test", "prima-di-nuovo").await.unwrap();
    assert!(out.backup.ends_with("prima-di-nuovo.ptwcs"), "{}", out.backup);
    assert_eq!(a.cfg().project.tags.len(), 0);
    assert!(dir.join("views/home.json").exists() && !dir.join("views/main.json").exists());
    assert!(!dir.join("scripts/hello.py").exists(), "script del vecchio progetto rimossi");
    assert_eq!(bundle::list_backups(&a).len(), 1);

    // riapri il progetto dal file salvato
    let back: bundle::Bundle = serde_json::from_str(&saved).unwrap();
    let out = bundle::import(&a, back, true, "test", "prima-di-apri").await.unwrap();
    assert_eq!(out.views, 2);
    assert_eq!(a.cfg().project.tags.len(), 2);
    assert!(dir.join("views/main.json").exists() && !dir.join("views/home.json").exists());
    assert!(dir.join("scripts/hello.py").exists());
    assert!(a.db.def("A").is_some(), "tag di nuovo attivi");

    // ripristino del backup "prima-di-nuovo" (= progetto originale)
    let list = bundle::list_backups(&a);
    assert_eq!(list.len(), 2);
    let first = list.iter().find(|b| b.file.ends_with("prima-di-nuovo.ptwcs")).unwrap();
    let restored = bundle::read_backup(&a, &first.file).unwrap();
    assert_eq!(restored.project.tags.len(), 2);

    // file non validi: formato sbagliato, percorsi pericolosi, progetto non valido → nessuna modifica
    let mut bad = bundle::export(&a, "", "test").unwrap();
    bad.files.insert("views/../../etc.json".into(), "{}".into());
    assert!(bundle::import(&a, bad, true, "test", "x").await.is_err());
    let mut bad = bundle::export(&a, "", "test").unwrap();
    bad.project.tags[0].driver = "INESISTENTE".into();
    assert!(bundle::import(&a, bad, true, "test", "x").await.is_err());
    assert_eq!(a.cfg().project.tags.len(), 2, "il progetto non è cambiato");
    assert!(bundle::read_backup(&a, "../plant.yaml").is_err());
}
