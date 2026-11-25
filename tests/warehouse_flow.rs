//! Test del magazzino automatico: allocazione, telegramma missioni Start/Busy/Done
//! verso il traslo (simulato scrivendo i tag lato PLC), deposito e prelievo.
use std::time::Duration;
use wcs_scada::{app, config::Config, value::Value};

async fn wait(cond: impl Fn() -> bool) -> bool {
    for _ in 0..100 {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

#[tokio::test(flavor = "multi_thread")]
async fn inbound_outbound_with_crane_protocol() {
    let dir = std::env::temp_dir().join(format!("wcs_wh_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plant.yaml"), "plant: { name: T }\nserver: { database: t.db, web_root: . }\ndrivers: [ { name: SIM, protocol: sim, poll_ms: 20 } ]\n").unwrap();
    let members: String = ["Mission:u32", "Type:u16", "SrcSide:u16", "SrcCol:u16", "SrcLvl:u16", "DstSide:u16", "DstCol:u16", "DstLvl:u16", "DstOut:u16", "Start:bool", "Busy:bool", "Done:u32", "Fault:bool", "Auto:bool"]
        .iter()
        .map(|m| {
            let (n, t) = m.split_once(':').unwrap();
            format!("  - {{ name: {n}, type: {t}, address: '{{Instance}}.{n}', writable: true }}\n")
        })
        .collect();
    std::fs::write(
        dir.join("project.yaml"),
        format!(
            "udt_types:\n- name: Tr\n  members:\n{members}udt_instances:\n- {{ name: K1, type: Tr, driver: SIM }}\n- {{ name: K2, type: Tr, driver: SIM }}\n\
warehouses:\n- {{ name: W, aisles: [ {{ name: '1', crane: K1 }}, {{ name: '2', crane: K2 }} ], sides: 2, columns: 3, levels: 2, outputs: [B1, B2] }}\n"
        ),
    )
    .unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    let w = a.warehouse("W").expect("magazzino aperto");
    assert_eq!(w.total(), 24);
    tokio::time::sleep(Duration::from_millis(150)).await;

    // corsia 2 non disponibile (traslo in guasto): il deposito va in corsia 1, cella più vicina al P&D
    a.db.write("K1.Auto", Value::Bool(true)).await.unwrap();
    a.db.write("K2.Auto", Value::Bool(true)).await.unwrap();
    a.db.write("K2.Fault", Value::Bool(true)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let (aisle, pos) = w.allocate_inbound("LU1", "IN1", &w.unavailable_aisles(&a.db), false).unwrap();
    assert_eq!((aisle, pos.col, pos.lvl), (1, 1, 1));
    assert!(w.allocate_inbound("LU1", "IN1", &[], false).is_err(), "duplicato rifiutato");

    // il WCS invia la missione al traslo K1
    assert!(wait(|| a.db.value("K1.Start") == Value::Bool(true)).await, "Start non inviato");
    assert_eq!(a.db.value("K1.Type"), Value::Int(1));
    assert_eq!(a.db.value("K1.DstCol"), Value::Int(1));
    let id = a.db.value("K1.Mission");
    // PLC: presa in carico -> il WCS azzera Start
    a.db.write("K1.Busy", Value::Bool(true)).await.unwrap();
    assert!(wait(|| a.db.value("K1.Start") == Value::Bool(false)).await, "Start non azzerato");
    // PLC: missione completata
    a.db.write("K1.Done", id.clone()).await.unwrap();
    a.db.write("K1.Busy", Value::Bool(false)).await.unwrap();
    assert!(wait(|| w.find("LU1").is_some_and(|(_, l)| l.state == wcs_scada::warehouse::LocState::Occupied)).await, "cella non occupata");

    // prelievo verso una baia
    let m = w.create_outbound(Some("LU1"), Some("B2"), &[]).unwrap();
    assert!(wait(|| a.db.value("K1.Start") == Value::Bool(true) && a.db.value("K1.Mission") == Value::Int(m.id as i64)).await);
    assert_eq!(a.db.value("K1.DstOut"), Value::Int(2));
    a.db.write("K1.Busy", Value::Bool(true)).await.unwrap();
    assert!(wait(|| a.db.value("K1.Start") == Value::Bool(false)).await);
    a.db.write("K1.Done", Value::Int(m.id as i64)).await.unwrap();
    a.db.write("K1.Busy", Value::Bool(false)).await.unwrap();
    assert!(wait(|| w.find("LU1").is_none()).await, "UdC non uscita");
    let s = w.summary();
    assert_eq!(s["totals"]["occupied"], 0);
    assert_eq!(s["missions"]["in_hour"], 1);
    assert_eq!(s["missions"]["out_hour"], 1);
}
