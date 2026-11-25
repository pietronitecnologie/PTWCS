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

#[tokio::test(flavor = "multi_thread")]
async fn double_lu_crane_pairs_two_missions() {
    let dir = std::env::temp_dir().join(format!("wcs_wh2_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("plant.yaml"), "plant: { name: T }\nserver: { database: t.db, web_root: . }\ndrivers: [ { name: SIM, protocol: sim, poll_ms: 20 } ]\n").unwrap();
    let mut names: Vec<String> = ["Mission:u32", "Type:u16", "SrcSide:u16", "SrcCol:u16", "SrcLvl:u16", "DstSide:u16", "DstCol:u16", "DstLvl:u16", "DstOut:u16", "Done:u32"]
        .iter().flat_map(|m| [m.to_string(), m.replacen(':', "2:", 1)]).collect();
    names.extend(["Start:bool", "Busy:bool", "Fault:bool", "Auto:bool"].map(String::from));
    let members: String = names.iter().map(|m| {
        let (n, t) = m.split_once(':').unwrap();
        format!("  - {{ name: {n}, type: {t}, address: '{{Instance}}.{n}', writable: true }}\n")
    }).collect();
    std::fs::write(dir.join("project.yaml"), format!(
        "udt_types:\n- name: Tr2\n  members:\n{members}udt_instances:\n- {{ name: D1, type: Tr2, driver: SIM }}\n\
warehouses:\n- {{ name: W, aisles: [ {{ name: '1', crane: D1, slots: 2, pair_wait_s: 10 }} ], sides: 2, columns: 5, levels: 2, outputs: [B1] }}\n")).unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    let w = a.warehouse("W").unwrap();
    a.db.write("D1.Auto", Value::Bool(true)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;

    // una sola missione: il traslo doppio attende la compagna (pair_wait_s)
    w.allocate_inbound("A", "IN1", &[], false).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(a.db.value("D1.Start"), Value::Bool(false), "deve attendere la seconda missione");
    // seconda missione: ciclo doppio
    w.allocate_inbound("B", "IN2", &[], false).unwrap();
    assert!(wait(|| a.db.value("D1.Start") == Value::Bool(true)).await, "ciclo doppio non inviato");
    let (m1, m2) = (a.db.value("D1.Mission"), a.db.value("D1.Mission2"));
    assert_ne!(m2, Value::Int(0));
    assert_eq!((a.db.value("D1.Type"), a.db.value("D1.Type2")), (Value::Int(1), Value::Int(1)));
    a.db.write("D1.Busy", Value::Bool(true)).await.unwrap();
    assert!(wait(|| a.db.value("D1.Start") == Value::Bool(false)).await);
    // il posto 2 termina per primo, poi il posto 1
    a.db.write("D1.Done2", m2).await.unwrap();
    assert!(wait(|| w.find("B").is_some_and(|(_, l)| l.state == wcs_scada::warehouse::LocState::Occupied)).await);
    assert_eq!(w.find("A").unwrap().1.state, wcs_scada::warehouse::LocState::Reserved);
    a.db.write("D1.Done", m1).await.unwrap();
    a.db.write("D1.Busy", Value::Bool(false)).await.unwrap();
    assert!(wait(|| w.find("A").is_some_and(|(_, l)| l.state == wcs_scada::warehouse::LocState::Occupied)).await);
    assert_eq!(w.summary()["missions"]["in_hour"], 2);
}

/// PLC del traslo simulato: Start → Busy, poi Done = Mission (posto singolo).
fn crane_plc(a: std::sync::Arc<app::AppState>, crane: &'static str) {
    tokio::spawn(async move {
        let mut cur: Option<Value> = None;
        loop {
            tokio::time::sleep(Duration::from_millis(25)).await;
            let t = |m: &str| format!("{crane}.{m}");
            let start = a.db.value(&t("Start")) == Value::Bool(true);
            let busy = a.db.value(&t("Busy")) == Value::Bool(true);
            if start && !busy {
                cur = Some(a.db.value(&t("Mission")));
                let _ = a.db.write(&t("Busy"), Value::Bool(true)).await;
            } else if busy && !start {
                if let Some(m) = cur.take() {
                    let _ = a.db.write(&t("Done"), m).await;
                }
                let _ = a.db.write(&t("Busy"), Value::Bool(false)).await;
            }
        }
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn wcs_abc_orders_priorities_and_reorg() {
    use wcs_scada::warehouse::{LuInfo, OrderLineReq, OrderReq};
    let dir = std::env::temp_dir().join(format!("wcs_wh3_{}", std::process::id()));
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
            "udt_types:\n- name: Tr\n  members:\n{members}udt_instances:\n- {{ name: K1, type: Tr, driver: SIM }}\n\
warehouses:\n- {{ name: W, aisles: [ {{ name: '1', crane: K1 }} ], sides: 1, columns: 10, levels: 2, outputs: [B1], \
strategy: {{ abc: true, a_zone_pct: 0.2, b_zone_pct: 0.3 }}, reorg: {{ enabled: false, min_gain_s: 0.5 }} }}\n"
        ),
    )
    .unwrap();
    let path = dir.join("plant.yaml");
    let a = app::start(Config::load_base(&path).unwrap(), Config::load(&path).unwrap()).await.unwrap();
    let w = a.warehouse("W").unwrap();
    a.db.write("K1.Auto", Value::Bool(true)).await.unwrap();
    crane_plc(a.clone(), "K1");
    tokio::time::sleep(Duration::from_millis(100)).await;
    let zone = |lu: &str| {
        let (p, _) = w.find(lu).unwrap();
        w.location_detail(p).unwrap()["zone"].as_str().unwrap().to_string()
    };
    let idle = || async {
        assert!(wait(|| w.missions().is_empty()).await, "missioni non concluse: {:?}", w.missions());
    };
    let info = |sku: &str| LuInfo { sku: sku.into(), qty: 10.0, ..Default::default() };

    // articoli senza storico prelievi = classe C → zona C; UdC senza articolo → cella più vicina
    w.allocate_inbound_with("F1", "IN", info("F"), &[], false).unwrap();
    idle().await;
    assert_eq!(zone("F1"), "C");
    w.allocate_inbound("N1", "IN", &[], false).unwrap();
    idle().await;
    assert_eq!(zone("N1"), "A");
    for lu in ["F2", "F3"] {
        tokio::time::sleep(Duration::from_millis(5)).await;
        w.allocate_inbound_with(lu, "IN", info("F"), &[], false).unwrap();
    }
    idle().await;

    // ordine 15 pz di F: due UdC intere, le più vecchie (FIFO)
    let o = w.create_order(OrderReq { reference: "T1".into(), bay: None, priority: None, lines: vec![OrderLineReq { sku: "F".into(), qty: 15.0 }] }, &[]).unwrap();
    assert_eq!(o.missions.len(), 2);
    let ms = w.missions();
    let mut lus: Vec<&str> = ms.iter().filter(|m| m.order == Some(o.id)).map(|m| m.lu.as_str()).collect();
    lus.sort();
    assert_eq!(lus, ["F1", "F2"]);
    idle().await;
    let o = w.orders().into_iter().find(|x| x.id == o.id).unwrap();
    assert_eq!(o.state, wcs_scada::warehouse::OrderState::Done);
    assert_eq!(o.lines[0].picked, 20.0);
    // ordine senza giacenza: in attesa
    let o2 = w.create_order(OrderReq { reference: String::new(), bay: None, priority: None, lines: vec![OrderLineReq { sku: "ZZ".into(), qty: 1.0 }] }, &[]).unwrap();
    assert_eq!(o2.state, wcs_scada::warehouse::OrderState::Waiting);
    w.order_action(o2.id, "close", None).unwrap();

    // dopo i prelievi F è classe A: la riorganizzazione porta F3 in zona A
    w.refresh_abc();
    assert_eq!(zone("F3"), "C");
    let plan = w.plan(None, 10);
    assert!(!plan.moves.is_empty());
    assert!(plan.moves[0].lu == "F3" && plan.moves[0].reason.starts_with("ABC"), "{:?}", plan.moves[0]);
    assert!(plan.after.misplaced < plan.before.misplaced);
    let created = w.reorg_now(None, 1, &[]);
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].kind, wcs_scada::warehouse::MissionKind::Move);
    idle().await;
    assert_eq!(zone("F3"), "A");
    assert_eq!(a.db.value("K1.Type"), Value::Int(3));

    // priorità e sospensione: con il traslo fermo si accodano tre prelievi
    a.db.write("K1.Fault", Value::Bool(true)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    for lu in ["P1", "P2", "P3"] {
        w.allocate_inbound(lu, "IN", &[], false).unwrap();
    }
    a.db.write("K1.Fault", Value::Bool(false)).await.unwrap();
    idle().await;
    a.db.write("K1.Fault", Value::Bool(true)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let m1 = w.create_outbound(Some("P1"), None, &[]).unwrap();
    let m2 = w.create_outbound_prio(Some("P2"), None, &[], 9).unwrap();
    let m3 = w.create_outbound(Some("P3"), None, &[]).unwrap();
    w.mission_action(m2.id, "hold", None).unwrap();
    w.mission_action(m3.id, "priority", Some(8)).unwrap();
    a.db.write("K1.Fault", Value::Bool(false)).await.unwrap();
    assert!(wait(|| a.db.value("K1.Mission") == Value::Int(m3.id as i64)).await, "prima la priorità 8");
    assert!(wait(|| a.db.value("K1.Mission") == Value::Int(m1.id as i64)).await, "poi la 5 (la 9 è sospesa)");
    assert!(wait(|| w.missions().len() == 1).await);
    w.mission_action(m2.id, "release", None).unwrap();
    idle().await;
    let k = w.kpi();
    assert!(k["cranes"][0]["out"].as_u64().unwrap() >= 5);
    assert!(k["cranes"][0]["move"].as_u64().unwrap() >= 1);
    assert!(k["cranes"][0]["utilization"].as_f64().unwrap() > 0.0, "{k}");
}
