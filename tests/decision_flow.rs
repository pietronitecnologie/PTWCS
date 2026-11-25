//! Test end-to-end del decision engine: progetto con UDT telegramma su driver
//! simulato, handshake a flag, regole grafiche con classi di smistamento.
use std::time::Duration;
use wcs_scada::{app, config::Config, value::Value};

async fn wait_for(app: &app::AppState, tag: &str, v: Value) -> bool {
    for _ in 0..100 {
        if app.db.value(tag) == v {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn flag_handshake_with_rules_and_classes() {
    let dir = std::env::temp_dir().join(format!("wcs_flow_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("rules")).unwrap();
    std::fs::write(
        dir.join("plant.yaml"),
        r#"
plant: { name: Test }
server: { database: test.db, web_root: ., views_dir: views, symbols_dir: symbols }
drivers: [ { name: SIM, protocol: sim, poll_ms: 20 } ]
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("project.yaml"),
        r#"
areas: [ { name: Test } ]
udt_types:
- name: Tel
  params: [ { name: P, default: tel } ]
  members:
  - { name: Req, type: bool, address: '{P}.req', role: request, writable: true }
  - { name: Barcode, type: string, address: '{P}.bc', role: input, writable: true }
  - { name: Ack, type: bool, address: '{P}.ack', role: response, writable: true }
  - { name: Dest, type: u16, address: '{P}.dest', role: output, writable: true }
udt_instances: [ { name: T1, type: Tel, driver: SIM, area: Test } ]
decision_points: [ { name: T1, telegram: T1, handshake: flag, fallback: { dest: 99 } } ]
sort_classes: [ { name: SUD, destination: '2', patterns: ['IT-NA*'] } ]
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("rules/T1.json"),
        r#"{"rules":[{"id":"a","name":"classe","when":[{"field":"barcode","op":"has_class"}],"then":{"outputs":{"dest":"class:barcode"}}}],
            "default":{"outputs":{"dest":"7"}}}"#,
    )
    .unwrap();
    let path = dir.join("plant.yaml");
    let base = Config::load_base(&path).unwrap();
    let cfg = Config::load(&path).unwrap();
    let app = app::start(base, cfg).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // collo appartenente a una classe -> uscita della classe
    app.db.write("T1.Barcode", Value::Str("IT-NA-1".into())).await.unwrap();
    app.db.write("T1.Req", Value::Bool(true)).await.unwrap();
    assert!(wait_for(&app, "T1.Ack", Value::Bool(true)).await, "ack non ricevuto");
    assert_eq!(app.db.value("T1.Dest"), Value::Int(2));
    // il PLC ritira la richiesta -> il WCS azzera l'ack
    app.db.write("T1.Req", Value::Bool(false)).await.unwrap();
    assert!(wait_for(&app, "T1.Ack", Value::Bool(false)).await, "ack non azzerato");

    // collo sconosciuto -> default delle regole
    app.db.write("T1.Barcode", Value::Str("ZZ".into())).await.unwrap();
    app.db.write("T1.Req", Value::Bool(true)).await.unwrap();
    assert!(wait_for(&app, "T1.Ack", Value::Bool(true)).await);
    assert_eq!(app.db.value("T1.Dest"), Value::Int(7));

    let recs = app.store.decisions(Some("T1".into()), 10).await.unwrap();
    assert_eq!(recs.len(), 2);
    assert_eq!(recs[1].rule.as_deref(), Some("classe"));
    assert_eq!(recs[0].rule.as_deref(), Some("Default"));
}

#[test]
fn demo_config_is_valid() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config/plant.yaml");
    let cfg = Config::load(&path).expect("config demo valida");
    assert!(cfg.tags.len() > 20);
    assert!(cfg.decision_points.iter().any(|d| d.name == "SC1" && d.request_tag == "SC1.ReqID"));
}
