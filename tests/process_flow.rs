//! Ricette (validazione, caricamento su unità) e sequenze a passi (transizioni,
//! @elapsed, @confirm, timeout, pausa, arresto con azioni di sicurezza), audit.
use std::collections::BTreeMap;
use std::time::Duration;
use wcs_scada::{app, config::Config, sequences::SeqStatus, value::Value};

async fn wait(cond: impl Fn() -> bool) -> bool {
    for _ in 0..150 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn recipes_and_sequences() {
    let dir = std::env::temp_dir().join(format!("wcs_proc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plant.yaml"), "plant: { name: T }\nserver: { database: t.db, web_root: . }\ndrivers: [ { name: SIM, protocol: sim, poll_ms: 20 } ]\n").unwrap();
    let tags: String = ["SP_Temp:f32", "SP_Level:f32", "Level:f32", "Fill:bool", "Heat:bool", "Go:bool"]
        .iter()
        .flat_map(|m| {
            let (n, t) = m.split_once(':').unwrap();
            ["R1", "R2"].map(|u| format!("- {{ name: {u}.{n}, driver: SIM, address: {u}_{n}, type: {t}, writable: true }}\n"))
        })
        .collect();
    const PROC: &str = r#"
recipe_types:
- name: Batch
  units: [R1, R2]
  parameters:
  - { name: Temp, tag: '{unit}.SP_Temp', min: 20, max: 90, default: 60 }
  - { name: Livello, tag: '{unit}.SP_Level', min: 0, max: 100, default: 50 }
sequences:
- name: S1
  unit: R1
  start_tag: R1.Go
  params: { recipe: '' }
  steps:
  - name: Ricetta
    actions: [ { kind: recipe, recipe_type: Batch, recipe: '{recipe}' } ]
    transitions: [ { next: '' } ]
  - name: Riempi
    actions: [ { kind: write, tag: '{unit}.Fill', value: true } ]
    transitions: [ { when: [ { tag: '{unit}.Level', op: '>=', value: 'tag:{unit}.SP_Level' } ] } ]
  - name: Attesa
    actions: [ { kind: write, tag: '{unit}.Fill', value: false } ]
    transitions: [ { when: [ { tag: '@elapsed', op: '>=', value: 0.3 } ] } ]
  - name: Conferma
    transitions: [ { when: [ { tag: '@confirm', op: '==', value: true } ], next: END } ]
    timeout_s: 30
  stop_actions: [ { kind: write, tag: '{unit}.Fill', value: false }, { kind: write, tag: '{unit}.Heat', value: false } ]
- name: S2
  unit: R2
  steps:
  - name: Lungo
    actions: [ { kind: write, tag: R2.Heat, value: true } ]
    transitions: [ { when: [ { tag: R2.Level, op: '>', value: 1000 } ] } ]
    timeout_s: 0.4
"#;
    std::fs::write(dir.join("project.yaml"), format!("tags:\n{tags}{PROC}")).unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let def = a.cfg().project.recipe_types[0].clone();

    // ricette: limiti, default, versioni
    let mut v = BTreeMap::new();
    v.insert("Temp".to_string(), Value::Float(95.0));
    assert!(a.recipes.save(&def, "Fuori", String::new(), &v, "t").is_err());
    v.insert("Temp".to_string(), Value::Float(72.5));
    let (r, ch) = a.recipes.save(&def, "Pane", "impasto".into(), &v, "t").unwrap();
    assert_eq!((r.version, r.values["Livello"].clone(), ch.len()), (1, Value::Int(50), 2));
    v.insert("Livello".to_string(), Value::Float(40.0));
    let (r, ch) = a.recipes.save(&def, "Pane", "impasto".into(), &v, "t").unwrap();
    assert_eq!((r.version, ch.len()), (2, 1));
    // caricamento su R2
    let res = a.recipes.load(&a.cfg(), &a.db, "Batch", "Pane", "R2", "t").await.unwrap();
    assert!(res.iter().all(|x| x.ok));
    assert!(wait(|| a.db.value("R2.SP_Temp").as_f64() == Some(72.5)).await);
    assert!(a.recipes.load(&a.cfg(), &a.db, "Batch", "Pane", "R9", "t").await.is_err());
    assert_eq!(a.recipes.active("Batch")["R2"].recipe, "Pane");

    // sequenza S1 avviata dal PLC (fronte di salita) senza ricetta: errore nel passo Ricetta
    a.db.write("R1.Go", Value::Bool(true)).await.unwrap();
    let st = |n: &str| a.sequencer.state(&a, n).unwrap();
    assert!(wait(|| st("S1").status == SeqStatus::Error).await, "{:?}", st("S1"));
    assert!(a.alarms.list().iter().any(|x| x.id == "SEQ.S1" && x.active));
    a.sequencer.command(&a, "S1", "reset", None, BTreeMap::new(), "t").await.unwrap();
    assert!(!a.alarms.list().iter().any(|x| x.id == "SEQ.S1" && x.active));

    // avvio con ricetta: carica R1, riempie fino al livello della ricetta, attende, chiede conferma
    let mut p = BTreeMap::new();
    p.insert("recipe".to_string(), "Pane".to_string());
    a.sequencer.command(&a, "S1", "start", None, p, "t").await.unwrap();
    assert!(wait(|| st("S1").step_name == "Riempi").await);
    assert!(wait(|| a.db.value("R1.SP_Level").as_f64() == Some(40.0) && a.db.value("R1.Fill") == Value::Bool(true)).await);
    // pausa: il passo non avanza anche se la condizione è vera
    a.sequencer.command(&a, "S1", "pause", None, BTreeMap::new(), "t").await.unwrap();
    a.db.write("R1.Level", Value::Float(45.0)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(st("S1").step_name, "Riempi");
    a.sequencer.command(&a, "S1", "resume", None, BTreeMap::new(), "t").await.unwrap();
    assert!(wait(|| st("S1").step_name == "Conferma").await, "{:?}", st("S1").step_name);
    assert_eq!(a.db.value("R1.Fill"), Value::Bool(false));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(st("S1").status, SeqStatus::Running, "attende la conferma");
    a.sequencer.command(&a, "S1", "confirm", None, BTreeMap::new(), "t").await.unwrap();
    assert!(wait(|| st("S1").status == SeqStatus::Done).await);
    let h: Vec<String> = st("S1").history.iter().map(|x| x.step.clone()).collect();
    assert_eq!(h, ["Ricetta", "Riempi", "Attesa", "Conferma"]);

    // S2: timeout senza passo alternativo → errore; poi riavvio e arresto con azioni di sicurezza
    a.sequencer.command(&a, "S2", "start", None, BTreeMap::new(), "t").await.unwrap();
    assert!(wait(|| st("S2").status == SeqStatus::Error).await);
    assert!(st("S2").message.contains("timeout"));
    a.sequencer.command(&a, "S1", "start", Some("Riempi"), BTreeMap::new(), "t").await.unwrap();
    a.db.write("R1.Heat", Value::Bool(true)).await.unwrap();
    a.db.write("R1.Level", Value::Float(0.0)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    a.sequencer.command(&a, "S1", "stop", None, BTreeMap::new(), "t").await.unwrap();
    assert_eq!(st("S1").status, SeqStatus::Stopped);
    assert!(wait(|| a.db.value("R1.Fill") == Value::Bool(false) && a.db.value("R1.Heat") == Value::Bool(false)).await);
    assert!(a.sequencer.command(&a, "S1", "pause", None, BTreeMap::new(), "t").await.is_err());

    // registro delle esecuzioni e audit (caricamento ricetta della sequenza)
    tokio::time::sleep(Duration::from_millis(200)).await;
    let runs = a.store.kv_recent("seq_runs", "S1/".into(), 10).await.unwrap();
    assert!(runs.len() >= 3);
    let (rows, total) = a.store.audit_query(wcs_scada::store::AuditFilter { to: i64::MAX, category: "ricette".into(), limit: 50, ..Default::default() }).await.unwrap();
    assert!(total >= 1 && rows.iter().any(|r| r.action == "recipe_load" && r.target == "Batch/Pane"), "{rows:?}");
}
