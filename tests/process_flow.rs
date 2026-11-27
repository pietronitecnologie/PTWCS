//! Processo ISA-88: fasi (sequenze SFC con parametri), ricette che schedulano le
//! fasi anche in parallelo, allocazione esclusiva dei dispositivi, stati
//! hold/restart/abort, errore e nuovo tentativo, rami paralleli in una sequenza.
use std::collections::BTreeMap;
use std::time::Duration;
use wcs_scada::{app, app::AppState, config::Config, recipes::RecipeDef, sequences::RunState, value::Value};

async fn wait(cond: impl Fn() -> bool) -> bool {
    for _ in 0..150 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    false
}

const PROJECT: &str = r#"
devices:
- { name: R1, class: Reattore }
- { name: R2, class: Reattore }
sequences:
- name: Carico
  device_class: Reattore
  parameters: [ { name: Livello, min: 0, max: 100, default: 50, unit: '%' } ]
  steps:
  - name: Riempi
    actions: [ { kind: write, tag: '{unit}.Fill', value: true } ]
    transitions: [ { when: [ { tag: '{unit}.Level', op: '>=', value: '{Livello}' } ] } ]
    timeout_s: 30
  - name: Chiudi
    actions: [ { kind: write, tag: '{unit}.Fill', value: false } ]
  hold_actions: [ { kind: write, tag: '{unit}.Fill', value: false } ]
  restart_actions: [ { kind: write, tag: '{unit}.Fill', value: true } ]
  stop_actions: [ { kind: write, tag: '{unit}.Fill', value: false } ]
- name: Riscaldo
  device_class: Reattore
  parameters: [ { name: Temp, min: 20, max: 90 } ]
  steps:
  - name: Scalda
    actions: [ { kind: write, tag: '{unit}.Heat', value: true } ]
    transitions: [ { when: [ { tag: '{unit}.Temp', op: '>=', value: '{Temp}' } ] } ]
    timeout_s: 0.6
  - name: Spegni
    actions: [ { kind: write, tag: '{unit}.Heat', value: false } ]
- name: Agita
  device_class: Reattore
  parameters: [ { name: Tempo, default: 0.3 } ]
  steps:
  - name: Gira
    actions: [ { kind: write, tag: '{unit}.Agit', value: true } ]
    transitions: [ { when: [ { tag: '@elapsed', op: '>=', value: '{Tempo}' } ] } ]
  - name: Ferma
    actions: [ { kind: write, tag: '{unit}.Agit', value: false } ]
- name: Doppio
  steps:
  - name: Insieme
    branches:
    - - { name: A, actions: [ { kind: write, tag: X1, value: true } ], transitions: [ { when: [ { tag: '@elapsed', op: '>=', value: 0.2 } ] } ] }
    - - { name: B1, actions: [ { kind: write, tag: X2, value: true } ], transitions: [ { when: [ { tag: X1, op: '==', value: true } ] } ] }
      - { name: B2, transitions: [ { when: [ { tag: '@confirm', op: '==', value: true } ] } ] }
  - name: Dopo
    actions: [ { kind: write, tag: X1, value: false } ]
"#;

fn recipe() -> RecipeDef {
    serde_yaml::from_str(
        r#"
name: Batch
devices: [ { alias: reattore, class: Reattore, default: R1 } ]
parameters:
- { name: Livello, min: 10, max: 95, default: 60 }
- { name: Temp, min: 25, max: 90, default: 40 }
steps:
- name: Carica
  actions: [ { kind: phase, sequence: Carico, device: '{reattore}', params: { Livello: '{Livello}' } } ]
- name: Scalda e agita
  branches:
  - - { name: Calore, actions: [ { kind: phase, sequence: Riscaldo, device: '{reattore}', params: { Temp: '{Temp}' } } ] }
  - - { name: Agitazione, actions: [ { kind: phase, sequence: Agita, device: '{reattore}' } ] }
- name: Controllo
  transitions: [ { when: [ { tag: '@confirm', op: '==', value: true } ], next: END } ]
"#,
    )
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn phases_recipes_batches_and_allocation() {
    let dir = std::env::temp_dir().join(format!("wcs_proc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plant.yaml"), "plant: { name: T }\nserver: { database: t.db, web_root: . }\ndrivers: [ { name: SIM, protocol: sim, poll_ms: 20 } ]\n").unwrap();
    let mut tags: String = ["Level:f32", "Temp:f32", "Fill:bool", "Heat:bool", "Agit:bool"]
        .iter()
        .flat_map(|m| {
            let (n, t) = m.split_once(':').unwrap();
            ["R1", "R2"].map(|u| format!("- {{ name: {u}.{n}, driver: SIM, address: {u}_{n}, type: {t}, writable: true }}\n"))
        })
        .collect();
    tags += "- { name: X1, driver: SIM, address: x1, type: bool, writable: true }\n- { name: X2, driver: SIM, address: x2, type: bool, writable: true }\n";
    std::fs::write(dir.join("project.yaml"), format!("tags:\n{tags}{PROJECT}")).unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let st = |a: &AppState, k: &str| {
        let s = a.sequencer.clone_state(k);
        s.map(|i| i.state).unwrap_or(RunState::Idle)
    };
    let none = BTreeMap::new;
    let p = |k: &str, v: &str| BTreeMap::from([(k.to_string(), v.to_string())]);

    // 1. fase manuale con parametro: limiti verificati, scrive e attende il livello
    assert!(a.sequencer.start_phase(&a, "Carico", "R1", p("Livello", "150"), "t").await.is_err());
    assert!(a.sequencer.start_phase(&a, "Carico", "", none(), "t").await.is_err(), "serve il dispositivo");
    a.sequencer.start_phase(&a, "Carico", "R1", p("Livello", "30"), "t").await.unwrap();
    assert!(wait(|| a.db.value("R1.Fill") == Value::Bool(true)).await);
    a.db.write("R1.Level", Value::Float(35.0)).await.unwrap();
    assert!(wait(|| st(&a, "Carico@R1") == RunState::Complete).await);
    assert_eq!(a.db.value("R1.Fill"), Value::Bool(false));
    a.sequencer.command(&a, "Carico@R1", "reset", None, "t").await.unwrap();

    // 2. ricetta: validazione e salvataggio con versioni
    let (r, _) = a.recipes.save(&a.cfg(), recipe(), "t").unwrap();
    assert_eq!(r.version, 1);
    let mut bad = recipe();
    bad.steps[0].actions[0].params.insert("Inesistente".into(), "1".into());
    assert!(a.recipes.save(&a.cfg(), bad, "t").is_err());

    // 3. batch su R1: allocazione esclusiva
    a.db.write("R1.Level", Value::Float(0.0)).await.unwrap();
    let b = a.sequencer.start_batch(&a, &r, none(), p("Temp", "45"), "LOTTO-1", "t").await.unwrap();
    let id = b.key.clone();
    assert_eq!(b.devices["reattore"], "R1");
    let e = a.sequencer.start_phase(&a, "Carico", "R1", none(), "t").await.unwrap_err();
    assert!(e.to_string().contains("allocato"), "{e}");
    assert!(a.sequencer.start_batch(&a, &r, none(), none(), "", "t").await.is_err(), "R1 occupato");
    // la fase del batch non si comanda direttamente
    assert!(wait(|| st(&a, "Carico@R1") == RunState::Running).await);
    assert!(a.sequencer.command(&a, "Carico@R1", "stop", None, "t").await.is_err());
    assert!(wait(|| a.db.value("R1.Fill") == Value::Bool(true)).await, "Livello della ricetta passato alla fase");

    // 4. hold e restart si propagano alle fasi (azioni di stato)
    a.sequencer.command(&a, &id, "hold", None, "t").await.unwrap();
    assert_eq!(st(&a, "Carico@R1"), RunState::Held);
    assert!(wait(|| a.db.value("R1.Fill") == Value::Bool(false)).await, "hold_actions");
    a.db.write("R1.Level", Value::Float(70.0)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(st(&a, "Carico@R1"), RunState::Held, "in hold non avanza");
    a.sequencer.command(&a, &id, "restart", None, "t").await.unwrap();
    assert!(wait(|| st(&a, "Carico@R1") == RunState::Complete).await);

    // 5. fasi in parallelo; il riscaldamento va in timeout → batch in errore, restart riprova
    assert!(wait(|| st(&a, "Riscaldo@R1") == RunState::Running && st(&a, "Agita@R1") != RunState::Idle).await);
    assert!(wait(|| st(&a, "Riscaldo@R1") == RunState::Error).await);
    assert!(wait(|| st(&a, &id) == RunState::Error).await);
    assert!(a.alarms.list().iter().any(|x| x.id == format!("SEQ.{id}") && x.active));
    assert!(wait(|| st(&a, "Agita@R1") == RunState::Complete).await, "l'altro ramo prosegue");
    a.db.write("R1.Temp", Value::Float(50.0)).await.unwrap();
    a.sequencer.command(&a, &id, "restart", None, "t").await.unwrap();
    assert!(wait(|| st(&a, "Riscaldo@R1") == RunState::Complete).await);
    let ok = wait(|| a.sequencer.clone_state(&id).is_some_and(|b| b.cursor.step == "Controllo")).await;
    assert!(ok, "convergenza dei rami");
    let runs_agita = a.sequencer.clone_state("Agita@R1").unwrap().started;
    a.sequencer.command(&a, &id, "confirm", None, "t").await.unwrap();
    assert!(wait(|| st(&a, &id) == RunState::Complete).await);
    assert_eq!(a.sequencer.clone_state("Agita@R1").unwrap().started, runs_agita, "la fase già conclusa non è stata ripetuta");
    // dispositivi rilasciati
    a.sequencer.command(&a, &id, "reset", None, "t").await.unwrap();
    a.sequencer.start_phase(&a, "Agita", "R1", p("Tempo", "0.1"), "t").await.unwrap();
    assert!(wait(|| st(&a, "Agita@R1") == RunState::Complete).await);

    // 6. abort: azioni di arresto e rilascio
    a.db.write("R2.Level", Value::Float(0.0)).await.unwrap();
    let b2 = a.sequencer.start_batch(&a, &r, BTreeMap::from([("reattore".to_string(), "R2".to_string())]), none(), "", "t").await.unwrap();
    assert!(wait(|| a.db.value("R2.Fill") == Value::Bool(true)).await);
    a.sequencer.command(&a, &b2.key, "abort", None, "t").await.unwrap();
    assert_eq!(st(&a, &b2.key), RunState::Aborted);
    assert_eq!(st(&a, "Carico@R2"), RunState::Aborted);
    assert!(wait(|| a.db.value("R2.Fill") == Value::Bool(false)).await, "stop_actions usate come abort");
    a.sequencer.start_phase(&a, "Agita", "R2", none(), "t").await.unwrap();

    // 7. rami paralleli in una sequenza, convergenza
    a.sequencer.start_phase(&a, "Doppio", "", none(), "t").await.unwrap();
    assert!(wait(|| a.db.value("X1") == Value::Bool(true) && a.db.value("X2") == Value::Bool(true)).await);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(a.sequencer.clone_state("Doppio").unwrap().cursor.step, "Insieme", "il ramo B attende la conferma");
    a.sequencer.command(&a, "Doppio", "confirm", None, "t").await.unwrap();
    assert!(wait(|| st(&a, "Doppio") == RunState::Complete).await);
    assert_eq!(a.db.value("X1"), Value::Bool(false));

    // registro dei batch
    tokio::time::sleep(Duration::from_millis(200)).await;
    let rec = a.store.kv_recent("batches", String::new(), 10).await.unwrap();
    assert_eq!(rec.len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn batches_survive_server_restart() {
    let dir = std::env::temp_dir().join(format!("wcs_proc2_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plant.yaml"), "plant: { name: T }\nserver: { database: t.db, web_root: . }\ndrivers: [ { name: SIM, protocol: sim, poll_ms: 20 } ]\n").unwrap();
    let tags: String = ["Level:f32", "Temp:f32", "Fill:bool", "Heat:bool", "Agit:bool"]
        .iter()
        .flat_map(|m| {
            let (n, t) = m.split_once(':').unwrap();
            ["R1", "R2"].map(|u| format!("- {{ name: {u}.{n}, driver: SIM, address: {u}_{n}, type: {t}, writable: true }}\n"))
        })
        .collect();
    std::fs::write(dir.join("project.yaml"), format!("tags:\n{tags}- {{ name: X1, driver: SIM, address: x1, type: bool, writable: true }}\n- {{ name: X2, driver: SIM, address: x2, type: bool, writable: true }}\n{PROJECT}")).unwrap();
    let path = dir.join("plant.yaml");
    let id = {
        let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
        let (r, _) = a.recipes.save(&a.cfg(), recipe(), "t").unwrap();
        let b = a.sequencer.start_batch(&a, &r, BTreeMap::new(), BTreeMap::new(), "", "t").await.unwrap();
        assert!(wait(|| a.sequencer.clone_state("Carico@R1").is_some_and(|i| i.state == RunState::Running)).await);
        tokio::time::sleep(Duration::from_millis(1500)).await; // persistenza (ogni secondo)
        b.key
    };
    // "riavvio": nuova istanza dell'applicazione sullo stesso database
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(400)).await;
    let (inst, alloc) = a.sequencer.snapshot(&a).await;
    let b = inst.iter().find(|i| i.key == id).expect("batch ripristinato");
    assert_eq!(b.state, RunState::Held);
    assert_eq!(alloc.get("R1"), Some(&id), "allocazione ripristinata");
    assert!(a.sequencer.start_phase(&a, "Carico", "R1", BTreeMap::new(), "t").await.is_err());
    a.sequencer.command(&a, &id, "restart", None, "t").await.unwrap();
    assert!(wait(|| a.sequencer.clone_state("Carico@R1").is_some_and(|i| i.state == RunState::Running)).await);
    a.sequencer.command(&a, &id, "abort", None, "t").await.unwrap();
}
