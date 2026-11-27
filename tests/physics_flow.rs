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

const FLUID_VIEW: &str = r##"{
  "title": "F", "width": 600, "height": 400,
  "components": [
    { "id": "W", "type": "label", "x": 0, "y": 0, "w": 60, "h": 20, "physics": { "kind": "supply", "pressure_bar": 2.0, "temp_c": 10 } },
    { "id": "V", "type": "symbol", "x": 80, "y": 0, "w": 40, "h": 40, "physics": { "kind": "valve", "cmd": "V.Open", "kv": 10, "time_s": 0.2, "fb_open": "V.IsOpen" } },
    { "id": "L", "type": "line", "x": 60, "y": 20, "w": 100, "h": 10,
      "physics": { "kind": "pipe", "from": "W", "to": "T", "elements": "V", "flow_tag": "F.Flow" } },
    { "id": "T", "type": "tank", "x": 200, "y": 0, "w": 80, "h": 140,
      "physics": { "kind": "tank", "volume_m3": 1.0, "height_m": 1.0, "level_tag": "T.Level", "temp_tag": "T.Temp", "init_temp": 30 } },
    { "id": "H", "type": "label", "x": 200, "y": 150, "w": 60, "h": 20, "physics": { "kind": "heater", "on": "T", "cmd": "H.On", "power_kw": 500 } }
  ]
}"##;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fluids_fill_heat_with_tags() {
    let dir = std::env::temp_dir().join(format!("wcs_fluid_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("views")).unwrap();
    std::fs::write(dir.join("plant.yaml"), "plant: { name: T }\nserver: { database: t.db, web_root: . }\ndrivers: [ { name: SIM, protocol: sim, poll_ms: 20 } ]\n").unwrap();
    let tags: String = ["V.Open:bool", "V.IsOpen:bool", "F.Flow:f32", "T.Level:f32", "T.Temp:f32", "H.On:bool"]
        .iter()
        .map(|m| {
            let (n, t) = m.split_once(':').unwrap();
            format!("- {{ name: {n}, driver: SIM, address: {}, type: {t}, writable: true }}\n", n.to_lowercase().replace('.', "_"))
        })
        .collect();
    std::fs::write(dir.join("project.yaml"), format!("tags:\n{tags}")).unwrap();
    std::fs::write(dir.join("views/f.json"), FLUID_VIEW).unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    let st = a.physics.status(&a);
    assert!(st["errors"].as_array().unwrap().is_empty(), "{}", st["errors"]);
    let v = |n: &str| a.db.value(n).as_f64().unwrap_or(-1.0);
    a.physics.command(&a, "start", &serde_json::Value::Null).unwrap();
    a.physics.command(&a, "speed", &serde_json::json!({"factor": 10})).unwrap();
    a.db.write("V.Open", Value::Bool(true)).await.unwrap();
    assert!(wait(|| a.db.value("V.IsOpen") == Value::Bool(true)).await, "finecorsa aperta");
    // ~14 m³/h a 2 bar: il serbatoio da 1 m³ si riempie; l'acqua a 10 °C raffredda il contenuto
    assert!(wait(|| v("F.Flow") > 10.0).await, "portata {}", v("F.Flow"));
    assert!(wait(|| v("T.Level") > 20.0).await, "livello {}", v("T.Level"));
    a.db.write("V.Open", Value::Bool(false)).await.unwrap();
    assert!(wait(|| v("F.Flow") == 0.0).await, "portata nulla a valvola chiusa");
    let (l, t0) = (v("T.Level"), v("T.Temp"));
    assert!(t0 < 20.0, "miscelazione con acqua fredda: {t0}");
    a.db.write("H.On", Value::Bool(true)).await.unwrap();
    assert!(wait(|| v("T.Temp") > t0 + 3.0).await, "riscaldamento {} → {}", t0, v("T.Temp"));
    assert!((v("T.Level") - l).abs() < 0.5, "livello stabile a valvola chiusa");
    // guasto: perdita dal serbatoio
    a.physics.inject(&a, &serde_json::json!({"action": "tank", "key": "f/T", "leak": 20.0})).unwrap();
    assert!(wait(|| v("T.Level") < l - 5.0).await, "la perdita fa scendere il livello");
}
