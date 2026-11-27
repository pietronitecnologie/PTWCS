//! Layer fisico: modello compilato da una vista, attuatori letti dai tag, sensori
//! scritti (fotocellula, scanner, contatore del pozzo), deviatore, heartbeat e
//! "simulazione attiva", protezione dei driver non abilitati alla simulazione.
use std::time::Duration;
use wcs_scada::{app, config::Config, value::Value};

async fn wait(cond: impl Fn() -> bool) -> bool {
    for _ in 0..200 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    false
}

const VIEW: &str = r##"{
  "title": "T", "width": 800, "height": 400, "physics": { "scale_mm": 10 },
  "components": [
    { "id": "A", "type": "conveyor", "x": 0, "y": 0, "w": 200, "h": 40,
      "physics": { "kind": "conveyor", "run": "A.Run", "next": "D", "speed": 1.0, "accel": 5, "decel": 5 } },
    { "id": "D", "type": "rect", "x": 200, "y": 0, "w": 20, "h": 40,
      "physics": { "kind": "diverter", "position": "D.Pos", "switch_s": 0.1,
                   "outputs": [ { "value": "1", "next": "B" }, { "value": "2", "next": "C" } ] } },
    { "id": "B", "type": "conveyor", "x": 220, "y": 0, "w": 100, "h": 40,
      "physics": { "kind": "conveyor", "run": "B.Run", "next": "K", "speed": 1.0, "accel": 5, "decel": 5 } },
    { "id": "C", "type": "conveyor", "x": 220, "y": 100, "w": 100, "h": 40,
      "physics": { "kind": "conveyor", "run": "B.Run", "next": "t2/E" } },
    { "id": "K", "type": "rect", "x": 330, "y": 0, "w": 20, "h": 40, "physics": { "kind": "sink", "count_tag": "Out" } },
    { "id": "PE", "type": "rect", "x": 150, "y": 10, "w": 20, "h": 20, "physics": { "kind": "photocell", "tag": "PE" } },
    { "id": "SC", "type": "rect", "x": 90, "y": -60, "w": 20, "h": 20,
      "physics": { "kind": "scanner", "on": "A", "at_m": 1.0, "code_tag": "Code", "count_tag": "Cnt" } },
    { "id": "SRC", "type": "label", "x": 0, "y": -30, "w": 60, "h": 20,
      "physics": { "kind": "source", "on": "A", "interval_min": 1.5, "barcodes": "T{n:3}", "max_items": 2 } },
    { "id": "BAD", "type": "rect", "x": 40, "y": 10, "w": 20, "h": 20, "physics": { "kind": "photocell", "tag": "P.Sensor" } }
  ]
}"##;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conveyors_sensors_diverter_and_protection() {
    let dir = std::env::temp_dir().join(format!("wcs_phys_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("views")).unwrap();
    std::fs::write(
        dir.join("plant.yaml"),
        "plant: { name: T }\nserver: { database: t.db, web_root: . }\ndrivers:\n- { name: SIM, protocol: sim, poll_ms: 20 }\n- { name: PLC, protocol: modbus, host: 127.0.0.1, port: 1, poll_ms: 500 }\n",
    )
    .unwrap();
    let tags: String = ["A.Run:bool", "B.Run:bool", "D.Pos:u16", "PE:bool", "Code:string", "Cnt:u16", "Out:u16", "Hb:u16", "Act:bool"]
        .iter()
        .map(|m| {
            let (n, t) = m.split_once(':').unwrap();
            format!("- {{ name: {n}, driver: SIM, address: {}, type: {t}, writable: true }}\n", n.to_lowercase().replace('.', "_"))
        })
        .collect();
    let project = format!(
        "tags:\n{tags}- {{ name: P.Sensor, driver: PLC, address: 'coil:5', type: bool, writable: true }}\nphysics:\n  step_ms: 10\n  heartbeat_tag: Hb\n  active_tag: Act\n"
    );
    std::fs::write(dir.join("project.yaml"), project).unwrap();
    std::fs::write(dir.join("views/t.json"), VIEW).unwrap();
    // seconda vista: il nastro C prosegue nel nastro E di un'altra pagina (riferimento vista/id)
    std::fs::write(
        dir.join("views/t2.json"),
        r#"{"title":"T2","width":400,"height":200,"components":[{"id":"E","type":"conveyor","x":0,"y":0,"w":100,"h":40,"physics":{"kind":"conveyor","run":"B.Run","sink":true}}]}"#,
    )
    .unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    // modello: il sensore sul driver PLC (non abilitato) è escluso con un errore chiaro
    let st = a.physics.status(&a);
    let errors: Vec<String> = serde_json::from_value(st["errors"].clone()).unwrap();
    assert!(errors.iter().any(|e| e.contains("P.Sensor") && e.contains("non è abilitato")), "{errors:?}");
    assert_eq!(st["components"].as_array().unwrap().len(), 9, "9 componenti validi (2 viste)");
    let c = st["components"].as_array().unwrap().iter().find(|c| c["key"] == "t/C").unwrap().clone();
    assert_eq!(c["next"], "t2/E", "collegamento tra viste");
    // la fotocellula senza "Montata su" è agganciata al nastro più vicino, nella posizione disegnata
    let pe = st["components"].as_array().unwrap().iter().find(|c| c["key"] == "t/PE").unwrap().clone();
    assert_eq!(pe["on"], "t/A");
    assert!((pe["at"].as_f64().unwrap() - 1.6).abs() < 1e-6, "{pe}");

    let v = |n: &str| a.db.value(n);
    a.physics.command(&a, "start", &serde_json::Value::Null).unwrap();
    a.physics.command(&a, "speed", &serde_json::json!({"factor": 4})).unwrap();
    assert!(wait(|| v("Act") == Value::Bool(true)).await, "simulazione attiva");
    // nastro fermo: le unità restano all'inizio, nessuna lettura
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(v("Cnt"), Value::Int(0));
    // il "PLC" (qui il test) avvia A e posiziona il deviatore sull'uscita 1
    a.db.write("D.Pos", Value::Int(1)).await.unwrap();
    a.db.write("A.Run", Value::Bool(true)).await.unwrap();
    assert!(wait(|| v("Cnt") == Value::Int(1)).await, "prima lettura");
    assert_eq!(v("Code"), Value::Str("T001".into()));
    assert!(wait(|| v("PE") == Value::Bool(true)).await, "fotocellula occupata");
    // B ferma: le unità si fermano a cavallo e la fotocellula resta occupata
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(v("Out"), Value::Int(0));
    a.db.write("B.Run", Value::Bool(true)).await.unwrap();
    assert!(wait(|| v("Out") == Value::Int(2)).await, "due unità al pozzo, Out = {:?}", v("Out"));
    assert_eq!(v("Code"), Value::Str("T002".into()));
    assert_eq!(v("PE"), Value::Bool(false));
    assert!(v("Hb").as_i64().unwrap() > 0, "heartbeat");

    // guasto: fotocellula bloccata occupata, poi arresto → simulazione non attiva
    a.physics.inject(&a, &serde_json::json!({"action": "photocell", "key": "t/PE", "stuck": true})).unwrap();
    assert!(wait(|| v("PE") == Value::Bool(true)).await);
    a.physics.command(&a, "stop", &serde_json::Value::Null).unwrap();
    assert!(wait(|| v("Act") == Value::Bool(false)).await, "simulazione non più attiva");
    let st = a.physics.status(&a);
    assert_eq!(st["stats"]["delivered"], 2);
    assert_eq!(st["stats"]["spawned"], 2);
}
